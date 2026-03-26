use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::chunk::chunk_data;
use crate::hash::ContentHash;
use crate::manifest::{Manifest, ManifestError};

const COMPRESSION_THRESHOLD: usize = 512;
const ZSTD_LEVEL: i32 = 3;
// zstd frame magic number: 0xFD2FB528 (little-endian)
const ZSTD_MAGIC: [u8; 4] = [0x28, 0xB5, 0x2F, 0xFD];

/// On-disk content-addressable store.
pub struct ContentStore {
    objects_dir: PathBuf,
    manifests_dir: PathBuf,
}

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("chunk not found: {0}")]
    ChunkNotFound(ContentHash),
    #[error("manifest not found: {0}")]
    ManifestNotFound(ContentHash),
    #[error("manifest error: {0}")]
    Manifest(#[from] ManifestError),
    #[error("zstd decompression failed: {0}")]
    Decompression(String),
}

impl ContentStore {
    /// Open or create a content store at the given root directory.
    pub fn open(root: &Path) -> Result<Self, StoreError> {
        let objects_dir = root.join("objects");
        let manifests_dir = root.join("manifests");
        fs::create_dir_all(&objects_dir)?;
        fs::create_dir_all(&manifests_dir)?;
        Ok(ContentStore {
            objects_dir,
            manifests_dir,
        })
    }

    /// Store a chunk. Returns its content hash. Deduplicates: if the chunk
    /// already exists, returns the hash without writing.
    /// Chunks > 512 bytes are zstd-compressed on disk.
    pub fn put_chunk(&self, data: &[u8]) -> Result<ContentHash, StoreError> {
        let hash = ContentHash::from_bytes(data);
        let path = self.chunk_path(&hash);
        if path.exists() {
            return Ok(hash);
        }
        fs::create_dir_all(path.parent().unwrap())?;
        let stored = if data.len() > COMPRESSION_THRESHOLD {
            zstd::encode_all(data, ZSTD_LEVEL)
                .map_err(|e| StoreError::Decompression(e.to_string()))?
        } else {
            data.to_vec()
        };
        let mut file = fs::File::create(&path)?;
        file.write_all(&stored)?;
        Ok(hash)
    }

    /// Retrieve a chunk by its hash.
    pub fn get_chunk(&self, hash: &ContentHash) -> Result<Vec<u8>, StoreError> {
        let raw = self.read_raw_chunk(hash)?;
        if raw.len() >= 4 && raw[..4] == ZSTD_MAGIC {
            zstd::decode_all(raw.as_slice())
                .map_err(|e| StoreError::Decompression(e.to_string()))
        } else {
            Ok(raw)
        }
    }

    /// Check if a chunk exists in the store.
    pub fn has_chunk(&self, hash: &ContentHash) -> bool {
        self.chunk_path(hash).exists()
    }

    /// Read the raw bytes of a chunk from disk (possibly compressed).
    pub fn read_raw_chunk(&self, hash: &ContentHash) -> Result<Vec<u8>, StoreError> {
        let path = self.chunk_path(hash);
        if !path.exists() {
            return Err(StoreError::ChunkNotFound(*hash));
        }
        Ok(fs::read(&path)?)
    }

    /// Store a manifest. Returns the manifest's content hash.
    pub fn put_manifest(&self, manifest: &Manifest) -> Result<ContentHash, StoreError> {
        let hash = manifest.hash();
        let path = self.manifest_path(&hash);
        if path.exists() {
            return Ok(hash);
        }
        fs::create_dir_all(path.parent().unwrap())?;
        let bytes = manifest.to_bytes();
        fs::write(&path, &bytes)?;
        Ok(hash)
    }

    /// Retrieve a manifest by its hash.
    pub fn get_manifest(&self, hash: &ContentHash) -> Result<Manifest, StoreError> {
        let path = self.manifest_path(hash);
        if !path.exists() {
            return Err(StoreError::ManifestNotFound(*hash));
        }
        let bytes = fs::read(&path)?;
        Ok(Manifest::from_bytes(&bytes)?)
    }

    /// Ingest a file: chunk it, store all chunks, create and store a manifest.
    /// Returns the manifest hash.
    pub fn put_file(&self, path: &Path) -> Result<ContentHash, StoreError> {
        let data = fs::read(path)?;
        let metadata = fs::metadata(path)?;
        let mode = {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                metadata.permissions().mode()
            }
            #[cfg(not(unix))]
            {
                0o644
            }
        };

        let chunks = chunk_data(&data);
        let mut chunk_hashes = Vec::with_capacity(chunks.len());
        for chunk in chunks {
            let hash = self.put_chunk(chunk)?;
            chunk_hashes.push(hash);
        }

        let manifest = Manifest::new(chunk_hashes, data.len() as u64, mode);
        self.put_manifest(&manifest)
    }

    /// Reconstruct a file from a manifest hash and write it to the destination path.
    pub fn get_file(&self, manifest_hash: &ContentHash, dest: &Path) -> Result<(), StoreError> {
        let manifest = self.get_manifest(manifest_hash)?;
        let mut file = fs::File::create(dest)?;
        for chunk_hash in &manifest.chunks {
            let data = self.get_chunk(chunk_hash)?;
            file.write_all(&data)?;
        }

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(dest, fs::Permissions::from_mode(manifest.mode))?;
        }

        Ok(())
    }

    fn chunk_path(&self, hash: &ContentHash) -> PathBuf {
        let hex = hash.to_hex();
        self.objects_dir.join(&hex[..2]).join(&hex[2..])
    }

    fn manifest_path(&self, hash: &ContentHash) -> PathBuf {
        let hex = hash.to_hex();
        self.manifests_dir.join(&hex[..2]).join(&hex[2..])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn test_store() -> (ContentStore, TempDir) {
        let dir = TempDir::new().unwrap();
        let store = ContentStore::open(dir.path()).unwrap();
        (store, dir)
    }

    #[test]
    fn put_and_get_chunk() {
        let (store, _dir) = test_store();
        let data = b"hello world";
        let hash = store.put_chunk(data).unwrap();
        let retrieved = store.get_chunk(&hash).unwrap();
        assert_eq!(retrieved, data);
    }

    #[test]
    fn has_chunk_returns_false_for_missing() {
        let (store, _dir) = test_store();
        let fake_hash = crate::hash::ContentHash::from_bytes(b"nonexistent");
        assert!(!store.has_chunk(&fake_hash));
    }

    #[test]
    fn put_chunk_deduplicates() {
        let (store, _dir) = test_store();
        let data = b"duplicate data";
        let h1 = store.put_chunk(data).unwrap();
        let h2 = store.put_chunk(data).unwrap();
        assert_eq!(h1, h2);
    }

    #[test]
    fn put_and_get_manifest() {
        let (store, _dir) = test_store();
        let chunk_hash = store.put_chunk(b"some data").unwrap();
        let manifest = crate::manifest::Manifest::new(vec![chunk_hash], 9, 0o644);
        let manifest_hash = store.put_manifest(&manifest).unwrap();
        let retrieved = store.get_manifest(&manifest_hash).unwrap();
        assert_eq!(manifest.hash(), retrieved.hash());
    }

    #[test]
    fn put_file_end_to_end() {
        let (store, dir) = test_store();
        let file_path = dir.path().join("testfile.txt");
        std::fs::write(&file_path, b"file content for testing").unwrap();
        let manifest_hash = store.put_file(&file_path).unwrap();
        let out_path = dir.path().join("output.txt");
        store.get_file(&manifest_hash, &out_path).unwrap();
        assert_eq!(
            std::fs::read(&file_path).unwrap(),
            std::fs::read(&out_path).unwrap(),
        );
    }

    #[test]
    fn put_file_large_produces_multiple_chunks() {
        let (store, dir) = test_store();
        let file_path = dir.path().join("large.bin");
        let data: Vec<u8> = (0..256 * 1024).map(|i| (i % 251) as u8).collect();
        std::fs::write(&file_path, &data).unwrap();
        let manifest_hash = store.put_file(&file_path).unwrap();
        let manifest = store.get_manifest(&manifest_hash).unwrap();
        assert!(manifest.chunks.len() > 1);
        let out_path = dir.path().join("large_out.bin");
        store.get_file(&manifest_hash, &out_path).unwrap();
        assert_eq!(std::fs::read(&file_path).unwrap(), std::fs::read(&out_path).unwrap());
    }

    #[test]
    fn small_chunks_not_compressed() {
        let (store, _dir) = test_store();
        let data = b"tiny chunk";
        let hash = store.put_chunk(data).unwrap();
        let raw = store.read_raw_chunk(&hash).unwrap();
        assert_eq!(raw, data.as_slice(), "small chunks should be stored uncompressed");
    }

    #[test]
    fn large_chunks_compressed() {
        let (store, _dir) = test_store();
        let data = vec![0xAA; 1024];
        let hash = store.put_chunk(&data).unwrap();
        let raw = store.read_raw_chunk(&hash).unwrap();
        assert!(raw.len() < data.len(), "large chunks should be compressed");
        let retrieved = store.get_chunk(&hash).unwrap();
        assert_eq!(retrieved, data);
    }
}

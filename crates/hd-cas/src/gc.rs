use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use crate::hash::ContentHash;
use crate::store::ContentStore;

/// Garbage collection statistics.
#[derive(Debug, Default)]
pub struct GcStats {
    pub manifests_removed: usize,
    pub chunks_removed: usize,
}

/// Reference-counting garbage collector for the CAS.
/// Ref counts are stored as simple files: refs/<shard>/<hash> contains the count as a u64.
pub struct GarbageCollector {
    refs_dir: PathBuf,
}

#[derive(Debug, thiserror::Error)]
pub enum GcError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("store error: {0}")]
    Store(#[from] crate::store::StoreError),
}

impl GarbageCollector {
    pub fn new(cas_root: &Path) -> Result<Self, GcError> {
        let refs_dir = cas_root.join("refs");
        fs::create_dir_all(&refs_dir)?;
        Ok(GarbageCollector { refs_dir })
    }

    pub fn add_ref(&self, manifest_hash: &ContentHash) -> Result<(), GcError> {
        let count = self.ref_count(manifest_hash).unwrap_or(0);
        self.write_ref_count(manifest_hash, count + 1)
    }

    pub fn remove_ref(&self, manifest_hash: &ContentHash) -> Result<(), GcError> {
        let count = self.ref_count(manifest_hash).unwrap_or(0);
        if count <= 1 {
            let path = self.ref_path(manifest_hash);
            if path.exists() {
                fs::remove_file(&path)?;
            }
        } else {
            self.write_ref_count(manifest_hash, count - 1)?;
        }
        Ok(())
    }

    pub fn ref_count(&self, manifest_hash: &ContentHash) -> Result<u64, GcError> {
        let path = self.ref_path(manifest_hash);
        if !path.exists() {
            return Ok(0);
        }
        let bytes = fs::read(&path)?;
        let count = u64::from_le_bytes(bytes.try_into().unwrap_or([0; 8]));
        Ok(count)
    }

    pub fn collect(&self, store: &ContentStore) -> Result<GcStats, GcError> {
        let mut stats = GcStats::default();
        let referenced_manifests = self.all_referenced_manifests()?;
        let mut referenced_chunks = HashSet::new();
        let mut manifests_to_remove = Vec::new();

        for manifest_hash in store.list_manifests()? {
            if referenced_manifests.contains(&manifest_hash) {
                if let Ok(manifest) = store.get_manifest(&manifest_hash) {
                    for chunk_hash in &manifest.chunks {
                        referenced_chunks.insert(*chunk_hash);
                    }
                }
            } else {
                manifests_to_remove.push(manifest_hash);
            }
        }

        for mhash in &manifests_to_remove {
            store.remove_manifest(mhash)?;
            stats.manifests_removed += 1;
        }

        for chunk_hash in store.list_chunks()? {
            if !referenced_chunks.contains(&chunk_hash) {
                store.remove_chunk(&chunk_hash)?;
                stats.chunks_removed += 1;
            }
        }

        Ok(stats)
    }

    fn ref_path(&self, hash: &ContentHash) -> PathBuf {
        let hex = hash.to_hex();
        self.refs_dir.join(&hex[..2]).join(&hex[2..])
    }

    fn write_ref_count(&self, hash: &ContentHash, count: u64) -> Result<(), GcError> {
        let path = self.ref_path(hash);
        fs::create_dir_all(path.parent().unwrap())?;
        fs::write(&path, count.to_le_bytes())?;
        Ok(())
    }

    fn all_referenced_manifests(&self) -> Result<HashSet<ContentHash>, GcError> {
        let mut set = HashSet::new();
        if !self.refs_dir.exists() {
            return Ok(set);
        }
        for shard_entry in fs::read_dir(&self.refs_dir)? {
            let shard_entry = shard_entry?;
            if !shard_entry.file_type()?.is_dir() {
                continue;
            }
            let shard = shard_entry.file_name().to_string_lossy().to_string();
            for entry in fs::read_dir(shard_entry.path())? {
                let entry = entry?;
                let rest = entry.file_name().to_string_lossy().to_string();
                let hex = format!("{}{}", shard, rest);
                if let Ok(hash) = ContentHash::from_hex(&hex) {
                    let bytes = fs::read(entry.path())?;
                    let count = u64::from_le_bytes(bytes.try_into().unwrap_or([0; 8]));
                    if count > 0 {
                        set.insert(hash);
                    }
                }
            }
        }
        Ok(set)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::ContentStore;
    use tempfile::TempDir;

    fn test_gc() -> (GarbageCollector, ContentStore, TempDir) {
        let dir = TempDir::new().unwrap();
        let store = ContentStore::open(dir.path()).unwrap();
        let gc = GarbageCollector::new(dir.path()).unwrap();
        (gc, store, dir)
    }

    #[test]
    fn ref_count_increment_and_decrement() {
        let (gc, store, _dir) = test_gc();
        let hash = store.put_chunk(b"data").unwrap();
        let manifest = crate::manifest::Manifest::new(vec![hash], 4, 0o644);
        let mhash = store.put_manifest(&manifest).unwrap();

        gc.add_ref(&mhash).unwrap();
        assert_eq!(gc.ref_count(&mhash).unwrap(), 1);

        gc.add_ref(&mhash).unwrap();
        assert_eq!(gc.ref_count(&mhash).unwrap(), 2);

        gc.remove_ref(&mhash).unwrap();
        assert_eq!(gc.ref_count(&mhash).unwrap(), 1);
    }

    #[test]
    fn gc_removes_unreferenced_manifests_and_chunks() {
        let (gc, store, _dir) = test_gc();
        let hash = store.put_chunk(b"orphan data").unwrap();
        let manifest = crate::manifest::Manifest::new(vec![hash], 11, 0o644);
        let _mhash = store.put_manifest(&manifest).unwrap();

        assert!(store.has_chunk(&hash));
        let stats = gc.collect(&store).unwrap();
        assert_eq!(stats.manifests_removed, 1);
        assert_eq!(stats.chunks_removed, 1);
        assert!(!store.has_chunk(&hash));
    }

    #[test]
    fn gc_preserves_referenced_data() {
        let (gc, store, _dir) = test_gc();
        let hash = store.put_chunk(b"keep me").unwrap();
        let manifest = crate::manifest::Manifest::new(vec![hash], 7, 0o644);
        let mhash = store.put_manifest(&manifest).unwrap();

        gc.add_ref(&mhash).unwrap();
        let stats = gc.collect(&store).unwrap();
        assert_eq!(stats.manifests_removed, 0);
        assert_eq!(stats.chunks_removed, 0);
        assert!(store.has_chunk(&hash));
    }

    #[test]
    fn gc_shared_chunks_preserved() {
        let (gc, store, _dir) = test_gc();
        let shared_chunk = store.put_chunk(b"shared").unwrap();

        let m1 = crate::manifest::Manifest::new(vec![shared_chunk], 6, 0o644);
        let mh1 = store.put_manifest(&m1).unwrap();
        gc.add_ref(&mh1).unwrap();

        let unique_chunk = store.put_chunk(b"unique").unwrap();
        let m2 = crate::manifest::Manifest::new(vec![shared_chunk, unique_chunk], 12, 0o644);
        let _mh2 = store.put_manifest(&m2).unwrap();

        let stats = gc.collect(&store).unwrap();
        assert_eq!(stats.manifests_removed, 1);
        assert_eq!(stats.chunks_removed, 1);
        assert!(store.has_chunk(&shared_chunk));
        assert!(!store.has_chunk(&unique_chunk));
    }
}

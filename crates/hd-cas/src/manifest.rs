use serde::{Deserialize, Serialize};

use crate::hash::ContentHash;

/// A file manifest: an ordered list of chunk hashes plus file metadata.
/// The manifest's own hash is derived from its contents (not stored inside the struct).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub chunks: Vec<ContentHash>,
    pub size: u64,
    pub mode: u32,
}

impl Manifest {
    pub fn new(chunks: Vec<ContentHash>, size: u64, mode: u32) -> Self {
        Manifest { chunks, size, mode }
    }

    /// Compute the content hash of this manifest.
    /// Hash = BLAKE3(all chunk hashes concatenated + size as le bytes + mode as le bytes).
    pub fn hash(&self) -> ContentHash {
        let mut hasher = blake3::Hasher::new();
        for chunk_hash in &self.chunks {
            hasher.update(chunk_hash.as_bytes());
        }
        hasher.update(&self.size.to_le_bytes());
        hasher.update(&self.mode.to_le_bytes());
        ContentHash::from_raw(*hasher.finalize().as_bytes())
    }

    /// Serialize to bincode bytes.
    pub fn to_bytes(&self) -> Vec<u8> {
        bincode::serde::encode_to_vec(self, bincode::config::standard()).unwrap()
    }

    /// Deserialize from bincode bytes.
    pub fn from_bytes(data: &[u8]) -> Result<Self, ManifestError> {
        let (manifest, _) = bincode::serde::decode_from_slice(data, bincode::config::standard())
            .map_err(|e| ManifestError::DeserializationFailed(e.to_string()))?;
        Ok(manifest)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ManifestError {
    #[error("manifest deserialization failed: {0}")]
    DeserializationFailed(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hash::ContentHash;

    #[test]
    fn manifest_hash_deterministic() {
        let chunks = vec![
            ContentHash::from_bytes(b"chunk1"),
            ContentHash::from_bytes(b"chunk2"),
        ];
        let m1 = Manifest::new(chunks.clone(), 1000, 0o644);
        let m2 = Manifest::new(chunks, 1000, 0o644);
        assert_eq!(m1.hash(), m2.hash());
    }

    #[test]
    fn manifest_hash_changes_with_chunks() {
        let m1 = Manifest::new(
            vec![ContentHash::from_bytes(b"a")],
            100,
            0o644,
        );
        let m2 = Manifest::new(
            vec![ContentHash::from_bytes(b"b")],
            100,
            0o644,
        );
        assert_ne!(m1.hash(), m2.hash());
    }

    #[test]
    fn manifest_hash_changes_with_mode() {
        let chunks = vec![ContentHash::from_bytes(b"same")];
        let m1 = Manifest::new(chunks.clone(), 100, 0o644);
        let m2 = Manifest::new(chunks, 100, 0o755);
        assert_ne!(m1.hash(), m2.hash());
    }

    #[test]
    fn manifest_serialization_roundtrip() {
        let chunks = vec![
            ContentHash::from_bytes(b"chunk1"),
            ContentHash::from_bytes(b"chunk2"),
        ];
        let m = Manifest::new(chunks, 2000, 0o644);
        let bytes = m.to_bytes();
        let m2 = Manifest::from_bytes(&bytes).unwrap();
        assert_eq!(m.hash(), m2.hash());
        assert_eq!(m.chunks, m2.chunks);
        assert_eq!(m.size, m2.size);
        assert_eq!(m.mode, m2.mode);
    }
}

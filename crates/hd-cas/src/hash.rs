use serde::{Deserialize, Serialize};
use std::fmt;

/// A BLAKE3 content hash. 32 bytes (256 bits).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
pub struct ContentHash([u8; 32]);

impl ContentHash {
    /// Hash arbitrary bytes with BLAKE3.
    pub fn from_bytes(data: &[u8]) -> Self {
        let hash = blake3::hash(data);
        ContentHash(*hash.as_bytes())
    }

    /// Wrap raw hash bytes (no rehashing). Use when you already have
    /// a finalized BLAKE3 hash (e.g., from a streaming Hasher).
    pub fn from_raw(bytes: [u8; 32]) -> Self {
        ContentHash(bytes)
    }

    /// Return the hash as a 64-character hex string.
    pub fn to_hex(&self) -> String {
        hex_encode(&self.0)
    }

    /// Parse a 64-character hex string into a ContentHash.
    pub fn from_hex(hex: &str) -> Result<Self, HashError> {
        if hex.len() != 64 {
            return Err(HashError::InvalidHexLength(hex.len()));
        }
        let mut bytes = [0u8; 32];
        for i in 0..32 {
            bytes[i] = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16)
                .map_err(|_| HashError::InvalidHexChar)?;
        }
        Ok(ContentHash(bytes))
    }

    /// First two hex characters, used for directory sharding.
    pub fn shard_prefix(&self) -> String {
        format!("{:02x}", self.0[0])
    }

    /// Access the raw 32-byte hash.
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl fmt::Debug for ContentHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ContentHash({})", &self.to_hex()[..12])
    }
}

impl fmt::Display for ContentHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_hex())
    }
}

fn hex_encode(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{:02x}", b));
    }
    s
}

#[derive(Debug, thiserror::Error)]
pub enum HashError {
    #[error("invalid hex length: expected 64, got {0}")]
    InvalidHexLength(usize),
    #[error("invalid hex character")]
    InvalidHexChar,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_bytes_deterministic() {
        let data = b"hello world";
        let h1 = ContentHash::from_bytes(data);
        let h2 = ContentHash::from_bytes(data);
        assert_eq!(h1, h2);
    }

    #[test]
    fn hash_bytes_different_input_different_hash() {
        let h1 = ContentHash::from_bytes(b"hello");
        let h2 = ContentHash::from_bytes(b"world");
        assert_ne!(h1, h2);
    }

    #[test]
    fn hash_hex_roundtrip() {
        let h = ContentHash::from_bytes(b"test data");
        let hex = h.to_hex();
        let parsed = ContentHash::from_hex(&hex).unwrap();
        assert_eq!(h, parsed);
    }

    #[test]
    fn hash_shard_prefix() {
        let h = ContentHash::from_bytes(b"test");
        let prefix = h.shard_prefix();
        assert_eq!(prefix.len(), 2);
        assert_eq!(prefix, &h.to_hex()[..2]);
    }

    #[test]
    fn from_raw_does_not_rehash() {
        let hash = blake3::hash(b"test");
        let h = ContentHash::from_raw(*hash.as_bytes());
        assert_eq!(h.as_bytes(), hash.as_bytes());
        let h2 = ContentHash::from_bytes(hash.as_bytes());
        assert_ne!(h, h2);
    }
}

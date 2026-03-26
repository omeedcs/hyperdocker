# hd-cas & hd-engine Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the two foundational crates of hyperdocker — the content-addressable store (`hd-cas`) and the Merkle DAG engine (`hd-engine`) — with full test coverage.

**Architecture:** `hd-cas` provides BLAKE3-hashed, CDC-chunked content storage with dedup and GC. `hd-engine` builds on `hd-cas` to represent environments as a Merkle DAG with deterministic hashing and bottom-up incremental invalidation. Both crates are pure libraries with no I/O dependencies beyond the filesystem.

**Tech Stack:** Rust (2021 edition), Cargo workspace, blake3, fastcdc, zstd, serde, bincode, rayon, tempfile (dev)

---

## File Structure

```
hyperdocker/
  Cargo.toml                          # workspace root
  crates/
    hd-cas/
      Cargo.toml
      src/
        lib.rs                        # public API re-exports
        hash.rs                       # BLAKE3 hashing, ContentHash type
        chunk.rs                      # FastCDC chunking
        manifest.rs                   # file manifests (ordered chunk list + metadata)
        store.rs                      # on-disk CAS (put/get chunks, compression, sharding)
        gc.rs                         # garbage collection (reference counting, sweep)
      tests/
        integration.rs                # end-to-end CAS tests
    hd-engine/
      Cargo.toml
      src/
        lib.rs                        # public API re-exports
        node.rs                       # DAG node types (FileNode, DirNode, PackageNode, BuildStepNode, EnvNode)
        dag.rs                        # DAG construction, query, storage
        invalidation.rs               # bottom-up incremental invalidation
        diff.rs                       # DAG diffing between two EnvNodes
      tests/
        integration.rs                # end-to-end DAG tests
```

---

## Task 1: Workspace Setup

**Files:**
- Create: `Cargo.toml` (workspace root)
- Create: `crates/hd-cas/Cargo.toml`
- Create: `crates/hd-cas/src/lib.rs`
- Create: `crates/hd-engine/Cargo.toml`
- Create: `crates/hd-engine/src/lib.rs`

- [ ] **Step 1: Create workspace root Cargo.toml**

```toml
[workspace]
members = ["crates/hd-cas", "crates/hd-engine"]
resolver = "2"

[workspace.package]
edition = "2021"
license = "MIT"
version = "0.1.0"

[workspace.dependencies]
blake3 = "1.6"
fastcdc = "3.1"
zstd = "0.13"
serde = { version = "1.0", features = ["derive"] }
bincode = "2.0"
rayon = "1.10"
tempfile = "3.15"
thiserror = "2.0"
```

- [ ] **Step 2: Create hd-cas crate**

```toml
# crates/hd-cas/Cargo.toml
[package]
name = "hd-cas"
edition.workspace = true
license.workspace = true
version.workspace = true

[dependencies]
blake3.workspace = true
fastcdc.workspace = true
zstd.workspace = true
serde.workspace = true
bincode.workspace = true
rayon.workspace = true
thiserror.workspace = true

[dev-dependencies]
tempfile.workspace = true
```

```rust
// crates/hd-cas/src/lib.rs
pub mod hash;
pub mod chunk;
pub mod manifest;
pub mod store;
pub mod gc;
```

- [ ] **Step 3: Create hd-engine crate**

```toml
# crates/hd-engine/Cargo.toml
[package]
name = "hd-engine"
edition.workspace = true
license.workspace = true
version.workspace = true

[dependencies]
hd-cas = { path = "../hd-cas" }
blake3.workspace = true
serde.workspace = true
bincode.workspace = true
rayon.workspace = true
thiserror.workspace = true

[dev-dependencies]
tempfile.workspace = true
```

```rust
// crates/hd-engine/src/lib.rs
pub mod node;
pub mod dag;
pub mod invalidation;
pub mod diff;
```

- [ ] **Step 4: Create placeholder source files**

Create empty module files so the workspace compiles:

```rust
// crates/hd-cas/src/hash.rs
// crates/hd-cas/src/chunk.rs
// crates/hd-cas/src/manifest.rs
// crates/hd-cas/src/store.rs
// crates/hd-cas/src/gc.rs
// (each file is empty)
```

```rust
// crates/hd-engine/src/node.rs
// crates/hd-engine/src/dag.rs
// crates/hd-engine/src/invalidation.rs
// crates/hd-engine/src/diff.rs
// (each file is empty)
```

- [ ] **Step 5: Verify workspace compiles**

Run: `cargo build`
Expected: Compiles with no errors (may have unused warnings, that's fine).

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml crates/
git commit -m "feat: initialize cargo workspace with hd-cas and hd-engine crates"
```

---

## Task 2: ContentHash Type (`hd-cas/src/hash.rs`)

**Files:**
- Create: `crates/hd-cas/src/hash.rs`
- Test: `crates/hd-cas/src/hash.rs` (inline tests)

- [ ] **Step 1: Write the failing test**

```rust
// crates/hd-cas/src/hash.rs

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
        // from_raw wraps the bytes directly — should equal the blake3 output
        assert_eq!(h.as_bytes(), hash.as_bytes());
        // from_bytes would double-hash, so it should differ
        let h2 = ContentHash::from_bytes(hash.as_bytes());
        assert_ne!(h, h2);
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p hd-cas hash::tests`
Expected: FAIL — `ContentHash` not defined.

- [ ] **Step 3: Write the implementation**

```rust
// crates/hd-cas/src/hash.rs
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
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p hd-cas hash::tests`
Expected: 5 tests PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/hd-cas/src/hash.rs
git commit -m "feat(hd-cas): add ContentHash type with BLAKE3 hashing"
```

---

## Task 3: Content-Defined Chunking (`hd-cas/src/chunk.rs`)

**Files:**
- Create: `crates/hd-cas/src/chunk.rs`
- Test: `crates/hd-cas/src/chunk.rs` (inline tests)

- [ ] **Step 1: Write the failing tests**

```rust
// crates/hd-cas/src/chunk.rs

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunk_small_file_single_chunk() {
        let data = b"small file content";
        let chunks = chunk_data(data);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0], data.as_slice());
    }

    #[test]
    fn chunk_empty_data() {
        let chunks = chunk_data(b"");
        assert!(chunks.is_empty());
    }

    #[test]
    fn chunk_large_file_multiple_chunks() {
        // 256KB of data should produce multiple chunks with 16KB target
        let data = vec![0xAB; 256 * 1024];
        let chunks = chunk_data(&data);
        assert!(chunks.len() > 1, "expected multiple chunks, got {}", chunks.len());
        // all chunks within size bounds
        for chunk in &chunks {
            assert!(chunk.len() <= MAX_CHUNK_SIZE);
        }
    }

    #[test]
    fn chunk_reassembly() {
        let data: Vec<u8> = (0..100_000).map(|i| (i % 251) as u8).collect();
        let chunks = chunk_data(&data);
        let reassembled: Vec<u8> = chunks.iter().flat_map(|c| c.iter().copied()).collect();
        assert_eq!(reassembled, data);
    }

    #[test]
    fn chunk_deterministic() {
        let data: Vec<u8> = (0..100_000).map(|i| (i % 251) as u8).collect();
        let chunks1 = chunk_data(&data);
        let chunks2 = chunk_data(&data);
        assert_eq!(chunks1.len(), chunks2.len());
        for (a, b) in chunks1.iter().zip(chunks2.iter()) {
            assert_eq!(a, b);
        }
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p hd-cas chunk::tests`
Expected: FAIL — `chunk_data` not defined.

- [ ] **Step 3: Write the implementation**

```rust
// crates/hd-cas/src/chunk.rs
use fastcdc::v2020::FastCDC;

pub const MIN_CHUNK_SIZE: usize = 4 * 1024;       // 4 KB
pub const TARGET_CHUNK_SIZE: usize = 16 * 1024;   // 16 KB
pub const MAX_CHUNK_SIZE: usize = 64 * 1024;      // 64 KB

/// Split data into content-defined chunks using FastCDC.
/// Files smaller than MIN_CHUNK_SIZE are returned as a single chunk.
/// Empty data returns an empty vec.
pub fn chunk_data(data: &[u8]) -> Vec<&[u8]> {
    if data.is_empty() {
        return Vec::new();
    }
    if data.len() <= MIN_CHUNK_SIZE {
        return vec![data];
    }
    let chunker = FastCDC::new(data, MIN_CHUNK_SIZE as u32, TARGET_CHUNK_SIZE as u32, MAX_CHUNK_SIZE as u32);
    chunker
        .map(|chunk| &data[chunk.offset..chunk.offset + chunk.length])
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunk_small_file_single_chunk() {
        let data = b"small file content";
        let chunks = chunk_data(data);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0], data.as_slice());
    }

    #[test]
    fn chunk_empty_data() {
        let chunks = chunk_data(b"");
        assert!(chunks.is_empty());
    }

    #[test]
    fn chunk_large_file_multiple_chunks() {
        let data = vec![0xAB; 256 * 1024];
        let chunks = chunk_data(&data);
        assert!(chunks.len() > 1, "expected multiple chunks, got {}", chunks.len());
        for chunk in &chunks {
            assert!(chunk.len() <= MAX_CHUNK_SIZE);
        }
    }

    #[test]
    fn chunk_reassembly() {
        let data: Vec<u8> = (0..100_000).map(|i| (i % 251) as u8).collect();
        let chunks = chunk_data(&data);
        let reassembled: Vec<u8> = chunks.iter().flat_map(|c| c.iter().copied()).collect();
        assert_eq!(reassembled, data);
    }

    #[test]
    fn chunk_deterministic() {
        let data: Vec<u8> = (0..100_000).map(|i| (i % 251) as u8).collect();
        let chunks1 = chunk_data(&data);
        let chunks2 = chunk_data(&data);
        assert_eq!(chunks1.len(), chunks2.len());
        for (a, b) in chunks1.iter().zip(chunks2.iter()) {
            assert_eq!(a, b);
        }
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p hd-cas chunk::tests`
Expected: 5 tests PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/hd-cas/src/chunk.rs
git commit -m "feat(hd-cas): add FastCDC content-defined chunking"
```

---

## Task 4: File Manifests (`hd-cas/src/manifest.rs`)

**Files:**
- Create: `crates/hd-cas/src/manifest.rs`
- Test: `crates/hd-cas/src/manifest.rs` (inline tests)

- [ ] **Step 1: Write the failing tests**

```rust
// crates/hd-cas/src/manifest.rs

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
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p hd-cas manifest::tests`
Expected: FAIL — `Manifest` not defined.

- [ ] **Step 3: Write the implementation**

```rust
// crates/hd-cas/src/manifest.rs
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
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p hd-cas manifest::tests`
Expected: 4 tests PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/hd-cas/src/manifest.rs
git commit -m "feat(hd-cas): add Manifest type with deterministic hashing and bincode serialization"
```

---

## Task 5: Content Store (`hd-cas/src/store.rs`)

**Files:**
- Create: `crates/hd-cas/src/store.rs`
- Test: `crates/hd-cas/src/store.rs` (inline tests)

- [ ] **Step 1: Write the failing tests**

```rust
// crates/hd-cas/src/store.rs

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
        // get_file to a different location
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
        // 256KB of varied data
        let data: Vec<u8> = (0..256 * 1024).map(|i| (i % 251) as u8).collect();
        std::fs::write(&file_path, &data).unwrap();
        let manifest_hash = store.put_file(&file_path).unwrap();
        let manifest = store.get_manifest(&manifest_hash).unwrap();
        assert!(manifest.chunks.len() > 1);
        // roundtrip
        let out_path = dir.path().join("large_out.bin");
        store.get_file(&manifest_hash, &out_path).unwrap();
        assert_eq!(std::fs::read(&file_path).unwrap(), std::fs::read(&out_path).unwrap());
    }

    #[test]
    fn small_chunks_not_compressed() {
        let (store, _dir) = test_store();
        // 10 bytes — below 512 byte threshold
        let data = b"tiny chunk";
        let hash = store.put_chunk(data).unwrap();
        // read raw file from disk — should not have zstd magic bytes
        let raw = store.read_raw_chunk(&hash).unwrap();
        assert_eq!(raw, data, "small chunks should be stored uncompressed");
    }

    #[test]
    fn large_chunks_compressed() {
        let (store, _dir) = test_store();
        // 1KB of compressible data
        let data = vec![0xAA; 1024];
        let hash = store.put_chunk(&data).unwrap();
        let raw = store.read_raw_chunk(&hash).unwrap();
        // compressed data should be smaller than original
        assert!(raw.len() < data.len(), "large chunks should be compressed");
        // but get_chunk should return decompressed data
        let retrieved = store.get_chunk(&hash).unwrap();
        assert_eq!(retrieved, data);
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p hd-cas store::tests`
Expected: FAIL — `ContentStore` struct not defined.

- [ ] **Step 3: Write the implementation**

```rust
// crates/hd-cas/src/store.rs
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
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p hd-cas store::tests`
Expected: 8 tests PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/hd-cas/src/store.rs
git commit -m "feat(hd-cas): add ContentStore with chunked storage, compression, and file ingestion"
```

---

## Task 6: Garbage Collection (`hd-cas/src/gc.rs`)

**Files:**
- Create: `crates/hd-cas/src/gc.rs`
- Modify: `crates/hd-cas/src/store.rs` (add ref counting methods)
- Test: `crates/hd-cas/src/gc.rs` (inline tests)

- [ ] **Step 1: Write the failing tests**

```rust
// crates/hd-cas/src/gc.rs

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
        let mhash = store.put_manifest(&manifest).unwrap();

        // manifest exists but has no refs
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

        // Two manifests referencing the same chunk
        let m1 = crate::manifest::Manifest::new(vec![shared_chunk], 6, 0o644);
        let mh1 = store.put_manifest(&m1).unwrap();
        gc.add_ref(&mh1).unwrap();

        let unique_chunk = store.put_chunk(b"unique").unwrap();
        let m2 = crate::manifest::Manifest::new(vec![shared_chunk, unique_chunk], 12, 0o644);
        let mh2 = store.put_manifest(&m2).unwrap();
        // m2 has no refs — eligible for GC

        let stats = gc.collect(&store).unwrap();
        assert_eq!(stats.manifests_removed, 1);
        // shared chunk preserved (still referenced by m1), only unique_chunk removed
        assert_eq!(stats.chunks_removed, 1);
        assert!(store.has_chunk(&shared_chunk));
        assert!(!store.has_chunk(&unique_chunk));
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p hd-cas gc::tests`
Expected: FAIL — `GarbageCollector` not defined.

- [ ] **Step 3: Write the implementation**

```rust
// crates/hd-cas/src/gc.rs
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

    /// Increment the reference count for a manifest.
    pub fn add_ref(&self, manifest_hash: &ContentHash) -> Result<(), GcError> {
        let count = self.ref_count(manifest_hash).unwrap_or(0);
        self.write_ref_count(manifest_hash, count + 1)
    }

    /// Decrement the reference count for a manifest.
    /// Does not go below 0.
    pub fn remove_ref(&self, manifest_hash: &ContentHash) -> Result<(), GcError> {
        let count = self.ref_count(manifest_hash).unwrap_or(0);
        if count <= 1 {
            // Remove the ref file entirely
            let path = self.ref_path(manifest_hash);
            if path.exists() {
                fs::remove_file(&path)?;
            }
        } else {
            self.write_ref_count(manifest_hash, count - 1)?;
        }
        Ok(())
    }

    /// Get the current reference count for a manifest.
    pub fn ref_count(&self, manifest_hash: &ContentHash) -> Result<u64, GcError> {
        let path = self.ref_path(manifest_hash);
        if !path.exists() {
            return Ok(0);
        }
        let bytes = fs::read(&path)?;
        let count = u64::from_le_bytes(bytes.try_into().unwrap_or([0; 8]));
        Ok(count)
    }

    /// Collect garbage: remove unreferenced manifests and orphaned chunks.
    pub fn collect(&self, store: &ContentStore) -> Result<GcStats, GcError> {
        let mut stats = GcStats::default();

        // 1. Find all manifest hashes that have refs
        let referenced_manifests = self.all_referenced_manifests()?;

        // 2. Walk all manifests in the store, collect referenced chunk hashes
        let mut referenced_chunks = HashSet::new();
        let mut manifests_to_remove = Vec::new();

        for manifest_hash in store.list_manifests()? {
            if referenced_manifests.contains(&manifest_hash) {
                // This manifest is referenced — preserve its chunks
                if let Ok(manifest) = store.get_manifest(&manifest_hash) {
                    for chunk_hash in &manifest.chunks {
                        referenced_chunks.insert(*chunk_hash);
                    }
                }
            } else {
                // Unreferenced manifest — mark for removal
                manifests_to_remove.push(manifest_hash);
            }
        }

        // 3. Remove unreferenced manifests
        for mhash in &manifests_to_remove {
            store.remove_manifest(mhash)?;
            stats.manifests_removed += 1;
        }

        // 4. Remove orphaned chunks
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
        fs::write(&path, &count.to_le_bytes())?;
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
                    // Only include if ref count > 0
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
```

- [ ] **Step 4: Add list and remove methods to ContentStore**

Add these methods to the `impl ContentStore` block in `crates/hd-cas/src/store.rs`:

```rust
    /// List all manifest hashes in the store.
    pub fn list_manifests(&self) -> Result<Vec<ContentHash>, StoreError> {
        Self::list_hashes(&self.manifests_dir)
    }

    /// List all chunk hashes in the store.
    pub fn list_chunks(&self) -> Result<Vec<ContentHash>, StoreError> {
        Self::list_hashes(&self.objects_dir)
    }

    /// Remove a manifest by hash.
    pub fn remove_manifest(&self, hash: &ContentHash) -> Result<(), StoreError> {
        let path = self.manifest_path(hash);
        if path.exists() {
            fs::remove_file(&path)?;
        }
        Ok(())
    }

    /// Remove a chunk by hash.
    pub fn remove_chunk(&self, hash: &ContentHash) -> Result<(), StoreError> {
        let path = self.chunk_path(hash);
        if path.exists() {
            fs::remove_file(&path)?;
        }
        Ok(())
    }

    fn list_hashes(dir: &Path) -> Result<Vec<ContentHash>, StoreError> {
        let mut hashes = Vec::new();
        if !dir.exists() {
            return Ok(hashes);
        }
        for shard_entry in fs::read_dir(dir)? {
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
                    hashes.push(hash);
                }
            }
        }
        Ok(hashes)
    }
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p hd-cas gc::tests`
Expected: 4 tests PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/hd-cas/src/gc.rs crates/hd-cas/src/store.rs
git commit -m "feat(hd-cas): add reference-counting garbage collector"
```

---

## Task 7: CAS Integration Tests

**Files:**
- Create: `crates/hd-cas/tests/integration.rs`

- [ ] **Step 1: Write integration tests**

```rust
// crates/hd-cas/tests/integration.rs
use hd_cas::hash::ContentHash;
use hd_cas::manifest::Manifest;
use hd_cas::store::ContentStore;
use hd_cas::gc::GarbageCollector;
use tempfile::TempDir;
use std::fs;

#[test]
fn full_lifecycle_put_file_gc() {
    let dir = TempDir::new().unwrap();
    let store = ContentStore::open(dir.path()).unwrap();
    let gc = GarbageCollector::new(dir.path()).unwrap();

    // Create and ingest two files with some shared content
    let file1_path = dir.path().join("file1.txt");
    let file2_path = dir.path().join("file2.txt");
    fs::write(&file1_path, b"shared prefix - unique suffix one").unwrap();
    fs::write(&file2_path, b"shared prefix - unique suffix two").unwrap();

    let mh1 = store.put_file(&file1_path).unwrap();
    let mh2 = store.put_file(&file2_path).unwrap();
    assert_ne!(mh1, mh2);

    // Reference only file1
    gc.add_ref(&mh1).unwrap();

    // GC should remove file2's manifest (and any unique chunks)
    let stats = gc.collect(&store).unwrap();
    assert_eq!(stats.manifests_removed, 1);

    // file1 should still be retrievable
    let out_path = dir.path().join("recovered.txt");
    store.get_file(&mh1, &out_path).unwrap();
    assert_eq!(fs::read(&out_path).unwrap(), b"shared prefix - unique suffix one");

    // file2's manifest should be gone
    assert!(store.get_manifest(&mh2).is_err());
}

#[test]
fn dedup_across_identical_files() {
    let dir = TempDir::new().unwrap();
    let store = ContentStore::open(dir.path()).unwrap();

    let content = b"identical content in both files";
    let f1 = dir.path().join("a.txt");
    let f2 = dir.path().join("b.txt");
    fs::write(&f1, content).unwrap();
    fs::write(&f2, content).unwrap();

    let mh1 = store.put_file(&f1).unwrap();
    let mh2 = store.put_file(&f2).unwrap();

    // Same content, same mode → same manifest hash
    assert_eq!(mh1, mh2);

    // Only one set of chunks should exist
    let chunks = store.list_chunks().unwrap();
    assert_eq!(chunks.len(), 1);
}

#[test]
fn large_file_roundtrip_integrity() {
    let dir = TempDir::new().unwrap();
    let store = ContentStore::open(dir.path()).unwrap();

    // 1MB of pseudo-random data
    let data: Vec<u8> = (0..1_000_000u64)
        .map(|i| ((i.wrapping_mul(6364136223846793005).wrapping_add(1)) >> 33) as u8)
        .collect();

    let file_path = dir.path().join("large.bin");
    fs::write(&file_path, &data).unwrap();

    let mhash = store.put_file(&file_path).unwrap();

    let manifest = store.get_manifest(&mhash).unwrap();
    assert!(manifest.chunks.len() > 1, "should have multiple chunks");
    assert_eq!(manifest.size, 1_000_000);

    let out_path = dir.path().join("large_out.bin");
    store.get_file(&mhash, &out_path).unwrap();
    assert_eq!(fs::read(&out_path).unwrap(), data);
}
```

- [ ] **Step 2: Run integration tests**

Run: `cargo test -p hd-cas --test integration`
Expected: 3 tests PASS.

- [ ] **Step 3: Commit**

```bash
git add crates/hd-cas/tests/integration.rs
git commit -m "test(hd-cas): add integration tests for full CAS lifecycle"
```

---

## Task 8: CAS Public API Exports (`hd-cas/src/lib.rs`)

**Files:**
- Modify: `crates/hd-cas/src/lib.rs`

- [ ] **Step 1: Update lib.rs with clean public API**

```rust
// crates/hd-cas/src/lib.rs
pub mod hash;
pub mod chunk;
pub mod manifest;
pub mod store;
pub mod gc;

// Re-export key types at crate root for convenience
pub use hash::ContentHash;
pub use manifest::Manifest;
pub use store::ContentStore;
pub use gc::{GarbageCollector, GcStats};
```

- [ ] **Step 2: Verify everything compiles and tests pass**

Run: `cargo test -p hd-cas`
Expected: All tests PASS (hash: 5, chunk: 5, manifest: 4, store: 8, gc: 4, integration: 3 = 29 total).

- [ ] **Step 3: Commit**

```bash
git add crates/hd-cas/src/lib.rs
git commit -m "feat(hd-cas): add public API re-exports"
```

---

## Task 9: DAG Node Types (`hd-engine/src/node.rs`)

**Files:**
- Create: `crates/hd-engine/src/node.rs`
- Test: `crates/hd-engine/src/node.rs` (inline tests)

- [ ] **Step 1: Write the failing tests**

```rust
// crates/hd-engine/src/node.rs

#[cfg(test)]
mod tests {
    use super::*;
    use hd_cas::ContentHash;

    #[test]
    fn file_node_hash_deterministic() {
        let manifest_hash = ContentHash::from_bytes(b"manifest1");
        let n1 = Node::file("src/main.rs", manifest_hash);
        let n2 = Node::file("src/main.rs", manifest_hash);
        assert_eq!(n1.content_hash(), n2.content_hash());
    }

    #[test]
    fn file_node_hash_changes_with_content() {
        let n1 = Node::file("src/main.rs", ContentHash::from_bytes(b"v1"));
        let n2 = Node::file("src/main.rs", ContentHash::from_bytes(b"v2"));
        assert_ne!(n1.content_hash(), n2.content_hash());
    }

    #[test]
    fn dir_node_hash_from_sorted_children() {
        let child_a = ContentHash::from_bytes(b"a");
        let child_b = ContentHash::from_bytes(b"b");

        // Order of children shouldn't matter — hash is computed from sorted children
        let n1 = Node::dir("src", vec![
            ("a.rs".into(), child_a),
            ("b.rs".into(), child_b),
        ]);
        let n2 = Node::dir("src", vec![
            ("b.rs".into(), child_b),
            ("a.rs".into(), child_a),
        ]);
        assert_eq!(n1.content_hash(), n2.content_hash());
    }

    #[test]
    fn build_step_hash_includes_command_and_inputs() {
        let input = ContentHash::from_bytes(b"input");
        let n1 = Node::build_step("npm install", vec![input], vec![]);
        let n2 = Node::build_step("npm install", vec![input], vec![]);
        assert_eq!(n1.content_hash(), n2.content_hash());

        // Different command → different hash
        let n3 = Node::build_step("npm ci", vec![input], vec![]);
        assert_ne!(n1.content_hash(), n3.content_hash());
    }

    #[test]
    fn build_step_hash_includes_env_vars() {
        let input = ContentHash::from_bytes(b"input");
        let n1 = Node::build_step("make", vec![input], vec![("CC".into(), "gcc".into())]);
        let n2 = Node::build_step("make", vec![input], vec![("CC".into(), "clang".into())]);
        assert_ne!(n1.content_hash(), n2.content_hash());
    }

    #[test]
    fn env_node_hash_from_children() {
        let child1 = ContentHash::from_bytes(b"child1");
        let child2 = ContentHash::from_bytes(b"child2");
        let env = Node::env("myapp", vec![child1, child2]);
        // just verify it produces a consistent hash
        assert_eq!(env.content_hash(), env.content_hash());
    }

    #[test]
    fn package_node_hash() {
        let n1 = Node::package("npm", "express", "4.18.2", ContentHash::from_bytes(b"express-pkg"));
        let n2 = Node::package("npm", "express", "4.18.2", ContentHash::from_bytes(b"express-pkg"));
        assert_eq!(n1.content_hash(), n2.content_hash());

        let n3 = Node::package("npm", "express", "4.19.0", ContentHash::from_bytes(b"express-pkg-new"));
        assert_ne!(n1.content_hash(), n3.content_hash());
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p hd-engine node::tests`
Expected: FAIL — `Node` not defined.

- [ ] **Step 3: Write the implementation**

```rust
// crates/hd-engine/src/node.rs
use hd_cas::ContentHash;
use serde::{Deserialize, Serialize};

/// A node in the Merkle DAG. Each variant computes its content hash
/// deterministically from its inputs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Node {
    File {
        path: String,
        manifest_hash: ContentHash,
    },
    Dir {
        path: String,
        /// Sorted vec of (child_name, child_content_hash)
        children: Vec<(String, ContentHash)>,
    },
    Package {
        provider: String,
        name: String,
        version: String,
        artifact_hash: ContentHash,
    },
    BuildStep {
        command: String,
        input_hashes: Vec<ContentHash>,
        env_vars: Vec<(String, String)>,
    },
    Env {
        name: String,
        children: Vec<ContentHash>,
    },
}

impl Node {
    pub fn file(path: &str, manifest_hash: ContentHash) -> Self {
        Node::File {
            path: path.to_string(),
            manifest_hash,
        }
    }

    pub fn dir(path: &str, mut children: Vec<(String, ContentHash)>) -> Self {
        children.sort_by(|a, b| a.0.cmp(&b.0));
        Node::Dir {
            path: path.to_string(),
            children,
        }
    }

    pub fn package(provider: &str, name: &str, version: &str, artifact_hash: ContentHash) -> Self {
        Node::Package {
            provider: provider.to_string(),
            name: name.to_string(),
            version: version.to_string(),
            artifact_hash,
        }
    }

    pub fn build_step(command: &str, input_hashes: Vec<ContentHash>, mut env_vars: Vec<(String, String)>) -> Self {
        env_vars.sort_by(|a, b| a.0.cmp(&b.0));
        Node::BuildStep {
            command: command.to_string(),
            input_hashes,
            env_vars,
        }
    }

    pub fn env(name: &str, children: Vec<ContentHash>) -> Self {
        Node::Env {
            name: name.to_string(),
            children,
        }
    }

    /// Compute the content hash of this node.
    pub fn content_hash(&self) -> ContentHash {
        let mut hasher = blake3::Hasher::new();

        match self {
            Node::File { path, manifest_hash } => {
                hasher.update(b"file:");
                hasher.update(path.as_bytes());
                hasher.update(manifest_hash.as_bytes());
            }
            Node::Dir { path, children } => {
                hasher.update(b"dir:");
                hasher.update(path.as_bytes());
                for (name, hash) in children {
                    hasher.update(name.as_bytes());
                    hasher.update(hash.as_bytes());
                }
            }
            Node::Package { provider, name, version, artifact_hash } => {
                hasher.update(b"pkg:");
                hasher.update(provider.as_bytes());
                hasher.update(name.as_bytes());
                hasher.update(version.as_bytes());
                hasher.update(artifact_hash.as_bytes());
            }
            Node::BuildStep { command, input_hashes, env_vars } => {
                hasher.update(b"build:");
                hasher.update(command.as_bytes());
                for ih in input_hashes {
                    hasher.update(ih.as_bytes());
                }
                for (k, v) in env_vars {
                    hasher.update(k.as_bytes());
                    hasher.update(b"=");
                    hasher.update(v.as_bytes());
                }
            }
            Node::Env { name, children } => {
                hasher.update(b"env:");
                hasher.update(name.as_bytes());
                for child in children {
                    hasher.update(child.as_bytes());
                }
            }
        }

        ContentHash::from_raw(*hasher.finalize().as_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_node_hash_deterministic() {
        let manifest_hash = ContentHash::from_bytes(b"manifest1");
        let n1 = Node::file("src/main.rs", manifest_hash);
        let n2 = Node::file("src/main.rs", manifest_hash);
        assert_eq!(n1.content_hash(), n2.content_hash());
    }

    #[test]
    fn file_node_hash_changes_with_content() {
        let n1 = Node::file("src/main.rs", ContentHash::from_bytes(b"v1"));
        let n2 = Node::file("src/main.rs", ContentHash::from_bytes(b"v2"));
        assert_ne!(n1.content_hash(), n2.content_hash());
    }

    #[test]
    fn dir_node_hash_from_sorted_children() {
        let child_a = ContentHash::from_bytes(b"a");
        let child_b = ContentHash::from_bytes(b"b");

        let n1 = Node::dir("src", vec![
            ("a.rs".into(), child_a),
            ("b.rs".into(), child_b),
        ]);
        let n2 = Node::dir("src", vec![
            ("b.rs".into(), child_b),
            ("a.rs".into(), child_a),
        ]);
        assert_eq!(n1.content_hash(), n2.content_hash());
    }

    #[test]
    fn build_step_hash_includes_command_and_inputs() {
        let input = ContentHash::from_bytes(b"input");
        let n1 = Node::build_step("npm install", vec![input], vec![]);
        let n2 = Node::build_step("npm install", vec![input], vec![]);
        assert_eq!(n1.content_hash(), n2.content_hash());

        let n3 = Node::build_step("npm ci", vec![input], vec![]);
        assert_ne!(n1.content_hash(), n3.content_hash());
    }

    #[test]
    fn build_step_hash_includes_env_vars() {
        let input = ContentHash::from_bytes(b"input");
        let n1 = Node::build_step("make", vec![input], vec![("CC".into(), "gcc".into())]);
        let n2 = Node::build_step("make", vec![input], vec![("CC".into(), "clang".into())]);
        assert_ne!(n1.content_hash(), n2.content_hash());
    }

    #[test]
    fn env_node_hash_from_children() {
        let child1 = ContentHash::from_bytes(b"child1");
        let child2 = ContentHash::from_bytes(b"child2");
        let env = Node::env("myapp", vec![child1, child2]);
        assert_eq!(env.content_hash(), env.content_hash());
    }

    #[test]
    fn package_node_hash() {
        let n1 = Node::package("npm", "express", "4.18.2", ContentHash::from_bytes(b"express-pkg"));
        let n2 = Node::package("npm", "express", "4.18.2", ContentHash::from_bytes(b"express-pkg"));
        assert_eq!(n1.content_hash(), n2.content_hash());

        let n3 = Node::package("npm", "express", "4.19.0", ContentHash::from_bytes(b"express-pkg-new"));
        assert_ne!(n1.content_hash(), n3.content_hash());
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p hd-engine node::tests`
Expected: 7 tests PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/hd-engine/src/node.rs
git commit -m "feat(hd-engine): add DAG node types with deterministic content hashing"
```

---

## Task 10: DAG Construction & Storage (`hd-engine/src/dag.rs`)

**Files:**
- Create: `crates/hd-engine/src/dag.rs`
- Test: `crates/hd-engine/src/dag.rs` (inline tests)

- [ ] **Step 1: Write the failing tests**

```rust
// crates/hd-engine/src/dag.rs

#[cfg(test)]
mod tests {
    use super::*;
    use hd_cas::ContentHash;
    use tempfile::TempDir;

    fn test_dag() -> (Dag, TempDir) {
        let dir = TempDir::new().unwrap();
        let store = hd_cas::ContentStore::open(dir.path()).unwrap();
        let dag = Dag::new(store);
        (dag, dir)
    }

    #[test]
    fn insert_and_get_node() {
        let (mut dag, _dir) = test_dag();
        let node = Node::file("main.rs", ContentHash::from_bytes(b"content"));
        let hash = dag.insert(node.clone()).unwrap();
        let retrieved = dag.get(&hash).unwrap();
        assert_eq!(retrieved.content_hash(), node.content_hash());
    }

    #[test]
    fn get_nonexistent_returns_none() {
        let (dag, _dir) = test_dag();
        let fake = ContentHash::from_bytes(b"nope");
        assert!(dag.get(&fake).is_none());
    }

    #[test]
    fn insert_deduplicates() {
        let (mut dag, _dir) = test_dag();
        let node = Node::file("lib.rs", ContentHash::from_bytes(b"same"));
        let h1 = dag.insert(node.clone()).unwrap();
        let h2 = dag.insert(node).unwrap();
        assert_eq!(h1, h2);
    }

    #[test]
    fn build_tree_and_query_path() {
        let (mut dag, _dir) = test_dag();

        // Build: env -> dir("src") -> file("main.rs")
        let file_node = Node::file("src/main.rs", ContentHash::from_bytes(b"main"));
        let file_hash = dag.insert(file_node).unwrap();

        let dir_node = Node::dir("src", vec![("main.rs".into(), file_hash)]);
        let dir_hash = dag.insert(dir_node).unwrap();

        let env_node = Node::env("myapp", vec![dir_hash]);
        let env_hash = dag.insert(env_node).unwrap();

        // Query by path
        let found = dag.query(&env_hash, "src/main.rs");
        assert!(found.is_some());
        assert_eq!(found.unwrap().content_hash(), ContentHash::from_bytes(
            // This should match the file node's hash
            &Node::file("src/main.rs", ContentHash::from_bytes(b"main")).content_hash().as_bytes()[..]
        ));
    }

    #[test]
    fn children_of_env_node() {
        let (mut dag, _dir) = test_dag();
        let f1 = dag.insert(Node::file("a.rs", ContentHash::from_bytes(b"a"))).unwrap();
        let f2 = dag.insert(Node::file("b.rs", ContentHash::from_bytes(b"b"))).unwrap();
        let env = dag.insert(Node::env("test", vec![f1, f2])).unwrap();

        let children = dag.children(&env);
        assert_eq!(children.len(), 2);
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p hd-engine dag::tests`
Expected: FAIL — `Dag` not defined.

- [ ] **Step 3: Write the implementation**

```rust
// crates/hd-engine/src/dag.rs
use std::collections::HashMap;

use hd_cas::{ContentHash, ContentStore};
use serde::{Deserialize, Serialize};

use crate::node::Node;

/// In-memory DAG with nodes indexed by content hash.
/// Nodes are also serialized into the CAS for persistence.
pub struct Dag {
    nodes: HashMap<ContentHash, Node>,
    store: ContentStore,
}

#[derive(Debug, thiserror::Error)]
pub enum DagError {
    #[error("store error: {0}")]
    Store(#[from] hd_cas::store::StoreError),
    #[error("serialization error: {0}")]
    Serialization(String),
}

impl Dag {
    pub fn new(store: ContentStore) -> Self {
        Dag {
            nodes: HashMap::new(),
            store,
        }
    }

    /// Insert a node into the DAG. Returns its content hash.
    /// Deduplicates: if a node with the same hash exists, returns the existing hash.
    pub fn insert(&mut self, node: Node) -> Result<ContentHash, DagError> {
        let hash = node.content_hash();
        if !self.nodes.contains_key(&hash) {
            // Persist to CAS
            let bytes = bincode::serde::encode_to_vec(&node, bincode::config::standard())
                .map_err(|e| DagError::Serialization(e.to_string()))?;
            self.store.put_chunk(&bytes)?;
            self.nodes.insert(hash, node);
        }
        Ok(hash)
    }

    /// Get a node by its content hash.
    pub fn get(&self, hash: &ContentHash) -> Option<&Node> {
        self.nodes.get(hash)
    }

    /// Get the direct children of a node (by content hash).
    pub fn children(&self, hash: &ContentHash) -> Vec<ContentHash> {
        match self.nodes.get(hash) {
            Some(Node::Dir { children, .. }) => children.iter().map(|(_, h)| *h).collect(),
            Some(Node::Env { children, .. }) => children.clone(),
            Some(Node::BuildStep { input_hashes, .. }) => input_hashes.clone(),
            _ => Vec::new(),
        }
    }

    /// Query the DAG for a node by path, starting from an env or dir root.
    /// Path format: "dir/subdir/file.rs"
    pub fn query(&self, root: &ContentHash, path: &str) -> Option<&Node> {
        let parts: Vec<&str> = path.split('/').collect();
        self.query_recursive(root, &parts, 0)
    }

    fn query_recursive(&self, current: &ContentHash, parts: &[&str], depth: usize) -> Option<&Node> {
        if depth >= parts.len() {
            return self.nodes.get(current);
        }

        let node = self.nodes.get(current)?;
        match node {
            Node::Env { children, .. } => {
                // Search children for a dir or file matching parts[depth]
                for child_hash in children {
                    if let Some(result) = self.query_recursive(child_hash, parts, depth) {
                        return Some(result);
                    }
                }
                None
            }
            Node::Dir { path: dir_path, children, .. } => {
                let dir_name = dir_path.rsplit('/').next().unwrap_or(dir_path);
                if depth == 0 || dir_name == parts[depth - 1] {
                    // Check if dir matches current path component
                    if dir_name == parts[depth] {
                        // This dir itself is what we're looking for
                        if depth == parts.len() - 1 {
                            return Some(node);
                        }
                        // Look into children for next part
                        for (child_name, child_hash) in children {
                            if child_name == parts[depth + 1] {
                                return self.query_recursive(child_hash, parts, depth + 2);
                            }
                        }
                    }
                    // Try matching children
                    for (child_name, child_hash) in children {
                        if child_name == parts[depth] {
                            if depth == parts.len() - 1 {
                                return self.nodes.get(child_hash);
                            }
                            return self.query_recursive(child_hash, parts, depth + 1);
                        }
                    }
                }
                None
            }
            _ => None,
        }
    }

    /// Return a reference to the underlying CAS.
    pub fn store(&self) -> &ContentStore {
        &self.store
    }

    /// Get all node hashes in the DAG.
    pub fn node_hashes(&self) -> Vec<ContentHash> {
        self.nodes.keys().copied().collect()
    }

    /// Get the parent hashes of a node (all nodes that reference this hash as a child).
    pub fn parents(&self, target: &ContentHash) -> Vec<ContentHash> {
        let mut parents = Vec::new();
        for (hash, node) in &self.nodes {
            let is_parent = match node {
                Node::Dir { children, .. } => children.iter().any(|(_, h)| h == target),
                Node::Env { children, .. } => children.contains(target),
                Node::BuildStep { input_hashes, .. } => input_hashes.contains(target),
                _ => false,
            };
            if is_parent {
                parents.push(*hash);
            }
        }
        parents
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn test_dag() -> (Dag, TempDir) {
        let dir = TempDir::new().unwrap();
        let store = hd_cas::ContentStore::open(dir.path()).unwrap();
        let dag = Dag::new(store);
        (dag, dir)
    }

    #[test]
    fn insert_and_get_node() {
        let (mut dag, _dir) = test_dag();
        let node = Node::file("main.rs", ContentHash::from_bytes(b"content"));
        let hash = dag.insert(node.clone()).unwrap();
        let retrieved = dag.get(&hash).unwrap();
        assert_eq!(retrieved.content_hash(), node.content_hash());
    }

    #[test]
    fn get_nonexistent_returns_none() {
        let (dag, _dir) = test_dag();
        let fake = ContentHash::from_bytes(b"nope");
        assert!(dag.get(&fake).is_none());
    }

    #[test]
    fn insert_deduplicates() {
        let (mut dag, _dir) = test_dag();
        let node = Node::file("lib.rs", ContentHash::from_bytes(b"same"));
        let h1 = dag.insert(node.clone()).unwrap();
        let h2 = dag.insert(node).unwrap();
        assert_eq!(h1, h2);
    }

    #[test]
    fn build_tree_and_query_path() {
        let (mut dag, _dir) = test_dag();

        let file_node = Node::file("src/main.rs", ContentHash::from_bytes(b"main"));
        let file_hash = dag.insert(file_node.clone()).unwrap();

        let dir_node = Node::dir("src", vec![("main.rs".into(), file_hash)]);
        let dir_hash = dag.insert(dir_node).unwrap();

        let env_node = Node::env("myapp", vec![dir_hash]);
        let env_hash = dag.insert(env_node).unwrap();

        let found = dag.query(&env_hash, "src/main.rs");
        assert!(found.is_some());
        assert_eq!(found.unwrap().content_hash(), file_node.content_hash());
    }

    #[test]
    fn children_of_env_node() {
        let (mut dag, _dir) = test_dag();
        let f1 = dag.insert(Node::file("a.rs", ContentHash::from_bytes(b"a"))).unwrap();
        let f2 = dag.insert(Node::file("b.rs", ContentHash::from_bytes(b"b"))).unwrap();
        let env = dag.insert(Node::env("test", vec![f1, f2])).unwrap();

        let children = dag.children(&env);
        assert_eq!(children.len(), 2);
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p hd-engine dag::tests`
Expected: 5 tests PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/hd-engine/src/dag.rs
git commit -m "feat(hd-engine): add DAG construction, storage, query, and parent traversal"
```

---

## Task 11: Incremental Invalidation (`hd-engine/src/invalidation.rs`)

**Files:**
- Create: `crates/hd-engine/src/invalidation.rs`
- Test: `crates/hd-engine/src/invalidation.rs` (inline tests)

- [ ] **Step 1: Write the failing tests**

```rust
// crates/hd-engine/src/invalidation.rs

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node::Node;
    use crate::dag::Dag;
    use hd_cas::ContentHash;
    use tempfile::TempDir;

    fn test_dag() -> (Dag, TempDir) {
        let dir = TempDir::new().unwrap();
        let store = hd_cas::ContentStore::open(dir.path()).unwrap();
        (Dag::new(store), dir)
    }

    #[test]
    fn invalidate_file_propagates_to_dir_and_env() {
        let (mut dag, _dir) = test_dag();

        // Build: env -> dir("src") -> file("main.rs")
        let file = Node::file("src/main.rs", ContentHash::from_bytes(b"v1"));
        let file_hash = dag.insert(file).unwrap();
        let dir = Node::dir("src", vec![("main.rs".into(), file_hash)]);
        let dir_hash = dag.insert(dir).unwrap();
        let env = Node::env("app", vec![dir_hash]);
        let env_hash = dag.insert(env).unwrap();

        // File changes from v1 to v2
        let new_file = Node::file("src/main.rs", ContentHash::from_bytes(b"v2"));
        let change = FileChange {
            old_hash: file_hash,
            new_node: new_file,
        };

        let result = invalidate(&mut dag, &env_hash, &[change]).unwrap();

        // New env hash should differ
        assert_ne!(result.new_root, env_hash);
        // Stale set should include old file, dir, and env hashes
        assert!(result.invalidated.contains(&file_hash));
        assert!(result.invalidated.contains(&dir_hash));
        assert!(result.invalidated.contains(&env_hash));
    }

    #[test]
    fn invalidate_preserves_unchanged_siblings() {
        let (mut dag, _dir) = test_dag();

        let file_a = Node::file("src/a.rs", ContentHash::from_bytes(b"a"));
        let file_b = Node::file("src/b.rs", ContentHash::from_bytes(b"b"));
        let hash_a = dag.insert(file_a).unwrap();
        let hash_b = dag.insert(file_b).unwrap();
        let dir = Node::dir("src", vec![
            ("a.rs".into(), hash_a),
            ("b.rs".into(), hash_b),
        ]);
        let dir_hash = dag.insert(dir).unwrap();
        let env = Node::env("app", vec![dir_hash]);
        let env_hash = dag.insert(env).unwrap();

        // Only file_a changes
        let new_a = Node::file("src/a.rs", ContentHash::from_bytes(b"a_v2"));
        let change = FileChange {
            old_hash: hash_a,
            new_node: new_a,
        };

        let result = invalidate(&mut dag, &env_hash, &[change]).unwrap();

        // b.rs should not be invalidated
        assert!(!result.invalidated.contains(&hash_b));
        // a.rs, dir, and env should be invalidated
        assert!(result.invalidated.contains(&hash_a));
        assert!(result.invalidated.contains(&dir_hash));
        assert!(result.invalidated.contains(&env_hash));
    }

    #[test]
    fn invalidate_no_changes_returns_same_root() {
        let (mut dag, _dir) = test_dag();
        let file = Node::file("main.rs", ContentHash::from_bytes(b"v1"));
        let fh = dag.insert(file).unwrap();
        let env = Node::env("app", vec![fh]);
        let eh = dag.insert(env).unwrap();

        let result = invalidate(&mut dag, &eh, &[]).unwrap();
        assert_eq!(result.new_root, eh);
        assert!(result.invalidated.is_empty());
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p hd-engine invalidation::tests`
Expected: FAIL — `invalidate`, `FileChange`, `InvalidationResult` not defined.

- [ ] **Step 3: Write the implementation**

```rust
// crates/hd-engine/src/invalidation.rs
use std::collections::HashSet;

use hd_cas::ContentHash;

use crate::dag::{Dag, DagError};
use crate::node::Node;

/// Describes a file-level change to apply to the DAG.
pub struct FileChange {
    pub old_hash: ContentHash,
    pub new_node: Node,
}

/// Result of an invalidation pass.
pub struct InvalidationResult {
    /// The new root hash after applying all changes.
    pub new_root: ContentHash,
    /// Set of old node hashes that were invalidated (stale).
    pub invalidated: HashSet<ContentHash>,
}

/// Apply a set of file changes to the DAG, propagating invalidation bottom-up.
///
/// For each changed file:
/// 1. Insert the new node
/// 2. Find all ancestors of the old node
/// 3. Rebuild each ancestor with the updated child hash
///
/// Returns the new root hash and the set of invalidated node hashes.
pub fn invalidate(
    dag: &mut Dag,
    root: &ContentHash,
    changes: &[FileChange],
) -> Result<InvalidationResult, DagError> {
    if changes.is_empty() {
        return Ok(InvalidationResult {
            new_root: *root,
            invalidated: HashSet::new(),
        });
    }

    let mut invalidated = HashSet::new();
    // Map from old hash -> new hash for remapping parent references
    let mut remap: std::collections::HashMap<ContentHash, ContentHash> = std::collections::HashMap::new();

    // Phase 1: Insert new file nodes and seed the remap
    for change in changes {
        let new_hash = dag.insert(change.new_node.clone())?;
        invalidated.insert(change.old_hash);
        remap.insert(change.old_hash, new_hash);
    }

    // Phase 2: Walk up from changed nodes, rebuilding ancestors
    // We process level by level — find all parents of changed nodes,
    // rebuild them, then find their parents, etc.
    let mut current_level: Vec<ContentHash> = changes.iter().map(|c| c.old_hash).collect();

    loop {
        let mut next_level = Vec::new();

        for old_hash in &current_level {
            let parents = dag.parents(old_hash);
            for parent_hash in parents {
                if invalidated.contains(&parent_hash) {
                    continue; // already processed
                }
                let parent_node = match dag.get(&parent_hash) {
                    Some(n) => n.clone(),
                    None => continue,
                };

                let new_parent = rebuild_node(&parent_node, &remap);
                let new_parent_hash = dag.insert(new_parent)?;

                invalidated.insert(parent_hash);
                remap.insert(parent_hash, new_parent_hash);
                next_level.push(parent_hash);
            }
        }

        if next_level.is_empty() {
            break;
        }
        current_level = next_level;
    }

    let new_root = remap.get(root).copied().unwrap_or(*root);

    Ok(InvalidationResult {
        new_root,
        invalidated,
    })
}

/// Rebuild a node by replacing any child references found in the remap table.
fn rebuild_node(
    node: &Node,
    remap: &std::collections::HashMap<ContentHash, ContentHash>,
) -> Node {
    match node {
        Node::Dir { path, children } => {
            let new_children: Vec<(String, ContentHash)> = children
                .iter()
                .map(|(name, hash)| {
                    let new_hash = remap.get(hash).copied().unwrap_or(*hash);
                    (name.clone(), new_hash)
                })
                .collect();
            Node::dir(path, new_children)
        }
        Node::Env { name, children } => {
            let new_children: Vec<ContentHash> = children
                .iter()
                .map(|h| remap.get(h).copied().unwrap_or(*h))
                .collect();
            Node::env(name, new_children)
        }
        Node::BuildStep { command, input_hashes, env_vars } => {
            let new_inputs: Vec<ContentHash> = input_hashes
                .iter()
                .map(|h| remap.get(h).copied().unwrap_or(*h))
                .collect();
            Node::build_step(command, new_inputs, env_vars.clone())
        }
        // File and Package nodes don't have children to remap
        other => other.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn test_dag() -> (Dag, TempDir) {
        let dir = TempDir::new().unwrap();
        let store = hd_cas::ContentStore::open(dir.path()).unwrap();
        (Dag::new(store), dir)
    }

    #[test]
    fn invalidate_file_propagates_to_dir_and_env() {
        let (mut dag, _dir) = test_dag();

        let file = Node::file("src/main.rs", ContentHash::from_bytes(b"v1"));
        let file_hash = dag.insert(file).unwrap();
        let dir = Node::dir("src", vec![("main.rs".into(), file_hash)]);
        let dir_hash = dag.insert(dir).unwrap();
        let env = Node::env("app", vec![dir_hash]);
        let env_hash = dag.insert(env).unwrap();

        let new_file = Node::file("src/main.rs", ContentHash::from_bytes(b"v2"));
        let change = FileChange {
            old_hash: file_hash,
            new_node: new_file,
        };

        let result = invalidate(&mut dag, &env_hash, &[change]).unwrap();

        assert_ne!(result.new_root, env_hash);
        assert!(result.invalidated.contains(&file_hash));
        assert!(result.invalidated.contains(&dir_hash));
        assert!(result.invalidated.contains(&env_hash));
    }

    #[test]
    fn invalidate_preserves_unchanged_siblings() {
        let (mut dag, _dir) = test_dag();

        let file_a = Node::file("src/a.rs", ContentHash::from_bytes(b"a"));
        let file_b = Node::file("src/b.rs", ContentHash::from_bytes(b"b"));
        let hash_a = dag.insert(file_a).unwrap();
        let hash_b = dag.insert(file_b).unwrap();
        let dir = Node::dir("src", vec![
            ("a.rs".into(), hash_a),
            ("b.rs".into(), hash_b),
        ]);
        let dir_hash = dag.insert(dir).unwrap();
        let env = Node::env("app", vec![dir_hash]);
        let env_hash = dag.insert(env).unwrap();

        let new_a = Node::file("src/a.rs", ContentHash::from_bytes(b"a_v2"));
        let change = FileChange {
            old_hash: hash_a,
            new_node: new_a,
        };

        let result = invalidate(&mut dag, &env_hash, &[change]).unwrap();

        assert!(!result.invalidated.contains(&hash_b));
        assert!(result.invalidated.contains(&hash_a));
        assert!(result.invalidated.contains(&dir_hash));
        assert!(result.invalidated.contains(&env_hash));
    }

    #[test]
    fn invalidate_no_changes_returns_same_root() {
        let (mut dag, _dir) = test_dag();
        let file = Node::file("main.rs", ContentHash::from_bytes(b"v1"));
        let fh = dag.insert(file).unwrap();
        let env = Node::env("app", vec![fh]);
        let eh = dag.insert(env).unwrap();

        let result = invalidate(&mut dag, &eh, &[]).unwrap();
        assert_eq!(result.new_root, eh);
        assert!(result.invalidated.is_empty());
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p hd-engine invalidation::tests`
Expected: 3 tests PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/hd-engine/src/invalidation.rs
git commit -m "feat(hd-engine): add bottom-up incremental DAG invalidation"
```

---

## Task 12: DAG Diffing (`hd-engine/src/diff.rs`)

**Files:**
- Create: `crates/hd-engine/src/diff.rs`
- Test: `crates/hd-engine/src/diff.rs` (inline tests)

- [ ] **Step 1: Write the failing tests**

```rust
// crates/hd-engine/src/diff.rs

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node::Node;
    use crate::dag::Dag;
    use hd_cas::ContentHash;
    use tempfile::TempDir;

    fn test_dag() -> (Dag, TempDir) {
        let dir = TempDir::new().unwrap();
        let store = hd_cas::ContentStore::open(dir.path()).unwrap();
        (Dag::new(store), dir)
    }

    #[test]
    fn diff_identical_roots() {
        let (mut dag, _dir) = test_dag();
        let file = Node::file("a.rs", ContentHash::from_bytes(b"a"));
        let fh = dag.insert(file).unwrap();
        let env = Node::env("app", vec![fh]);
        let eh = dag.insert(env).unwrap();

        let diff = dag_diff(&dag, &eh, &eh);
        assert!(diff.added.is_empty());
        assert!(diff.removed.is_empty());
        assert!(diff.changed.is_empty());
    }

    #[test]
    fn diff_added_node() {
        let (mut dag, _dir) = test_dag();
        let f1 = Node::file("a.rs", ContentHash::from_bytes(b"a"));
        let fh1 = dag.insert(f1).unwrap();
        let env1 = Node::env("app", vec![fh1]);
        let eh1 = dag.insert(env1).unwrap();

        let f2 = Node::file("b.rs", ContentHash::from_bytes(b"b"));
        let fh2 = dag.insert(f2).unwrap();
        let env2 = Node::env("app", vec![fh1, fh2]);
        let eh2 = dag.insert(env2).unwrap();

        let diff = dag_diff(&dag, &eh1, &eh2);
        assert_eq!(diff.added.len(), 1);
        assert!(diff.added.contains(&fh2));
        assert!(diff.removed.is_empty());
    }

    #[test]
    fn diff_removed_node() {
        let (mut dag, _dir) = test_dag();
        let f1 = Node::file("a.rs", ContentHash::from_bytes(b"a"));
        let f2 = Node::file("b.rs", ContentHash::from_bytes(b"b"));
        let fh1 = dag.insert(f1).unwrap();
        let fh2 = dag.insert(f2).unwrap();
        let env1 = Node::env("app", vec![fh1, fh2]);
        let eh1 = dag.insert(env1).unwrap();

        let env2 = Node::env("app", vec![fh1]);
        let eh2 = dag.insert(env2).unwrap();

        let diff = dag_diff(&dag, &eh1, &eh2);
        assert!(diff.added.is_empty());
        assert_eq!(diff.removed.len(), 1);
        assert!(diff.removed.contains(&fh2));
    }

    #[test]
    fn diff_changed_is_symmetric_add_remove() {
        let (mut dag, _dir) = test_dag();
        let f1 = Node::file("a.rs", ContentHash::from_bytes(b"v1"));
        let fh1 = dag.insert(f1).unwrap();
        let env1 = Node::env("app", vec![fh1]);
        let eh1 = dag.insert(env1).unwrap();

        let f2 = Node::file("a.rs", ContentHash::from_bytes(b"v2"));
        let fh2 = dag.insert(f2).unwrap();
        let env2 = Node::env("app", vec![fh2]);
        let eh2 = dag.insert(env2).unwrap();

        let diff = dag_diff(&dag, &eh1, &eh2);
        // fh1 removed, fh2 added, roots changed
        assert!(diff.added.contains(&fh2));
        assert!(diff.removed.contains(&fh1));
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p hd-engine diff::tests`
Expected: FAIL — `dag_diff`, `DagDiff` not defined.

- [ ] **Step 3: Write the implementation**

```rust
// crates/hd-engine/src/diff.rs
use std::collections::HashSet;

use hd_cas::ContentHash;

use crate::dag::Dag;

/// The result of diffing two DAG states.
#[derive(Debug)]
pub struct DagDiff {
    /// Node hashes present in `to` but not in `from`.
    pub added: HashSet<ContentHash>,
    /// Node hashes present in `from` but not in `to`.
    pub removed: HashSet<ContentHash>,
    /// Node hashes present in both but with different content (same logical path, different hash).
    /// In practice, this is the intersection of parents of added/removed — nodes that were rebuilt.
    pub changed: HashSet<ContentHash>,
}

/// Diff two DAG states by comparing the reachable node sets.
pub fn dag_diff(dag: &Dag, from: &ContentHash, to: &ContentHash) -> DagDiff {
    if from == to {
        return DagDiff {
            added: HashSet::new(),
            removed: HashSet::new(),
            changed: HashSet::new(),
        };
    }

    let from_set = collect_reachable(dag, from);
    let to_set = collect_reachable(dag, to);

    let added: HashSet<ContentHash> = to_set.difference(&from_set).copied().collect();
    let removed: HashSet<ContentHash> = from_set.difference(&to_set).copied().collect();

    // "Changed" nodes are those that are both added and removed at the same logical position.
    // For now, we report the new root hashes that differ — consumers can inspect further.
    let changed = if from != to && !added.is_empty() && !removed.is_empty() {
        // The roots themselves changed
        let mut ch = HashSet::new();
        ch.insert(*from);
        ch.insert(*to);
        ch
    } else {
        HashSet::new()
    };

    DagDiff { added, removed, changed }
}

/// Collect all node hashes reachable from a root.
fn collect_reachable(dag: &Dag, root: &ContentHash) -> HashSet<ContentHash> {
    let mut visited = HashSet::new();
    let mut stack = vec![*root];

    while let Some(hash) = stack.pop() {
        if !visited.insert(hash) {
            continue;
        }
        for child in dag.children(&hash) {
            if !visited.contains(&child) {
                stack.push(child);
            }
        }
    }

    visited
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node::Node;
    use tempfile::TempDir;

    fn test_dag() -> (Dag, TempDir) {
        let dir = TempDir::new().unwrap();
        let store = hd_cas::ContentStore::open(dir.path()).unwrap();
        (Dag::new(store), dir)
    }

    #[test]
    fn diff_identical_roots() {
        let (mut dag, _dir) = test_dag();
        let file = Node::file("a.rs", ContentHash::from_bytes(b"a"));
        let fh = dag.insert(file).unwrap();
        let env = Node::env("app", vec![fh]);
        let eh = dag.insert(env).unwrap();

        let diff = dag_diff(&dag, &eh, &eh);
        assert!(diff.added.is_empty());
        assert!(diff.removed.is_empty());
        assert!(diff.changed.is_empty());
    }

    #[test]
    fn diff_added_node() {
        let (mut dag, _dir) = test_dag();
        let f1 = Node::file("a.rs", ContentHash::from_bytes(b"a"));
        let fh1 = dag.insert(f1).unwrap();
        let env1 = Node::env("app", vec![fh1]);
        let eh1 = dag.insert(env1).unwrap();

        let f2 = Node::file("b.rs", ContentHash::from_bytes(b"b"));
        let fh2 = dag.insert(f2).unwrap();
        let env2 = Node::env("app", vec![fh1, fh2]);
        let eh2 = dag.insert(env2).unwrap();

        let diff = dag_diff(&dag, &eh1, &eh2);
        assert_eq!(diff.added.len(), 1);
        assert!(diff.added.contains(&fh2));
        assert!(diff.removed.is_empty());
    }

    #[test]
    fn diff_removed_node() {
        let (mut dag, _dir) = test_dag();
        let f1 = Node::file("a.rs", ContentHash::from_bytes(b"a"));
        let f2 = Node::file("b.rs", ContentHash::from_bytes(b"b"));
        let fh1 = dag.insert(f1).unwrap();
        let fh2 = dag.insert(f2).unwrap();
        let env1 = Node::env("app", vec![fh1, fh2]);
        let eh1 = dag.insert(env1).unwrap();

        let env2 = Node::env("app", vec![fh1]);
        let eh2 = dag.insert(env2).unwrap();

        let diff = dag_diff(&dag, &eh1, &eh2);
        assert!(diff.added.is_empty());
        assert_eq!(diff.removed.len(), 1);
        assert!(diff.removed.contains(&fh2));
    }

    #[test]
    fn diff_changed_is_symmetric_add_remove() {
        let (mut dag, _dir) = test_dag();
        let f1 = Node::file("a.rs", ContentHash::from_bytes(b"v1"));
        let fh1 = dag.insert(f1).unwrap();
        let env1 = Node::env("app", vec![fh1]);
        let eh1 = dag.insert(env1).unwrap();

        let f2 = Node::file("a.rs", ContentHash::from_bytes(b"v2"));
        let fh2 = dag.insert(f2).unwrap();
        let env2 = Node::env("app", vec![fh2]);
        let eh2 = dag.insert(env2).unwrap();

        let diff = dag_diff(&dag, &eh1, &eh2);
        assert!(diff.added.contains(&fh2));
        assert!(diff.removed.contains(&fh1));
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p hd-engine diff::tests`
Expected: 4 tests PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/hd-engine/src/diff.rs
git commit -m "feat(hd-engine): add DAG diffing between environment states"
```

---

## Task 13: Engine Public API & Integration Tests

**Files:**
- Modify: `crates/hd-engine/src/lib.rs`
- Create: `crates/hd-engine/tests/integration.rs`

- [ ] **Step 1: Update lib.rs with public API re-exports**

```rust
// crates/hd-engine/src/lib.rs
pub mod node;
pub mod dag;
pub mod invalidation;
pub mod diff;

// Re-export key types
pub use node::Node;
pub use dag::{Dag, DagError};
pub use invalidation::{invalidate, FileChange, InvalidationResult};
pub use diff::{dag_diff, DagDiff};
```

- [ ] **Step 2: Write integration tests**

```rust
// crates/hd-engine/tests/integration.rs
use hd_cas::{ContentHash, ContentStore};
use hd_engine::{Node, Dag, invalidate, dag_diff, FileChange};
use tempfile::TempDir;

#[test]
fn full_invalidation_cycle() {
    let dir = TempDir::new().unwrap();
    let store = ContentStore::open(dir.path()).unwrap();
    let mut dag = Dag::new(store);

    // Build initial DAG: env -> dir("src") -> {main.rs, lib.rs}
    let main_v1 = Node::file("src/main.rs", ContentHash::from_bytes(b"main_v1"));
    let lib_v1 = Node::file("src/lib.rs", ContentHash::from_bytes(b"lib_v1"));
    let main_hash = dag.insert(main_v1).unwrap();
    let lib_hash = dag.insert(lib_v1).unwrap();

    let src_dir = Node::dir("src", vec![
        ("main.rs".into(), main_hash),
        ("lib.rs".into(), lib_hash),
    ]);
    let src_hash = dag.insert(src_dir).unwrap();
    let env = Node::env("app", vec![src_hash]);
    let env_hash = dag.insert(env).unwrap();

    // Modify main.rs
    let main_v2 = Node::file("src/main.rs", ContentHash::from_bytes(b"main_v2"));
    let change = FileChange {
        old_hash: main_hash,
        new_node: main_v2,
    };
    let result = invalidate(&mut dag, &env_hash, &[change]).unwrap();

    // Root changed
    assert_ne!(result.new_root, env_hash);

    // Diff old and new
    let diff = dag_diff(&dag, &env_hash, &result.new_root);
    assert!(!diff.added.is_empty(), "should have new nodes");
    assert!(!diff.removed.is_empty(), "should have removed old nodes");

    // lib.rs should not be in the diff
    assert!(!diff.added.contains(&lib_hash));
    assert!(!diff.removed.contains(&lib_hash));
}

#[test]
fn multiple_changes_in_one_pass() {
    let dir = TempDir::new().unwrap();
    let store = ContentStore::open(dir.path()).unwrap();
    let mut dag = Dag::new(store);

    let f1 = Node::file("a.rs", ContentHash::from_bytes(b"a1"));
    let f2 = Node::file("b.rs", ContentHash::from_bytes(b"b1"));
    let fh1 = dag.insert(f1).unwrap();
    let fh2 = dag.insert(f2).unwrap();
    let dir_node = Node::dir("src", vec![
        ("a.rs".into(), fh1),
        ("b.rs".into(), fh2),
    ]);
    let dh = dag.insert(dir_node).unwrap();
    let env = Node::env("app", vec![dh]);
    let eh = dag.insert(env).unwrap();

    // Both files change
    let changes = vec![
        FileChange {
            old_hash: fh1,
            new_node: Node::file("a.rs", ContentHash::from_bytes(b"a2")),
        },
        FileChange {
            old_hash: fh2,
            new_node: Node::file("b.rs", ContentHash::from_bytes(b"b2")),
        },
    ];

    let result = invalidate(&mut dag, &eh, &changes).unwrap();
    assert_ne!(result.new_root, eh);
    assert_eq!(result.invalidated.len(), 4); // 2 files + dir + env
}
```

- [ ] **Step 3: Run all tests**

Run: `cargo test -p hd-engine`
Expected: All tests PASS (node: 7, dag: 5, invalidation: 3, diff: 4, integration: 2 = 21 total).

- [ ] **Step 4: Run full workspace tests**

Run: `cargo test`
Expected: All tests PASS across both crates (~50 total).

- [ ] **Step 5: Commit**

```bash
git add crates/hd-engine/src/lib.rs crates/hd-engine/tests/integration.rs
git commit -m "feat(hd-engine): add public API re-exports and integration tests"
```

---

## Task 14: Add .gitignore and Final Cleanup

**Files:**
- Create: `.gitignore`

- [ ] **Step 1: Create .gitignore**

```
/target
```

- [ ] **Step 2: Run final full test suite**

Run: `cargo test`
Expected: All tests PASS.

Run: `cargo clippy -- -D warnings`
Expected: No warnings (fix any that appear).

- [ ] **Step 3: Commit**

```bash
git add .gitignore
git commit -m "chore: add .gitignore"
```

---

## Summary

| Task | Component | Tests |
|------|-----------|-------|
| 1 | Workspace setup | compile check |
| 2 | `ContentHash` type | 5 |
| 3 | CDC chunking | 5 |
| 4 | `Manifest` type | 4 |
| 5 | `ContentStore` | 8 |
| 6 | Garbage collector | 4 |
| 7 | CAS integration tests | 3 |
| 8 | CAS public API | compile check |
| 9 | DAG node types | 7 |
| 10 | DAG construction & query | 5 |
| 11 | Incremental invalidation | 3 |
| 12 | DAG diffing | 4 |
| 13 | Engine public API & integration | 2 |
| 14 | .gitignore & cleanup | lint check |

**Total: 14 tasks, ~50 tests, 14 commits.**

## Next Plans

After this plan is complete, the following plans should be written in order:

1. **`hd-spec`** — TOML environment spec parsing, dependency provider trait, lockfile generation
2. **`hd-mount`** — FUSE filesystem projection from the DAG
3. **`hd-watch`** — File watching and DAG invalidation integration
4. **`hd-sandbox`** — Process isolation and service management
5. **`hd-oci`** — OCI image pulling and Dockerfile translation
6. **`hd-cli`** — CLI client, daemon, Unix socket protocol

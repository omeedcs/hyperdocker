//! # hd-cas
//!
//! Content-addressable store for hyperdocker.
//!
//! This crate provides a BLAKE3-hashed, content-defined chunked storage system.
//! Files are split into variable-size chunks using FastCDC, hashed with BLAKE3,
//! and stored with optional zstd compression. Identical content is automatically
//! deduplicated across environments.
//!
//! ## Key Types
//!
//! - [`ContentHash`] - A 256-bit BLAKE3 content hash
//! - [`ContentStore`] - On-disk content-addressable store
//! - [`Manifest`] - File manifest (ordered list of chunk hashes)
//! - [`GarbageCollector`] - Reference-counting garbage collector
//!
//! ## Example
//!
//! ```no_run
//! use hd_cas::{ContentStore, ContentHash};
//! use std::path::Path;
//!
//! let store = ContentStore::open(Path::new("/tmp/cas")).unwrap();
//! let hash = store.put_file(Path::new("myfile.txt")).unwrap();
//! store.get_file(&hash, Path::new("recovered.txt")).unwrap();
//! ```

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

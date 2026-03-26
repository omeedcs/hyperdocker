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

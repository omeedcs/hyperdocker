//! # hd-engine
//!
//! Merkle DAG engine with incremental invalidation for hyperdocker.
//!
//! This crate implements the core Merkle DAG data structure that tracks all files
//! in an environment. Each node represents a file or directory with its content
//! hash. When a file changes, the DAG is incrementally invalidated from the
//! changed leaf up to the root, avoiding a full re-hash of the entire tree.
//!
//! ## Key Types
//!
//! - [`Node`] - A single node in the Merkle DAG (file or directory)
//! - [`Dag`] - The full Merkle DAG tree structure
//! - [`FileChange`] - Represents a detected change to a file
//! - [`DagDiff`] - Computed diff between two DAG snapshots
//!
//! ## Example
//!
//! ```no_run
//! use hd_engine::{Dag, FileChange};
//! use hd_cas::ContentStore;
//! use std::path::Path;
//!
//! let store = ContentStore::open(Path::new("/tmp/cas")).unwrap();
//! let dag = Dag::build(Path::new("/my/project"), &store).unwrap();
//! println!("root hash: {:?}", dag.root_hash());
//! ```

pub mod node;
pub mod dag;
pub mod invalidation;
pub mod diff;
pub mod ingest;

// Re-export key types
pub use node::Node;
pub use dag::{Dag, DagError};
pub use invalidation::{invalidate, FileChange, InvalidationResult};
pub use diff::{dag_diff, DagDiff};
pub use ingest::{ingest_tree, IngestResult, IngestError};

pub mod node;
pub mod dag;
pub mod invalidation;
pub mod diff;

// Re-export key types
pub use node::Node;
pub use dag::{Dag, DagError};
pub use invalidation::{invalidate, FileChange, InvalidationResult};
pub use diff::{dag_diff, DagDiff};

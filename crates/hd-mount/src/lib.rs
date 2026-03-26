//! # hd-mount
//!
//! FUSE filesystem projection from a Merkle DAG for hyperdocker.
//!
//! This crate implements a virtual filesystem layer that projects content from
//! the CAS store through the Merkle DAG into a mount point. It uses FUSE to
//! present a read-only view of the environment's files, with overlay support
//! for writable layers.
//!
//! ## Key Types
//!
//! - [`ProjectedFs`] - Projects DAG nodes as filesystem entries
//! - [`Overlay`] - Writable overlay on top of the projected filesystem
//! - [`FuseFs`] - FUSE filesystem implementation
//! - [`MountManager`] - Manages mount/unmount lifecycle
//! - [`MountHandle`] - Handle to an active mount point
//!
//! ## Example
//!
//! ```no_run
//! use hd_mount::MountManager;
//! use std::path::Path;
//!
//! let manager = MountManager::new(
//!     Path::new("/tmp/cas"),
//!     Path::new("/tmp/mnt"),
//! ).unwrap();
//! let handle = manager.mount().unwrap();
//! // filesystem is now projected at /tmp/mnt
//! ```

pub mod projected;
pub mod overlay;
pub mod fuse;
pub mod manager;

pub use projected::ProjectedFs;
pub use overlay::Overlay;
pub use fuse::FuseFs;
pub use manager::{MountManager, MountHandle, MountState};

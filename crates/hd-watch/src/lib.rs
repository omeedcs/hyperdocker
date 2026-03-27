//! # hd-watch
//!
//! File system watcher with debouncing and DAG invalidation for hyperdocker.
//!
//! This crate monitors the host filesystem for changes and feeds them into the
//! Merkle DAG invalidation pipeline. It uses `notify` for cross-platform file
//! watching, applies configurable path filters (respecting `.gitignore`-style
//! patterns), and debounces rapid changes to avoid thrashing.
//!
//! ## Key Types
//!
//! - [`FileWatcher`] - Main watcher that monitors directories for changes
//! - [`PathFilter`] - Configurable filter for include/exclude path patterns
//! - [`Debouncer`] - Batches rapid filesystem events into coalesced changes
//! - [`PathMap`] - Maps host paths to DAG node paths
//!
//! ## Example
//!
//! ```no_run
//! use hd_watch::{FileWatcher, PathFilter};
//! use std::path::Path;
//!
//! let filter = PathFilter::new(vec!["src/**".into()], vec!["target/**".into()]);
//! let mut watcher = FileWatcher::new(Path::new("."), filter).unwrap();
//! for change in watcher.poll_changes() {
//!     println!("changed: {:?}", change);
//! }
//! ```

pub mod pathmap;
pub mod filter;
pub mod debounce;
pub mod watcher;

pub use pathmap::PathMap;
pub use filter::PathFilter;
pub use debounce::{Debouncer, RawChange, ChangeKind};
pub use watcher::{FileWatcher, WatchError};

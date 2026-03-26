//! # hd-sandbox
//!
//! Process sandbox with service lifecycle management for hyperdocker.
//!
//! This crate provides a lightweight process sandbox for running services and
//! build commands within a hyperdocker environment. It manages process spawning,
//! signal forwarding, restart policies, and graceful shutdown. Services are
//! defined in the `hd.toml` spec and supervised by the sandbox.
//!
//! ## Key Types
//!
//! - [`Sandbox`] - Top-level sandbox managing all processes and services
//! - [`ManagedProcess`] - A spawned process with signal and lifecycle control
//! - [`Service`] - A long-running service with health checks and restart policy
//! - [`ServiceDef`] - Service definition from the environment spec
//!
//! ## Example
//!
//! ```no_run
//! use hd_sandbox::{Sandbox, ServiceDef};
//!
//! let sandbox = Sandbox::new("/tmp/env").unwrap();
//! sandbox.start_service(ServiceDef {
//!     name: "redis".into(),
//!     command: "redis-server".into(),
//!     restart: Default::default(),
//! }).unwrap();
//! ```

pub mod process;
pub mod service;
pub mod sandbox;

pub use process::ManagedProcess;
pub use service::{Service, ServiceDef, ServiceState, RestartPolicy};
pub use sandbox::Sandbox;

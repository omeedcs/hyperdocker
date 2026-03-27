//! # hd-spec
//!
//! TOML environment spec parser and dependency provider framework for hyperdocker.
//!
//! This crate parses `hd.toml` specification files that declare an environment's
//! dependencies, services, build steps, and file mappings. It also provides a
//! pluggable dependency provider system for resolving packages from different
//! sources, and a lockfile mechanism for reproducible builds.
//!
//! ## Key Types
//!
//! - [`EnvSpec`] - Parsed `hd.toml` environment specification
//! - [`DependencyProvider`] - Trait for pluggable dependency resolution
//! - [`ProviderRegistry`] - Registry of available dependency providers
//! - [`Lockfile`] - Lockfile for pinning resolved dependency versions
//! - [`compile`] - Compiles a spec into a materialized DAG
//!
//! ## Example
//!
//! ```no_run
//! use hd_spec::EnvSpec;
//! use std::path::Path;
//!
//! let spec = EnvSpec::load(Path::new("hd.toml")).unwrap();
//! println!("environment: {}", spec.config.name);
//! for dep in &spec.config.deps {
//!     println!("  dep: {} = {}", dep.name, dep.version);
//! }
//! ```

pub mod spec;
pub mod provider;
pub mod compiler;
pub mod lockfile;

pub use spec::{EnvSpec, EnvironmentConfig, DependencySpec, BuildConfig, ServiceConfig, FilesConfig, OptionsConfig, RestartPolicy};
pub use provider::{DependencyProvider, ResolvedDependency, ProviderRegistry, ProviderError};
pub use compiler::{compile, compile_with_files, CompileError};
pub use lockfile::{Lockfile, LockedDependency, LockfileError};

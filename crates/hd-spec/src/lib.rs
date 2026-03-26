pub mod spec;
pub mod provider;
pub mod compiler;
pub mod lockfile;

pub use spec::{EnvSpec, EnvironmentConfig, DependencySpec, BuildConfig, ServiceConfig, FilesConfig, OptionsConfig, RestartPolicy};
pub use provider::{DependencyProvider, ResolvedDependency, ProviderRegistry, ProviderError};
pub use compiler::{compile, CompileError};
pub use lockfile::{Lockfile, LockedDependency, LockfileError};

use std::collections::HashMap;

use hd_cas::ContentHash;

use crate::spec::DependencySpec;

/// A resolved dependency with exact version and content hash.
#[derive(Debug, Clone)]
pub struct ResolvedDependency {
    pub provider: String,
    pub name: String,
    pub version: String,
    pub artifact_hash: ContentHash,
}

#[derive(Debug, thiserror::Error)]
pub enum ProviderError {
    #[error("provider '{0}' not found")]
    NotFound(String),
    #[error("resolution failed: {0}")]
    ResolutionFailed(String),
}

/// Trait for dependency providers (apt, npm, pip, cargo).
/// Each provider knows how to resolve a DependencySpec into concrete,
/// content-addressed artifacts.
pub trait DependencyProvider {
    /// Provider name (e.g., "apt", "npm").
    fn name(&self) -> &str;

    /// Resolve a dependency specification into concrete resolved dependencies.
    fn resolve(&self, spec: &DependencySpec) -> Result<Vec<ResolvedDependency>, ProviderError>;
}

/// Registry of available dependency providers.
pub struct ProviderRegistry {
    providers: HashMap<String, Box<dyn DependencyProvider>>,
}

impl ProviderRegistry {
    pub fn new() -> Self {
        ProviderRegistry {
            providers: HashMap::new(),
        }
    }

    /// Register a provider.
    pub fn register(&mut self, provider: Box<dyn DependencyProvider>) {
        let name = provider.name().to_string();
        self.providers.insert(name, provider);
    }

    /// Get a provider by name.
    pub fn get(&self, name: &str) -> Option<&dyn DependencyProvider> {
        self.providers.get(name).map(|p| p.as_ref())
    }

    /// Resolve all dependencies in a spec using registered providers.
    /// Dependencies whose provider name doesn't match a registered provider
    /// are returned as an error.
    pub fn resolve_all(
        &self,
        deps: &HashMap<String, DependencySpec>,
    ) -> Result<Vec<ResolvedDependency>, ProviderError> {
        let mut all_resolved = Vec::new();
        for (provider_name, spec) in deps {
            let provider = self
                .get(provider_name)
                .ok_or_else(|| ProviderError::NotFound(provider_name.clone()))?;
            let resolved = provider.resolve(spec)?;
            all_resolved.extend(resolved);
        }
        Ok(all_resolved)
    }
}

impl Default for ProviderRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockProvider;

    impl DependencyProvider for MockProvider {
        fn name(&self) -> &str {
            "mock"
        }

        fn resolve(&self, spec: &DependencySpec) -> Result<Vec<ResolvedDependency>, ProviderError> {
            match spec {
                DependencySpec::Packages(pkgs) => {
                    Ok(pkgs.iter().map(|p| ResolvedDependency {
                        provider: "mock".to_string(),
                        name: p.clone(),
                        version: "1.0.0".to_string(),
                        artifact_hash: hd_cas::ContentHash::from_bytes(p.as_bytes()),
                    }).collect())
                }
                DependencySpec::Version(v) => {
                    Ok(vec![ResolvedDependency {
                        provider: "mock".to_string(),
                        name: "pkg".to_string(),
                        version: v.clone(),
                        artifact_hash: hd_cas::ContentHash::from_bytes(v.as_bytes()),
                    }])
                }
                DependencySpec::FileRef { file } => {
                    Ok(vec![ResolvedDependency {
                        provider: "mock".to_string(),
                        name: file.clone(),
                        version: "from-file".to_string(),
                        artifact_hash: hd_cas::ContentHash::from_bytes(file.as_bytes()),
                    }])
                }
            }
        }
    }

    #[test]
    fn registry_register_and_get() {
        let mut registry = ProviderRegistry::new();
        registry.register(Box::new(MockProvider));
        assert!(registry.get("mock").is_some());
        assert!(registry.get("nonexistent").is_none());
    }

    #[test]
    fn resolve_packages_spec() {
        let provider = MockProvider;
        let spec = DependencySpec::Packages(vec!["curl".into(), "git".into()]);
        let resolved = provider.resolve(&spec).unwrap();
        assert_eq!(resolved.len(), 2);
        assert_eq!(resolved[0].name, "curl");
        assert_eq!(resolved[1].name, "git");
    }

    #[test]
    fn resolve_version_spec() {
        let provider = MockProvider;
        let spec = DependencySpec::Version("20.x".into());
        let resolved = provider.resolve(&spec).unwrap();
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].version, "20.x");
    }

    #[test]
    fn resolve_file_ref_spec() {
        let provider = MockProvider;
        let spec = DependencySpec::FileRef { file: "package.json".into() };
        let resolved = provider.resolve(&spec).unwrap();
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].name, "package.json");
    }

    #[test]
    fn registry_resolve_all() {
        let mut registry = ProviderRegistry::new();
        registry.register(Box::new(MockProvider));

        let mut deps = std::collections::HashMap::new();
        deps.insert("mock".to_string(), DependencySpec::Packages(vec!["a".into(), "b".into()]));

        let resolved = registry.resolve_all(&deps).unwrap();
        assert_eq!(resolved.len(), 2);
    }
}

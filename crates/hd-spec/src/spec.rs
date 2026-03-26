use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Top-level environment specification parsed from hd.toml.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvSpec {
    pub environment: EnvironmentConfig,
    #[serde(default)]
    pub dependencies: HashMap<String, DependencySpec>,
    #[serde(default)]
    pub build: BuildConfig,
    #[serde(default)]
    pub services: HashMap<String, ServiceConfig>,
    #[serde(default)]
    pub files: FilesConfig,
    #[serde(default)]
    pub options: OptionsConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvironmentConfig {
    pub name: String,
    pub base: String,
}

/// A dependency can be specified as:
/// - A list of strings: `apt = ["curl", "git"]`
/// - A version string: `node = "20.x"`
/// - A table with a file reference: `npm = { file = "package.json" }`
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum DependencySpec {
    Packages(Vec<String>),
    Version(String),
    FileRef { file: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BuildConfig {
    #[serde(default)]
    pub steps: Vec<String>,
    #[serde(default)]
    pub cache: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceConfig {
    pub command: String,
    #[serde(default)]
    pub watch: Vec<String>,
    #[serde(default)]
    pub port: Option<u16>,
    #[serde(default)]
    pub depends_on: Vec<String>,
    #[serde(default)]
    pub restart_policy: RestartPolicy,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum RestartPolicy {
    #[default]
    Always,
    OnFailure,
    Never,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FilesConfig {
    #[serde(default)]
    pub include: Vec<String>,
    #[serde(default)]
    pub exclude: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptionsConfig {
    #[serde(default = "default_restart_grace")]
    pub restart_grace: String,
}

impl Default for OptionsConfig {
    fn default() -> Self {
        OptionsConfig {
            restart_grace: default_restart_grace(),
        }
    }
}

fn default_restart_grace() -> String {
    "5s".to_string()
}

#[derive(Debug, thiserror::Error)]
pub enum SpecError {
    #[error("TOML parse error: {0}")]
    TomlParse(#[from] toml::de::Error),
    #[error("validation error: {0}")]
    Validation(String),
}

impl EnvSpec {
    /// Parse an EnvSpec from a TOML string.
    pub fn from_toml(input: &str) -> Result<Self, SpecError> {
        let spec: EnvSpec = toml::from_str(input)?;
        Ok(spec)
    }

    /// Parse an EnvSpec from a file path.
    pub fn from_file(path: &std::path::Path) -> Result<Self, SpecError> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| SpecError::Validation(format!("cannot read file: {}", e)))?;
        Self::from_toml(&content)
    }

    /// Validate the spec for logical consistency.
    pub fn validate(&self) -> Result<(), SpecError> {
        self.validate_service_dependencies()?;
        Ok(())
    }

    fn validate_service_dependencies(&self) -> Result<(), SpecError> {
        // Check that all depends_on references exist
        for (name, service) in &self.services {
            for dep in &service.depends_on {
                if !self.services.contains_key(dep) {
                    return Err(SpecError::Validation(format!(
                        "service '{}' depends on '{}' which does not exist",
                        name, dep
                    )));
                }
            }
        }
        // Check for circular dependencies using DFS
        for name in self.services.keys() {
            let mut visited = std::collections::HashSet::new();
            let mut stack = vec![name.as_str()];
            while let Some(current) = stack.pop() {
                if !visited.insert(current) {
                    return Err(SpecError::Validation(format!(
                        "circular dependency detected involving service '{}'",
                        current
                    )));
                }
                if let Some(service) = self.services.get(current) {
                    for dep in &service.depends_on {
                        stack.push(dep);
                    }
                }
            }
        }
        Ok(())
    }

    /// Serialize the spec back to TOML.
    pub fn to_toml(&self) -> Result<String, SpecError> {
        toml::to_string_pretty(self)
            .map_err(|e| SpecError::Validation(format!("serialization error: {}", e)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINIMAL_SPEC: &str = r#"
[environment]
name = "myapp"
base = "ubuntu:22.04"
"#;

    const FULL_SPEC: &str = r#"
[environment]
name = "myapp"
base = "ubuntu:22.04"

[dependencies]
apt = ["curl", "git", "build-essential"]
node = "20.x"

[dependencies.npm]
file = "package.json"

[build]
steps = [
    "npm install",
    "npm run build",
]
cache = ["node_modules", "dist"]

[services.web]
command = "npm run dev"
watch = ["src/**/*.ts", "src/**/*.tsx"]
port = 3000

[services.worker]
command = "node worker.js"
watch = ["worker.js", "lib/**"]
depends_on = ["web"]

[files]
include = ["src", "public", "package.json", "tsconfig.json"]
exclude = [".git", "node_modules/.cache", "*.log"]

[options]
restart_grace = "5s"
"#;

    #[test]
    fn parse_minimal_spec() {
        let spec = EnvSpec::from_toml(MINIMAL_SPEC).unwrap();
        assert_eq!(spec.environment.name, "myapp");
        assert_eq!(spec.environment.base, "ubuntu:22.04");
        assert!(spec.dependencies.is_empty());
        assert!(spec.build.steps.is_empty());
        assert!(spec.services.is_empty());
    }

    #[test]
    fn parse_full_spec() {
        let spec = EnvSpec::from_toml(FULL_SPEC).unwrap();
        assert_eq!(spec.environment.name, "myapp");
        assert_eq!(spec.environment.base, "ubuntu:22.04");

        assert_eq!(spec.dependencies.len(), 3);

        assert_eq!(spec.build.steps.len(), 2);
        assert_eq!(spec.build.steps[0], "npm install");
        assert_eq!(spec.build.cache.len(), 2);

        assert_eq!(spec.services.len(), 2);
        let web = &spec.services["web"];
        assert_eq!(web.command, "npm run dev");
        assert_eq!(web.watch.len(), 2);
        assert_eq!(web.port, Some(3000));
        assert!(web.depends_on.is_empty());

        let worker = &spec.services["worker"];
        assert_eq!(worker.depends_on, vec!["web"]);

        assert_eq!(spec.files.include.len(), 4);
        assert_eq!(spec.files.exclude.len(), 3);

        assert_eq!(spec.options.restart_grace, "5s");
    }

    #[test]
    fn parse_invalid_toml_returns_error() {
        let result = EnvSpec::from_toml("not valid [toml");
        assert!(result.is_err());
    }

    #[test]
    fn parse_missing_environment_returns_error() {
        let result = EnvSpec::from_toml("[build]\nsteps = []");
        assert!(result.is_err());
    }

    #[test]
    fn validate_duplicate_service_dependency() {
        let toml = r#"
[environment]
name = "test"
base = "ubuntu:22.04"

[services.web]
command = "node server.js"
depends_on = ["worker"]

[services.worker]
command = "node worker.js"
depends_on = ["web"]
"#;
        let spec = EnvSpec::from_toml(toml).unwrap();
        let result = spec.validate();
        assert!(result.is_err());
    }
}

use serde::{Deserialize, Serialize};
use std::path::Path;

use hd_cas::ContentHash;

/// A locked dependency with exact version and artifact hash.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LockedDependency {
    pub provider: String,
    pub name: String,
    pub version: String,
    pub artifact_hash: ContentHash,
}

/// The lockfile: a deterministic snapshot of all resolved dependencies.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Lockfile {
    pub dependencies: Vec<LockedDependency>,
}

#[derive(Debug, thiserror::Error)]
pub enum LockfileError {
    #[error("TOML parse error: {0}")]
    TomlParse(String),
    #[error("TOML serialize error: {0}")]
    TomlSerialize(String),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

impl Lockfile {
    pub fn new() -> Self {
        Lockfile {
            dependencies: Vec::new(),
        }
    }

    /// Add a locked dependency. The list is kept sorted for determinism.
    pub fn add(&mut self, dep: LockedDependency) {
        self.dependencies.push(dep);
        self.dependencies
            .sort_by(|a, b| a.provider.cmp(&b.provider).then(a.name.cmp(&b.name)));
    }

    /// Serialize to TOML string.
    pub fn to_toml(&self) -> Result<String, LockfileError> {
        toml::to_string_pretty(self).map_err(|e| LockfileError::TomlSerialize(e.to_string()))
    }

    /// Parse from TOML string.
    pub fn from_toml(input: &str) -> Result<Self, LockfileError> {
        toml::from_str(input).map_err(|e| LockfileError::TomlParse(e.to_string()))
    }

    /// Write to a file.
    pub fn write_to_file(&self, path: &Path) -> Result<(), LockfileError> {
        let content = self.to_toml()?;
        std::fs::write(path, content)?;
        Ok(())
    }

    /// Read from a file.
    pub fn from_file(path: &Path) -> Result<Self, LockfileError> {
        let content = std::fs::read_to_string(path)?;
        Self::from_toml(&content)
    }

    /// Compute a content hash of the entire lockfile for change detection.
    pub fn content_hash(&self) -> ContentHash {
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"lockfile:");
        for dep in &self.dependencies {
            hasher.update(dep.provider.as_bytes());
            hasher.update(dep.name.as_bytes());
            hasher.update(dep.version.as_bytes());
            hasher.update(dep.artifact_hash.as_bytes());
        }
        ContentHash::from_raw(*hasher.finalize().as_bytes())
    }
}

impl Default for Lockfile {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hd_cas::ContentHash;

    #[test]
    fn lockfile_roundtrip() {
        let mut lock = Lockfile::new();
        lock.add(LockedDependency {
            provider: "apt".into(),
            name: "curl".into(),
            version: "7.88.1-10".into(),
            artifact_hash: ContentHash::from_bytes(b"curl-artifact"),
        });
        lock.add(LockedDependency {
            provider: "npm".into(),
            name: "express".into(),
            version: "4.18.2".into(),
            artifact_hash: ContentHash::from_bytes(b"express-artifact"),
        });

        let toml_str = lock.to_toml().unwrap();
        let parsed = Lockfile::from_toml(&toml_str).unwrap();

        assert_eq!(parsed.dependencies.len(), 2);
        assert_eq!(parsed.dependencies[0].name, "curl");
        assert_eq!(parsed.dependencies[1].name, "express");
    }

    #[test]
    fn lockfile_sorted_deterministic() {
        let mut lock1 = Lockfile::new();
        lock1.add(LockedDependency {
            provider: "npm".into(),
            name: "b".into(),
            version: "1.0".into(),
            artifact_hash: ContentHash::from_bytes(b"b"),
        });
        lock1.add(LockedDependency {
            provider: "apt".into(),
            name: "a".into(),
            version: "1.0".into(),
            artifact_hash: ContentHash::from_bytes(b"a"),
        });

        let mut lock2 = Lockfile::new();
        lock2.add(LockedDependency {
            provider: "apt".into(),
            name: "a".into(),
            version: "1.0".into(),
            artifact_hash: ContentHash::from_bytes(b"a"),
        });
        lock2.add(LockedDependency {
            provider: "npm".into(),
            name: "b".into(),
            version: "1.0".into(),
            artifact_hash: ContentHash::from_bytes(b"b"),
        });

        // Regardless of insertion order, TOML output should be identical
        assert_eq!(lock1.to_toml().unwrap(), lock2.to_toml().unwrap());
    }

    #[test]
    fn lockfile_hash_deterministic() {
        let mut lock = Lockfile::new();
        lock.add(LockedDependency {
            provider: "apt".into(),
            name: "curl".into(),
            version: "7.88".into(),
            artifact_hash: ContentHash::from_bytes(b"curl"),
        });

        let h1 = lock.content_hash();
        let h2 = lock.content_hash();
        assert_eq!(h1, h2);
    }

    #[test]
    fn lockfile_file_roundtrip() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("hd.lock");

        let mut lock = Lockfile::new();
        lock.add(LockedDependency {
            provider: "pip".into(),
            name: "flask".into(),
            version: "3.0.0".into(),
            artifact_hash: ContentHash::from_bytes(b"flask"),
        });

        lock.write_to_file(&path).unwrap();
        let loaded = Lockfile::from_file(&path).unwrap();
        assert_eq!(loaded.dependencies.len(), 1);
        assert_eq!(loaded.dependencies[0].name, "flask");
    }
}

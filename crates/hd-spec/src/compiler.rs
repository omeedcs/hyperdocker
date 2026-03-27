use hd_cas::ContentHash;
use hd_engine::{Dag, DagError, Node};

use crate::provider::{ProviderError, ProviderRegistry};
use crate::spec::EnvSpec;

use hd_engine::ingest_tree;

#[derive(Debug, thiserror::Error)]
pub enum CompileError {
    #[error("DAG error: {0}")]
    Dag(#[from] DagError),
    #[error("provider error: {0}")]
    Provider(#[from] ProviderError),
}

/// Compile an EnvSpec into a DAG, returning the root EnvNode hash.
///
/// The compilation produces:
/// 1. A PackageNode for the base image reference
/// 2. PackageNodes for each resolved dependency
/// 3. BuildStepNodes for each build step (chained: each depends on the previous)
/// 4. An EnvNode that ties everything together
pub fn compile(
    spec: &EnvSpec,
    registry: &ProviderRegistry,
    dag: &mut Dag,
) -> Result<ContentHash, CompileError> {
    let mut children = Vec::new();

    // 1. Base image as a PackageNode
    let base_node = Node::package(
        "oci",
        &spec.environment.base,
        "latest",
        ContentHash::from_bytes(spec.environment.base.as_bytes()),
    );
    let base_hash = dag.insert(base_node)?;
    children.push(base_hash);

    // 2. Resolve and insert dependencies
    let resolved = registry.resolve_all(&spec.dependencies)?;
    for dep in &resolved {
        let pkg_node = Node::package(
            &dep.provider,
            &dep.name,
            &dep.version,
            dep.artifact_hash,
        );
        let pkg_hash = dag.insert(pkg_node)?;
        children.push(pkg_hash);
    }

    // 3. Build steps (chained: each step's inputs include the previous step)
    let mut prev_inputs: Vec<ContentHash> = children.clone();
    for step_cmd in &spec.build.steps {
        let step_node = Node::build_step(step_cmd, prev_inputs.clone(), vec![]);
        let step_hash = dag.insert(step_node)?;
        children.push(step_hash);
        prev_inputs = vec![step_hash];
    }

    // 4. Root EnvNode
    let env_node = Node::env(&spec.environment.name, children);
    let env_hash = dag.insert(env_node)?;

    Ok(env_hash)
}

/// Compile an EnvSpec into a DAG, including project files from disk.
///
/// This is the same as [`compile`] but also ingests the project file tree from
/// `project_dir` using the `[files]` section of the spec, and adds the
/// resulting file-tree root hash as a child of the Env node.
pub fn compile_with_files(
    spec: &EnvSpec,
    registry: &ProviderRegistry,
    dag: &mut Dag,
    project_dir: &std::path::Path,
    store: &hd_cas::ContentStore,
) -> Result<ContentHash, CompileError> {
    let mut children = Vec::new();

    // 1. Base image as a PackageNode
    let base_node = Node::package(
        "oci",
        &spec.environment.base,
        "latest",
        ContentHash::from_bytes(spec.environment.base.as_bytes()),
    );
    let base_hash = dag.insert(base_node)?;
    children.push(base_hash);

    // 2. Resolve and insert dependencies
    let resolved = registry.resolve_all(&spec.dependencies)?;
    for dep in &resolved {
        let pkg_node = Node::package(
            &dep.provider,
            &dep.name,
            &dep.version,
            dep.artifact_hash,
        );
        let pkg_hash = dag.insert(pkg_node)?;
        children.push(pkg_hash);
    }

    // 3. Ingest project files from disk
    let includes = &spec.files.include;
    let excludes = &spec.files.exclude;
    let ingest_result = ingest_tree(project_dir, includes, excludes, store, dag)
        .map_err(|e| CompileError::Dag(DagError::Serialization(e.to_string())))?;
    children.push(ingest_result.root_hash);

    // 4. Build steps (chained: each step's inputs include the previous step)
    let mut prev_inputs: Vec<ContentHash> = children.clone();
    for step_cmd in &spec.build.steps {
        let step_node = Node::build_step(step_cmd, prev_inputs.clone(), vec![]);
        let step_hash = dag.insert(step_node)?;
        children.push(step_hash);
        prev_inputs = vec![step_hash];
    }

    // 5. Root EnvNode
    let env_node = Node::env(&spec.environment.name, children);
    let env_hash = dag.insert(env_node)?;

    Ok(env_hash)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::{DependencyProvider, ProviderError, ProviderRegistry, ResolvedDependency};
    use crate::spec::{DependencySpec, EnvSpec};
    use tempfile::TempDir;

    struct MockProvider;

    impl DependencyProvider for MockProvider {
        fn name(&self) -> &str {
            "mock"
        }
        fn resolve(&self, spec: &DependencySpec) -> Result<Vec<ResolvedDependency>, ProviderError> {
            match spec {
                DependencySpec::Packages(pkgs) => Ok(pkgs
                    .iter()
                    .map(|p| ResolvedDependency {
                        provider: "mock".to_string(),
                        name: p.clone(),
                        version: "1.0.0".to_string(),
                        artifact_hash: ContentHash::from_bytes(p.as_bytes()),
                    })
                    .collect()),
                _ => Ok(vec![]),
            }
        }
    }

    fn setup() -> (ProviderRegistry, hd_cas::ContentStore, TempDir) {
        let dir = TempDir::new().unwrap();
        let store = hd_cas::ContentStore::open(dir.path()).unwrap();
        let mut registry = ProviderRegistry::new();
        registry.register(Box::new(MockProvider));
        (registry, store, dir)
    }

    #[test]
    fn compile_minimal_spec() {
        let (registry, store, _dir) = setup();
        let spec = EnvSpec::from_toml(
            r#"
[environment]
name = "test"
base = "ubuntu:22.04"
"#,
        )
        .unwrap();

        let mut dag = hd_engine::Dag::new(store);
        let result = compile(&spec, &registry, &mut dag).unwrap();

        // Should produce an EnvNode
        let root = dag.get(&result).unwrap();
        match root {
            hd_engine::Node::Env { name, children } => {
                assert_eq!(name, "test");
                assert!(!children.is_empty()); // at least the base node
            }
            _ => panic!("expected EnvNode"),
        }
    }

    #[test]
    fn compile_with_dependencies() {
        let (registry, store, _dir) = setup();
        let spec = EnvSpec::from_toml(
            r#"
[environment]
name = "test"
base = "ubuntu:22.04"

[dependencies]
mock = ["curl", "git"]
"#,
        )
        .unwrap();

        let mut dag = hd_engine::Dag::new(store);
        let result = compile(&spec, &registry, &mut dag).unwrap();
        let root = dag.get(&result).unwrap();

        match root {
            hd_engine::Node::Env { children, .. } => {
                // base + 2 package nodes = 3 children
                assert!(
                    children.len() >= 3,
                    "expected at least 3 children, got {}",
                    children.len()
                );
            }
            _ => panic!("expected EnvNode"),
        }
    }

    #[test]
    fn compile_with_build_steps() {
        let (registry, store, _dir) = setup();
        let spec = EnvSpec::from_toml(
            r#"
[environment]
name = "test"
base = "ubuntu:22.04"

[build]
steps = ["make build", "make install"]
"#,
        )
        .unwrap();

        let mut dag = hd_engine::Dag::new(store);
        let result = compile(&spec, &registry, &mut dag).unwrap();
        let root = dag.get(&result).unwrap();

        match root {
            hd_engine::Node::Env { children, .. } => {
                // base + 2 build step nodes = 3 children
                assert!(
                    children.len() >= 3,
                    "expected at least 3 children, got {}",
                    children.len()
                );
            }
            _ => panic!("expected EnvNode"),
        }
    }

    #[test]
    fn compile_deterministic() {
        let (registry, store1, _dir1) = setup();
        let spec = EnvSpec::from_toml(
            r#"
[environment]
name = "test"
base = "ubuntu:22.04"

[dependencies]
mock = ["curl"]

[build]
steps = ["make"]
"#,
        )
        .unwrap();

        let mut dag1 = hd_engine::Dag::new(store1);
        let h1 = compile(&spec, &registry, &mut dag1).unwrap();

        let (registry2, store2, _dir2) = setup();
        let mut dag2 = hd_engine::Dag::new(store2);
        let h2 = compile(&spec, &registry2, &mut dag2).unwrap();

        assert_eq!(h1, h2);
    }

    #[test]
    fn compile_with_project_files() {
        let (registry, store, _store_dir) = setup();

        // Create a temp project dir with some Python files
        let project_dir = TempDir::new().unwrap();
        std::fs::write(project_dir.path().join("app.py"), b"print('hello')").unwrap();
        std::fs::create_dir(project_dir.path().join("src")).unwrap();
        std::fs::write(project_dir.path().join("src").join("util.py"), b"def helper(): pass").unwrap();

        let spec = EnvSpec::from_toml(
            r#"
[environment]
name = "pyapp"
base = "python:3.11"

[files]
include = ["*.py", "src"]
exclude = []
"#,
        )
        .unwrap();

        // Open a second handle to the same store so we can pass it to compile_with_files
        // without conflicting borrows on `dag`.
        let store_dir = TempDir::new().unwrap();
        let store2 = hd_cas::ContentStore::open(store_dir.path()).unwrap();
        let mut dag = hd_engine::Dag::new(store);
        let result = compile_with_files(&spec, &registry, &mut dag, project_dir.path(), &store2).unwrap();

        let root = dag.get(&result).unwrap();
        match root {
            hd_engine::Node::Env { name, children } => {
                assert_eq!(name, "pyapp");
                // At minimum: base image node + file tree root
                assert!(
                    children.len() >= 2,
                    "expected at least 2 children (base + file tree), got {}",
                    children.len()
                );
            }
            _ => panic!("expected EnvNode"),
        }
    }
}

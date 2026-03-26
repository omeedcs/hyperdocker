use hd_cas::ContentHash;
use hd_cas::ContentStore;
use hd_engine::Dag;
use hd_spec::{
    compile, DependencyProvider, DependencySpec, EnvSpec, LockedDependency, Lockfile, ProviderError,
    ProviderRegistry, ResolvedDependency,
};
use tempfile::TempDir;

struct TestProvider;

impl DependencyProvider for TestProvider {
    fn name(&self) -> &str {
        "test"
    }
    fn resolve(&self, spec: &DependencySpec) -> Result<Vec<ResolvedDependency>, ProviderError> {
        match spec {
            DependencySpec::Packages(pkgs) => Ok(pkgs
                .iter()
                .map(|p| ResolvedDependency {
                    provider: "test".to_string(),
                    name: p.clone(),
                    version: "1.0.0".to_string(),
                    artifact_hash: ContentHash::from_bytes(p.as_bytes()),
                })
                .collect()),
            _ => Ok(vec![]),
        }
    }
}

#[test]
fn full_spec_to_dag_to_lockfile() {
    let dir = TempDir::new().unwrap();
    let store = ContentStore::open(dir.path()).unwrap();
    let mut registry = ProviderRegistry::new();
    registry.register(Box::new(TestProvider));

    let spec = EnvSpec::from_toml(
        r#"
[environment]
name = "integration-test"
base = "node:20-alpine"

[dependencies]
test = ["express", "lodash"]

[build]
steps = ["npm install", "npm run build"]

[services.web]
command = "node server.js"
watch = ["src/**/*.js"]
port = 3000

[files]
include = ["src", "package.json"]
exclude = [".git"]
"#,
    )
    .unwrap();

    spec.validate().unwrap();

    let mut dag = Dag::new(store);
    let root_hash = compile(&spec, &registry, &mut dag).unwrap();

    // Root should be an EnvNode
    let root = dag.get(&root_hash).unwrap();
    match root {
        hd_engine::Node::Env { name, children } => {
            assert_eq!(name, "integration-test");
            // base(1) + deps(2) + build_steps(2) = 5 children
            assert_eq!(children.len(), 5);
        }
        _ => panic!("expected EnvNode"),
    }

    // Generate lockfile from resolved deps
    let resolved = registry.resolve_all(&spec.dependencies).unwrap();
    let mut lockfile = Lockfile::new();
    for dep in resolved {
        lockfile.add(LockedDependency {
            provider: dep.provider,
            name: dep.name,
            version: dep.version,
            artifact_hash: dep.artifact_hash,
        });
    }

    let lock_path = dir.path().join("hd.lock");
    lockfile.write_to_file(&lock_path).unwrap();
    let loaded = Lockfile::from_file(&lock_path).unwrap();
    assert_eq!(loaded.dependencies.len(), 2);
}

#[test]
fn spec_file_roundtrip() {
    let dir = TempDir::new().unwrap();
    let spec_path = dir.path().join("hd.toml");

    let spec = EnvSpec::from_toml(
        r#"
[environment]
name = "roundtrip"
base = "ubuntu:22.04"

[build]
steps = ["make"]
"#,
    )
    .unwrap();

    let toml_out = spec.to_toml().unwrap();
    std::fs::write(&spec_path, &toml_out).unwrap();

    let loaded = EnvSpec::from_file(&spec_path).unwrap();
    assert_eq!(loaded.environment.name, "roundtrip");
    assert_eq!(loaded.build.steps, vec!["make"]);
}

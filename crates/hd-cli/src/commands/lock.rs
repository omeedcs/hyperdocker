pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let spec = hd_spec::EnvSpec::from_file(std::path::Path::new("hd.toml"))?;
    let registry = hd_spec::ProviderRegistry::new();
    let resolved = registry.resolve_all(&spec.dependencies)?;

    let mut lockfile = hd_spec::Lockfile::new();
    for dep in resolved {
        lockfile.add(hd_spec::LockedDependency {
            provider: dep.provider,
            name: dep.name,
            version: dep.version,
            artifact_hash: dep.artifact_hash,
        });
    }

    lockfile.write_to_file(std::path::Path::new("hd.lock"))?;
    println!(
        "Generated hd.lock ({} dependencies)",
        lockfile.dependencies.len()
    );
    Ok(())
}

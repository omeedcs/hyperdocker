pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let spec = hd_spec::EnvSpec::from_file(std::path::Path::new("hd.toml"))?;
    spec.validate()?;

    // Set up CAS
    let cas_dir = dirs::home_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join(".hd")
        .join("cas");
    let store = hd_cas::ContentStore::open(&cas_dir)?;
    let gc = hd_cas::GarbageCollector::new(&cas_dir)?;

    // Compile spec to DAG
    let registry = hd_spec::ProviderRegistry::new();
    let mut dag = hd_engine::Dag::new(store);
    let root_hash = hd_spec::compile(&spec, &registry, &mut dag)?;

    println!("Environment '{}' built", spec.environment.name);
    println!("DAG root: {}", root_hash);
    println!(
        "Services: {}",
        spec.services
            .keys()
            .cloned()
            .collect::<Vec<_>>()
            .join(", ")
    );

    // Add ref for GC
    gc.add_ref(&root_hash)?;

    Ok(())
}

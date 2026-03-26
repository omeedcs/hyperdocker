pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::path::Path::new("hd.toml");
    if !path.exists() {
        return Err("No hd.toml found. Run 'hd init' first.".into());
    }

    let spec = hd_spec::EnvSpec::from_file(path)?;
    println!("Environment: {}", spec.environment.name);
    println!("Base: {}", spec.environment.base);
    println!("Services:");
    for (name, svc) in &spec.services {
        println!("  {} — {} (watch: {:?})", name, svc.command, svc.watch);
    }
    if spec.services.is_empty() {
        println!("  (none defined)");
    }
    Ok(())
}

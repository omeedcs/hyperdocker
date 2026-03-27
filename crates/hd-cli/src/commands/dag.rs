use crate::render;

pub fn run_show() -> Result<(), Box<dyn std::error::Error>> {
    let spec = hd_spec::EnvSpec::from_file(std::path::Path::new("hd.toml"))?;
    let cas_dir = dirs::home_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join(".hd")
        .join("cas");
    let store = hd_cas::ContentStore::open(&cas_dir)?;
    let registry = hd_spec::ProviderRegistry::new();
    let mut dag = hd_engine::Dag::new(store);
    let root = hd_spec::compile(&spec, &registry, &mut dag)?;
    println!("DAG root: {}", root);
    println!();
    render::render_tree(&dag, &root);
    Ok(())
}

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
    print_dag_tree(&dag, &root, 0);
    Ok(())
}

fn print_dag_tree(dag: &hd_engine::Dag, hash: &hd_cas::ContentHash, depth: usize) {
    let indent = "  ".repeat(depth);
    if let Some(node) = dag.get(hash) {
        match node {
            hd_engine::Node::Env { name, children } => {
                println!("{}Env({})", indent, name);
                for child in children {
                    print_dag_tree(dag, child, depth + 1);
                }
            }
            hd_engine::Node::Dir { path, children } => {
                println!("{}Dir({})", indent, path);
                for (name, child) in children {
                    println!("{}  {}: {}", indent, name, child);
                }
            }
            hd_engine::Node::File {
                path,
                manifest_hash,
            } => {
                println!(
                    "{}File({}) -> {}",
                    indent,
                    path,
                    &manifest_hash.to_hex()[..12]
                );
            }
            hd_engine::Node::Package {
                provider,
                name,
                version,
                ..
            } => {
                println!("{}Pkg({}/{} {})", indent, provider, name, version);
            }
            hd_engine::Node::BuildStep { command, .. } => {
                println!("{}Build({})", indent, command);
            }
        }
    }
}

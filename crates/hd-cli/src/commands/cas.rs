pub fn run_stats() -> Result<(), Box<dyn std::error::Error>> {
    let cas_dir = dirs::home_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join(".hd")
        .join("cas");

    if !cas_dir.exists() {
        println!("No CAS found. Run 'hd up' first.");
        return Ok(());
    }

    let store = hd_cas::ContentStore::open(&cas_dir)?;
    let chunks = store.list_chunks()?;
    let manifests = store.list_manifests()?;
    println!("CAS Statistics:");
    println!("  Chunks: {}", chunks.len());
    println!("  Manifests: {}", manifests.len());
    Ok(())
}

pub fn run_gc() -> Result<(), Box<dyn std::error::Error>> {
    let cas_dir = dirs::home_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join(".hd")
        .join("cas");

    if !cas_dir.exists() {
        println!("No CAS found. Nothing to collect.");
        return Ok(());
    }

    let store = hd_cas::ContentStore::open(&cas_dir)?;
    let gc = hd_cas::GarbageCollector::new(&cas_dir)?;
    let stats = gc.collect(&store)?;
    println!("Garbage collection complete:");
    println!("  Manifests removed: {}", stats.manifests_removed);
    println!("  Chunks removed: {}", stats.chunks_removed);
    Ok(())
}

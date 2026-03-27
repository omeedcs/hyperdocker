use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

use hd_cas::ContentHash;

use crate::render;

// ANSI color codes
const RESET: &str = "\x1b[0m";
const GREEN: &str = "\x1b[32m";
const YELLOW: &str = "\x1b[33m";
const RED: &str = "\x1b[31m";

/// Persisted build state at ~/.hd/state.json
#[derive(serde::Serialize, serde::Deserialize)]
struct BuildState {
    /// Root DAG hash as a 64-character hex string.
    root_hash: String,
    /// Map of relative file path -> manifest hash hex string.
    file_hashes: BTreeMap<String, String>,
    /// Unix epoch seconds as a string.
    timestamp: String,
}

fn state_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".hd")
        .join("state.json")
}

fn cas_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".hd")
        .join("cas")
}

fn load_state() -> Option<BuildState> {
    let path = state_path();
    let data = std::fs::read_to_string(&path).ok()?;
    serde_json::from_str(&data).ok()
}

fn save_state(root_hash: &ContentHash, file_hashes: &BTreeMap<String, ContentHash>) {
    let path = state_path();

    // Ensure parent directory exists.
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .to_string();

    let state = BuildState {
        root_hash: root_hash.to_hex(),
        file_hashes: file_hashes
            .iter()
            .map(|(k, v)| (k.clone(), v.to_hex()))
            .collect(),
        timestamp,
    };

    if let Ok(json) = serde_json::to_string_pretty(&state) {
        let _ = std::fs::write(&path, json);
    }
}

pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let start = Instant::now();

    // 1. Load and validate spec from hd.toml
    let spec = hd_spec::EnvSpec::from_file(Path::new("hd.toml"))?;
    spec.validate()?;

    // 2. Set up CAS path and GC
    let cas_path = cas_dir();
    let gc = hd_cas::GarbageCollector::new(&cas_path)?;

    // 3. Load previous state (before compilation)
    let prev_state = load_state();

    // 4. Compile spec with file ingestion.
    //    compile_with_files takes a &ContentStore for the file ingest and
    //    ownership of a separate ContentStore inside the Dag.
    let registry = hd_spec::ProviderRegistry::new();
    let dag_store = hd_cas::ContentStore::open(&cas_path)?;
    let file_store = hd_cas::ContentStore::open(&cas_path)?;
    let mut dag = hd_engine::Dag::new(dag_store);
    let project_dir = std::env::current_dir()?;
    let root_hash = hd_spec::compile_with_files(&spec, &registry, &mut dag, &project_dir, &file_store)?;

    // 5. Separately ingest the file tree to obtain per-file hashes.
    //    This is a no-op in the CAS (chunks already exist), but gives us
    //    file_hashes for diffing against the previous state.
    let ingest_store = hd_cas::ContentStore::open(&cas_path)?;
    let mut ingest_dag = hd_engine::Dag::new(hd_cas::ContentStore::open(&cas_path)?);
    let ingest_result = hd_engine::ingest_tree(
        &project_dir,
        &spec.files.include,
        &spec.files.exclude,
        &ingest_store,
        &mut ingest_dag,
    )?;

    let elapsed = start.elapsed();

    // 6. Print build summary
    let service_list = {
        let names: Vec<&str> = spec.services.keys().map(|s| s.as_str()).collect();
        if names.is_empty() {
            "none".to_string()
        } else {
            names.join(", ")
        }
    };

    println!(
        "Environment '{}' built in {:.1}ms",
        spec.environment.name,
        elapsed.as_secs_f64() * 1000.0
    );
    println!("DAG root: {}", root_hash);
    println!(
        "Files: {} ({})  Services: {}",
        ingest_result.file_count,
        render::format_bytes(ingest_result.total_bytes),
        service_list,
    );

    // 7. Show diff if a previous state exists
    if let Some(prev) = &prev_state {
        if let Ok(prev_hash) = ContentHash::from_hex(&prev.root_hash) {
            if prev_hash != root_hash {
                // Build maps for comparison
                let prev_files: BTreeMap<&str, &str> = prev
                    .file_hashes
                    .iter()
                    .map(|(k, v)| (k.as_str(), v.as_str()))
                    .collect();
                let curr_files: BTreeMap<&str, String> = ingest_result
                    .file_hashes
                    .iter()
                    .map(|(k, v)| (k.as_str(), v.to_hex()))
                    .collect();

                let mut changed = 0usize;
                let mut added = 0usize;
                let mut removed = 0usize;
                let mut unchanged = 0usize;

                // Detect changed and added files
                for (path, curr_hex) in &curr_files {
                    match prev_files.get(path) {
                        Some(prev_hex) => {
                            if prev_hex != curr_hex {
                                println!("  {}~ {}{}", YELLOW, path, RESET);
                                changed += 1;
                            } else {
                                unchanged += 1;
                            }
                        }
                        None => {
                            println!("  {}+ {}{}", GREEN, path, RESET);
                            added += 1;
                        }
                    }
                }

                // Detect removed files
                for path in prev_files.keys() {
                    if !curr_files.contains_key(path) {
                        println!("  {}- {}{}", RED, path, RESET);
                        removed += 1;
                    }
                }

                println!();
                println!(
                    "{}{} changed{}  {}{} added{}  {}{} removed{}  {} unchanged",
                    YELLOW, changed, RESET,
                    GREEN, added, RESET,
                    RED, removed, RESET,
                    unchanged,
                );
            } else {
                println!("\nNo changes since last build.");
            }
        }
    }

    // 8. Persist new state and register GC reference
    save_state(&root_hash, &ingest_result.file_hashes);
    gc.add_ref(&root_hash)?;

    Ok(())
}

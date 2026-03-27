use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Instant;

use crate::commands::up::StubProvider;

const BENCH_RUNS: usize = 5;

// ── ANSI helpers ─────────────────────────────────────────────────────────────
const BOLD: &str = "\x1b[1m";
const GREEN: &str = "\x1b[32m";
const YELLOW: &str = "\x1b[33m";
const DIM: &str = "\x1b[2m";
const RESET: &str = "\x1b[0m";

fn bold(s: &str) -> String { format!("{}{}{}", BOLD, s, RESET) }
fn green(s: &str) -> String { format!("{}{}{}", GREEN, s, RESET) }
fn yellow(s: &str) -> String { format!("{}{}{}", YELLOW, s, RESET) }
fn dim(s: &str) -> String { format!("{}{}{}", DIM, s, RESET) }

// ── Entry point ───────────────────────────────────────────────────────────────

pub fn run(path: Option<&str>) -> Result<(), Box<dyn std::error::Error>> {
    let project_dir = match path {
        Some(p) => PathBuf::from(p),
        None => find_demo_project()?,
    };

    println!();
    println!("{}", bold("=== Hyperdocker Demo & Benchmark ==="));
    println!("Project: {}", project_dir.display());
    println!();

    // ── Phase 0: Prerequisites ────────────────────────────────────────────────
    verify_prerequisites(&project_dir)?;

    // ── Phase 1: Docker cold build ────────────────────────────────────────────
    println!("{}", bold("Phase 1: Docker cold build (--no-cache)…"));
    let docker_cold_ms = docker_build_timed(&project_dir, true)?;
    println!("  Done in {:.0} ms\n", docker_cold_ms);

    // ── Phase 2: Hyperdocker cold build ───────────────────────────────────────
    println!("{}", bold("Phase 2: Hyperdocker cold build…"));
    let (hd_cold_ms, _) = hd_build_timed(&project_dir)?;
    println!("  Done in {:.1} ms\n", hd_cold_ms);

    // ── Phase 3: Show file diff preview ──────────────────────────────────────
    println!("{}", bold("Phase 3: File Change + Diff Preview"));
    show_diff_preview(&project_dir)?;
    println!();

    // ── Phase 4: Warm rebuild benchmark (BENCH_RUNS runs, median) ─────────────
    println!("{}", bold("Phase 4: Rebuild Comparison"));
    println!("  Running {} iterations each (median reported)…\n", BENCH_RUNS);

    let app_py = project_dir.join("app.py");
    let original = std::fs::read_to_string(&app_py)?;
    let modified = original.replace(
        "Hello from Hyperdocker!",
        "Hello from Hyperdocker! (modified)",
    );

    let mut docker_warm_times: Vec<f64> = Vec::with_capacity(BENCH_RUNS);
    let mut hd_warm_times: Vec<f64> = Vec::with_capacity(BENCH_RUNS);

    for i in 0..BENCH_RUNS {
        print!("  Run {}/{}…", i + 1, BENCH_RUNS);

        // Restore original, do a baseline build so Docker cache is warm
        std::fs::write(&app_py, &original)?;
        // Baseline Docker build to warm the cache
        let _ = docker_build_timed(&project_dir, false)?;
        // Baseline hd build
        let _ = hd_build_timed(&project_dir)?;

        // Apply modification
        std::fs::write(&app_py, &modified)?;

        // Timed Docker warm rebuild
        let d_ms = docker_build_timed(&project_dir, false)?;
        docker_warm_times.push(d_ms);

        // Timed Hyperdocker warm rebuild
        let (h_ms, _) = hd_build_timed(&project_dir)?;
        hd_warm_times.push(h_ms);

        println!("  Docker {:.0} ms  |  Hyperdocker {:.1} ms", d_ms, h_ms);
    }

    // Restore original app.py
    std::fs::write(&app_py, &original)?;

    let docker_median = median(&docker_warm_times);
    let hd_median = median(&hd_warm_times);
    let speedup = docker_median / hd_median.max(0.001);

    // ── Phase 5: Results table ────────────────────────────────────────────────
    println!();
    println!("{}", bold("Phase 5: Results"));
    println!("{}", bold("----------------------------------------------------------"));
    println!("{:<40} {:>12}", bold("Metric"), bold("Time"));
    println!("{}", bold("----------------------------------------------------------"));
    println!("{:<40} {:>12}", "Docker cold build", format!("{:.0} ms", docker_cold_ms));
    println!("{:<40} {:>12}", "Hyperdocker cold build", format!("{:.1} ms", hd_cold_ms));
    println!("{:<40} {:>12}", "Docker warm rebuild (median)", format!("{:.0} ms", docker_median));
    println!("{:<40} {:>12}", "Hyperdocker warm rebuild (median)", format!("{:.1} ms", hd_median));
    println!("{}", bold("----------------------------------------------------------"));
    println!();
    println!("{}", green(&format!("Hyperdocker is {:.0}x faster on warm rebuilds", speedup)));
    println!();
    println!("{}", dim("Note: Docker executes all Dockerfile build steps when any file changes."));
    println!("{}", dim("      Hyperdocker tracks changes at the file level via content-addressed"));
    println!("{}", dim("      storage (CAS). Unchanged files are never re-hashed or re-uploaded,"));
    println!("{}", dim("      so incremental rebuilds only pay for what actually changed."));
    println!();

    Ok(())
}

// ── Demo project finder ───────────────────────────────────────────────────────

fn find_demo_project() -> Result<PathBuf, Box<dyn std::error::Error>> {
    let cwd = std::env::current_dir()?;

    // 1. examples/flask-demo under CWD
    let candidate = cwd.join("examples").join("flask-demo");
    if is_demo_project(&candidate) {
        return Ok(candidate);
    }

    // 2. examples/flask-demo under parent dir
    if let Some(parent) = cwd.parent() {
        let candidate = parent.join("examples").join("flask-demo");
        if is_demo_project(&candidate) {
            return Ok(candidate);
        }
    }

    // 3. ~/.hd/demo/flask-demo
    if let Some(home) = dirs::home_dir() {
        let candidate = home.join(".hd").join("demo").join("flask-demo");
        if is_demo_project(&candidate) {
            return Ok(candidate);
        }
    }

    Err(concat!(
        "Could not find the flask-demo project.\n",
        "Provide a path with: hd demo <path>\n",
        "Or run from the hyperdocker repo root so 'examples/flask-demo' is found automatically."
    ).into())
}

fn is_demo_project(dir: &Path) -> bool {
    dir.is_dir()
        && dir.join("Dockerfile").exists()
        && dir.join("hd.toml").exists()
}

// ── Prerequisites check ───────────────────────────────────────────────────────

fn verify_prerequisites(project_dir: &Path) -> Result<(), Box<dyn std::error::Error>> {
    // Docker installed?
    let docker_ok = Command::new("docker")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);

    if !docker_ok {
        return Err("Docker is not installed or not on PATH. Install Docker to run this demo.".into());
    }

    // Dockerfile present?
    if !project_dir.join("Dockerfile").exists() {
        return Err(format!("No Dockerfile found in {}", project_dir.display()).into());
    }

    // hd.toml present?
    if !project_dir.join("hd.toml").exists() {
        return Err(format!("No hd.toml found in {}", project_dir.display()).into());
    }

    println!("Prerequisites: {}", green("OK"));
    println!();
    Ok(())
}

// ── Timed build helpers ───────────────────────────────────────────────────────

fn hd_build_timed(dir: &Path) -> Result<(f64, hd_cas::ContentHash), Box<dyn std::error::Error>> {
    let start = Instant::now();
    let spec = hd_spec::EnvSpec::from_file(&dir.join("hd.toml"))?;
    let cas_path = dirs::home_dir()
        .unwrap_or_default()
        .join(".hd")
        .join("cas");
    let store = hd_cas::ContentStore::open(&cas_path)?;
    let file_store = hd_cas::ContentStore::open(&cas_path)?;
    let mut registry = hd_spec::ProviderRegistry::new();
    for provider_name in spec.dependencies.keys() {
        registry.register(Box::new(StubProvider {
            provider_name: provider_name.clone(),
        }));
    }
    let mut dag = hd_engine::Dag::new(store);
    let root_hash = hd_spec::compile_with_files(&spec, &registry, &mut dag, dir, &file_store)?;
    let elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;
    Ok((elapsed_ms, root_hash))
}

fn docker_build_timed(dir: &Path, no_cache: bool) -> Result<f64, Box<dyn std::error::Error>> {
    let mut cmd = Command::new("docker");
    cmd.arg("build").arg("-t").arg("hd-demo-bench").arg(".");
    if no_cache {
        cmd.arg("--no-cache");
    }
    cmd.current_dir(dir);
    cmd.stdout(Stdio::null()).stderr(Stdio::null());
    let start = Instant::now();
    let output = cmd.output()?;
    let elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;
    if !output.status.success() {
        return Err(format!(
            "Docker build failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    Ok(elapsed_ms)
}

// ── Median ────────────────────────────────────────────────────────────────────

fn median(values: &[f64]) -> f64 {
    let mut sorted = values.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    sorted[sorted.len() / 2]
}

// ── Visual diff preview ───────────────────────────────────────────────────────

fn ingest_file_hashes(
    project_dir: &Path,
    spec: &hd_spec::EnvSpec,
) -> Result<BTreeMap<String, String>, Box<dyn std::error::Error>> {
    let cas_path = dirs::home_dir()
        .unwrap_or_default()
        .join(".hd")
        .join("cas");
    let store = hd_cas::ContentStore::open(&cas_path)?;
    let mut dag = hd_engine::Dag::new(hd_cas::ContentStore::open(&cas_path)?);
    let result = hd_engine::ingest_tree(
        project_dir,
        &spec.files.include,
        &spec.files.exclude,
        &store,
        &mut dag,
    )?;
    Ok(result
        .file_hashes
        .into_iter()
        .map(|(k, v)| (k, v.to_hex()))
        .collect())
}

fn show_diff_preview(project_dir: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let spec = hd_spec::EnvSpec::from_file(&project_dir.join("hd.toml"))?;
    let app_py = project_dir.join("app.py");

    // Ingest before modification
    let before = ingest_file_hashes(project_dir, &spec)?;

    // Apply modification to app.py
    let original = std::fs::read_to_string(&app_py)?;
    let modified = original.replace(
        "Hello from Hyperdocker!",
        "Hello from Hyperdocker! (modified)",
    );
    std::fs::write(&app_py, &modified)?;

    // Ingest after modification
    let after = ingest_file_hashes(project_dir, &spec)?;

    // Restore original immediately
    std::fs::write(&app_py, &original)?;

    // Compare and print actual diff
    println!("  Visual DAG diff (after modifying app.py):");
    let mut all_keys: std::collections::BTreeSet<&str> = std::collections::BTreeSet::new();
    for k in before.keys() {
        all_keys.insert(k.as_str());
    }
    for k in after.keys() {
        all_keys.insert(k.as_str());
    }

    for path in &all_keys {
        match (before.get(*path), after.get(*path)) {
            (Some(b), Some(a)) if b != a => {
                println!("    {}  (content changed)", yellow(path));
            }
            (Some(_), Some(_)) => {
                println!("    {}  (unchanged)", dim(path));
            }
            (None, Some(_)) => {
                println!("    {}  (added)", green(path));
            }
            (Some(_), None) => {
                println!("    {}  (removed)", yellow(path));
            }
            (None, None) => {}
        }
    }

    Ok(())
}

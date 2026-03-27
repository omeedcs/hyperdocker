use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use hd_cas::{ContentHash, ContentStore};

use crate::dag::{Dag, DagError};
use crate::node::Node;

/// Result of ingesting a directory tree.
#[derive(Debug)]
pub struct IngestResult {
    /// Root Dir node hash.
    pub root_hash: ContentHash,
    /// Number of files ingested.
    pub file_count: usize,
    /// Total bytes ingested.
    pub total_bytes: u64,
    /// Map of relative path string -> manifest hash.
    pub file_hashes: BTreeMap<String, ContentHash>,
}

/// Errors that can occur during ingestion.
#[derive(Debug, thiserror::Error)]
pub enum IngestError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("store error: {0}")]
    Store(#[from] hd_cas::store::StoreError),
    #[error("DAG error: {0}")]
    Dag(#[from] DagError),
    #[error("directory not found: {0}")]
    DirNotFound(PathBuf),
}

/// Default path components/patterns that are always excluded.
const DEFAULT_EXCLUDES: &[&str] = &[
    ".git",
    "target",
    "node_modules",
    "__pycache__",
    ".DS_Store",
    "*.pyc",
    ".hd",
];

/// Check if a relative path should be excluded.
/// Returns true if the path itself or any component matches any exclude pattern.
fn is_excluded(rel_path: &str, user_excludes: &[String]) -> bool {
    let all_excludes: Vec<&str> = DEFAULT_EXCLUDES
        .iter()
        .copied()
        .chain(user_excludes.iter().map(|s| s.as_str()))
        .collect();

    for pattern_str in &all_excludes {
        if let Ok(pattern) = glob::Pattern::new(pattern_str) {
            // Check the full path
            if pattern.matches(rel_path) {
                return true;
            }
            // Check each individual path component
            for component in Path::new(rel_path).components() {
                let comp_str = component.as_os_str().to_string_lossy();
                if pattern.matches(comp_str.as_ref()) {
                    return true;
                }
            }
        }
    }
    false
}

/// Check if a relative path should be included.
/// If includes is empty, include everything. Otherwise, the path or any parent must match.
fn is_included(rel_path: &str, includes: &[String]) -> bool {
    if includes.is_empty() {
        return true;
    }
    for pattern_str in includes {
        if let Ok(pattern) = glob::Pattern::new(pattern_str) {
            // Check the full path
            if pattern.matches(rel_path) {
                return true;
            }
            // Check each ancestor directory component so that "templates" includes
            // "templates/index.html"
            let path = Path::new(rel_path);
            let mut ancestor = PathBuf::new();
            for component in path.components() {
                ancestor.push(component);
                let ancestor_str = ancestor.to_string_lossy();
                if pattern.matches(ancestor_str.as_ref()) {
                    return true;
                }
            }
        }
    }
    false
}

/// Walk a directory recursively, collecting (relative_path_string, absolute_path) pairs.
fn walk_dir(root: &Path, current: &Path, results: &mut Vec<(String, PathBuf)>) -> Result<(), IngestError> {
    for entry in std::fs::read_dir(current)? {
        let entry = entry?;
        let abs_path = entry.path();
        let rel_path = abs_path
            .strip_prefix(root)
            .expect("absolute path is always under root")
            .to_string_lossy()
            .to_string();

        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            walk_dir(root, &abs_path, results)?;
        } else if file_type.is_file() {
            results.push((rel_path, abs_path));
        }
        // Skip symlinks and other special files
    }
    Ok(())
}

/// Build the Dir tree bottom-up from a set of (rel_path, manifest_hash) file entries.
/// Returns the root Dir node's content hash.
fn build_dir_tree(
    dir_name: &str,
    files: &[(String, ContentHash)],
    dag: &mut Dag,
) -> Result<ContentHash, IngestError> {
    // Separate files at this level from files in subdirectories.
    let mut direct_children: Vec<(String, ContentHash)> = Vec::new();
    // Group sub-entries by their first component (subdirectory name).
    let mut subdirs: BTreeMap<String, Vec<(String, ContentHash)>> = BTreeMap::new();

    for (rel_path, manifest_hash) in files {
        if let Some(slash_pos) = rel_path.find('/') {
            let subdir_name = &rel_path[..slash_pos];
            let remainder = &rel_path[slash_pos + 1..];
            subdirs
                .entry(subdir_name.to_string())
                .or_default()
                .push((remainder.to_string(), *manifest_hash));
        } else {
            // This file is directly under the current directory.
            direct_children.push((rel_path.clone(), *manifest_hash));
        }
    }

    // Build child file nodes.
    let mut children: Vec<(String, ContentHash)> = Vec::new();
    for (file_name, manifest_hash) in direct_children {
        let file_node = Node::file(&file_name, manifest_hash);
        let file_hash = dag.insert(file_node)?;
        children.push((file_name, file_hash));
    }

    // Recursively build subdirectory nodes.
    for (subdir_name, sub_files) in subdirs {
        let sub_hash = build_dir_tree(&subdir_name, &sub_files, dag)?;
        children.push((subdir_name, sub_hash));
    }

    let dir_node = Node::dir(dir_name, children);
    let dir_hash = dag.insert(dir_node)?;
    Ok(dir_hash)
}

/// Ingest a directory tree into the CAS and DAG.
///
/// - Walks `root_dir` recursively.
/// - Filters using `includes` and `excludes` glob patterns.
/// - Ingests each file into `store`.
/// - Creates File and Dir DAG nodes in `dag`.
/// - Returns an [`IngestResult`] with the root hash and statistics.
pub fn ingest_tree(
    root_dir: &Path,
    includes: &[String],
    excludes: &[String],
    store: &ContentStore,
    dag: &mut Dag,
) -> Result<IngestResult, IngestError> {
    if !root_dir.exists() || !root_dir.is_dir() {
        return Err(IngestError::DirNotFound(root_dir.to_path_buf()));
    }

    // Collect all files under root_dir.
    let mut all_files: Vec<(String, PathBuf)> = Vec::new();
    walk_dir(root_dir, root_dir, &mut all_files)?;

    // Filter by exclude and include patterns.
    let filtered: Vec<(String, PathBuf)> = all_files
        .into_iter()
        .filter(|(rel_path, _)| {
            !is_excluded(rel_path, excludes) && is_included(rel_path, includes)
        })
        .collect();

    // Ingest each file into the CAS.
    let mut file_count = 0usize;
    let mut total_bytes = 0u64;
    let mut file_hashes: BTreeMap<String, ContentHash> = BTreeMap::new();
    let mut entries: Vec<(String, ContentHash)> = Vec::new();

    for (rel_path, abs_path) in &filtered {
        let manifest_hash = store.put_file(abs_path)?;
        let byte_count = std::fs::metadata(abs_path)?.len();
        total_bytes += byte_count;
        file_count += 1;
        file_hashes.insert(rel_path.clone(), manifest_hash);
        entries.push((rel_path.clone(), manifest_hash));
    }

    // Build the Dir tree and get root hash.
    let root_hash = build_dir_tree(".", &entries, dag)?;

    Ok(IngestResult {
        root_hash,
        file_count,
        total_bytes,
        file_hashes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use hd_cas::ContentStore;
    use tempfile::TempDir;

    /// Helper: create a fresh CAS + Dag in a temp dir.
    fn make_cas_and_dag() -> (ContentStore, Dag, TempDir) {
        let dir = TempDir::new().unwrap();
        let store = ContentStore::open(dir.path()).unwrap();
        let dag = Dag::new(ContentStore::open(dir.path()).unwrap());
        (store, dag, dir)
    }

    #[test]
    fn ingest_single_file() {
        let (store, mut dag, _cas_dir) = make_cas_and_dag();
        let project_dir = TempDir::new().unwrap();
        std::fs::write(project_dir.path().join("hello.txt"), b"hello world").unwrap();

        let result = ingest_tree(
            project_dir.path(),
            &[],
            &[],
            &store,
            &mut dag,
        )
        .unwrap();

        assert_eq!(result.file_count, 1);
        assert!(result.file_hashes.contains_key("hello.txt"));
    }

    #[test]
    fn ingest_nested_directory() {
        let (store, mut dag, _cas_dir) = make_cas_and_dag();
        let project_dir = TempDir::new().unwrap();
        std::fs::create_dir(project_dir.path().join("src")).unwrap();
        std::fs::write(project_dir.path().join("src/lib.rs"), b"pub fn lib() {}").unwrap();
        std::fs::write(project_dir.path().join("src/util.rs"), b"pub fn util() {}").unwrap();
        std::fs::write(project_dir.path().join("Cargo.toml"), b"[package]\nname = \"test\"").unwrap();

        let result = ingest_tree(
            project_dir.path(),
            &[],
            &[],
            &store,
            &mut dag,
        )
        .unwrap();

        assert_eq!(result.file_count, 3);
        assert!(result.file_hashes.contains_key("Cargo.toml"));
        assert!(result.file_hashes.contains_key("src/lib.rs"));
        assert!(result.file_hashes.contains_key("src/util.rs"));
    }

    #[test]
    fn ingest_respects_excludes() {
        let (store, mut dag, _cas_dir) = make_cas_and_dag();
        let project_dir = TempDir::new().unwrap();
        std::fs::create_dir(project_dir.path().join(".git")).unwrap();
        std::fs::write(project_dir.path().join(".git/config"), b"[core]").unwrap();
        std::fs::write(project_dir.path().join("app.py"), b"print('hello')").unwrap();
        std::fs::write(project_dir.path().join("cache.pyc"), b"bytecode").unwrap();

        let result = ingest_tree(
            project_dir.path(),
            &[],
            &[],
            &store,
            &mut dag,
        )
        .unwrap();

        assert_eq!(result.file_count, 1);
        assert!(result.file_hashes.contains_key("app.py"));
        assert!(!result.file_hashes.contains_key(".git/config"));
        assert!(!result.file_hashes.contains_key("cache.pyc"));
    }

    #[test]
    fn ingest_respects_includes() {
        let (store, mut dag, _cas_dir) = make_cas_and_dag();
        let project_dir = TempDir::new().unwrap();
        std::fs::write(project_dir.path().join("app.py"), b"print('hello')").unwrap();
        std::fs::write(project_dir.path().join("readme.md"), b"# readme").unwrap();
        std::fs::write(project_dir.path().join("config.py"), b"DEBUG = True").unwrap();

        let result = ingest_tree(
            project_dir.path(),
            &["*.py".to_string()],
            &[],
            &store,
            &mut dag,
        )
        .unwrap();

        assert_eq!(result.file_count, 2);
        assert!(result.file_hashes.contains_key("app.py"));
        assert!(result.file_hashes.contains_key("config.py"));
        assert!(!result.file_hashes.contains_key("readme.md"));
    }

    #[test]
    fn ingest_changed_file_produces_different_root() {
        let (store, mut dag, _cas_dir) = make_cas_and_dag();
        let project_dir = TempDir::new().unwrap();
        let file_path = project_dir.path().join("data.txt");
        std::fs::write(&file_path, b"version 1").unwrap();

        let result1 = ingest_tree(
            project_dir.path(),
            &[],
            &[],
            &store,
            &mut dag,
        )
        .unwrap();

        std::fs::write(&file_path, b"version 2 - different content").unwrap();

        let result2 = ingest_tree(
            project_dir.path(),
            &[],
            &[],
            &store,
            &mut dag,
        )
        .unwrap();

        assert_ne!(result1.root_hash, result2.root_hash);
    }

    #[test]
    fn ingest_unchanged_file_produces_same_hash() {
        let (store, mut dag, _cas_dir) = make_cas_and_dag();
        let project_dir = TempDir::new().unwrap();
        std::fs::write(project_dir.path().join("stable.txt"), b"stable content").unwrap();

        let result1 = ingest_tree(
            project_dir.path(),
            &[],
            &[],
            &store,
            &mut dag,
        )
        .unwrap();

        let result2 = ingest_tree(
            project_dir.path(),
            &[],
            &[],
            &store,
            &mut dag,
        )
        .unwrap();

        assert_eq!(result1.root_hash, result2.root_hash);
    }
}

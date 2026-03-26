use hd_cas::ContentHash;
use hd_engine::{Dag, Node};

use crate::overlay::Overlay;

#[derive(Debug, thiserror::Error)]
pub enum ProjectedError {
    #[error("path not found: {0}")]
    NotFound(String),
    #[error("not a file: {0}")]
    NotAFile(String),
    #[error("not a directory: {0}")]
    NotADirectory(String),
    #[error("CAS error: {0}")]
    Store(#[from] hd_cas::store::StoreError),
}

/// A projected filesystem that resolves paths against the DAG and serves
/// file content from the CAS. An overlay captures writes.
pub struct ProjectedFs {
    dag: Dag,
    root: ContentHash,
    overlay: Overlay,
}

impl ProjectedFs {
    pub fn new(dag: Dag, root: ContentHash) -> Self {
        ProjectedFs {
            dag,
            root,
            overlay: Overlay::new(),
        }
    }

    /// Read a file's contents. Checks the overlay first, then the DAG/CAS.
    pub fn read_file(&self, path: &str) -> Result<Vec<u8>, ProjectedError> {
        // Check overlay first
        if self.overlay.is_deleted(path) {
            return Err(ProjectedError::NotFound(path.to_string()));
        }
        if let Some(data) = self.overlay.read(path) {
            return Ok(data.to_vec());
        }

        // Fall through to DAG
        let node = self.dag.query(&self.root, path)
            .ok_or_else(|| ProjectedError::NotFound(path.to_string()))?;

        match node {
            Node::File { manifest_hash, .. } => {
                let manifest = self.dag.store().get_manifest(manifest_hash)?;
                let mut data = Vec::with_capacity(manifest.size as usize);
                for chunk_hash in &manifest.chunks {
                    let chunk = self.dag.store().get_chunk(chunk_hash)?;
                    data.extend_from_slice(&chunk);
                }
                Ok(data)
            }
            _ => Err(ProjectedError::NotAFile(path.to_string())),
        }
    }

    /// List entries in a directory.
    pub fn list_dir(&self, path: &str) -> Result<Vec<String>, ProjectedError> {
        if path.is_empty() {
            // Root: list children of the EnvNode
            let root_node = self.dag.get(&self.root)
                .ok_or_else(|| ProjectedError::NotFound("root".to_string()))?;
            match root_node {
                Node::Env { children, .. } => {
                    let mut entries = Vec::new();
                    for child_hash in children {
                        if let Some(child) = self.dag.get(child_hash) {
                            if let Some(name) = node_name(child) {
                                entries.push(name);
                            }
                        }
                    }
                    Ok(entries)
                }
                _ => Err(ProjectedError::NotADirectory(path.to_string())),
            }
        } else {
            let node = self.dag.query(&self.root, path)
                .ok_or_else(|| ProjectedError::NotFound(path.to_string()))?;
            match node {
                Node::Dir { children, .. } => {
                    Ok(children.iter().map(|(name, _)| name.clone()).collect())
                }
                _ => Err(ProjectedError::NotADirectory(path.to_string())),
            }
        }
    }

    /// Check if a path exists (file or directory).
    pub fn exists(&self, path: &str) -> bool {
        if self.overlay.is_deleted(path) {
            return false;
        }
        if self.overlay.read(path).is_some() {
            return true;
        }
        self.dag.query(&self.root, path).is_some()
    }

    /// Get a mutable reference to the overlay.
    pub fn overlay_mut(&mut self) -> &mut Overlay {
        &mut self.overlay
    }

    /// Get a reference to the overlay.
    pub fn overlay(&self) -> &Overlay {
        &self.overlay
    }

    /// Get the current root hash.
    pub fn root(&self) -> &ContentHash {
        &self.root
    }

    /// Update the root hash (after DAG invalidation).
    pub fn set_root(&mut self, root: ContentHash) {
        self.root = root;
    }

    /// Get a reference to the DAG.
    pub fn dag(&self) -> &Dag {
        &self.dag
    }
}

/// Extract the display name from a node (last path component for Dir, filename for File).
fn node_name(node: &Node) -> Option<String> {
    match node {
        Node::Dir { path, .. } => {
            Some(path.rsplit('/').next().unwrap_or(path).to_string())
        }
        Node::File { path, .. } => {
            Some(path.rsplit('/').next().unwrap_or(path).to_string())
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hd_cas::ContentStore;
    use tempfile::TempDir;

    fn setup() -> (ProjectedFs, TempDir) {
        let dir = TempDir::new().unwrap();
        let store = ContentStore::open(dir.path()).unwrap();

        // Store some file content in the CAS
        let file_data = b"fn main() { println!(\"hello\"); }";
        let manifest_hash = store.put_file_from_bytes(file_data, 0o644).unwrap();

        let lib_data = b"pub fn greet() {}";
        let lib_manifest_hash = store.put_file_from_bytes(lib_data, 0o644).unwrap();

        // Build DAG: env -> dir("src") -> {main.rs, lib.rs}
        let mut dag = Dag::new(store);
        let main_node = Node::file("src/main.rs", manifest_hash);
        let main_hash = dag.insert(main_node).unwrap();
        let lib_node = Node::file("src/lib.rs", lib_manifest_hash);
        let lib_hash = dag.insert(lib_node).unwrap();
        let src_dir = Node::dir("src", vec![
            ("main.rs".into(), main_hash),
            ("lib.rs".into(), lib_hash),
        ]);
        let src_hash = dag.insert(src_dir).unwrap();
        let env = Node::env("testapp", vec![src_hash]);
        let env_hash = dag.insert(env).unwrap();

        let fs = ProjectedFs::new(dag, env_hash);
        (fs, dir)
    }

    #[test]
    fn read_file_from_dag() {
        let (fs, _dir) = setup();
        let data = fs.read_file("src/main.rs").unwrap();
        assert_eq!(data, b"fn main() { println!(\"hello\"); }");
    }

    #[test]
    fn read_nonexistent_file() {
        let (fs, _dir) = setup();
        assert!(fs.read_file("missing.rs").is_err());
    }

    #[test]
    fn list_directory() {
        let (fs, _dir) = setup();
        let entries = fs.list_dir("src").unwrap();
        assert_eq!(entries.len(), 2);
        assert!(entries.contains(&"main.rs".to_string()));
        assert!(entries.contains(&"lib.rs".to_string()));
    }

    #[test]
    fn list_root() {
        let (fs, _dir) = setup();
        let entries = fs.list_dir("").unwrap();
        assert!(entries.contains(&"src".to_string()));
    }

    #[test]
    fn overlay_overrides_dag() {
        let (mut fs, _dir) = setup();
        fs.overlay_mut().write("src/main.rs", b"// modified".to_vec());
        let data = fs.read_file("src/main.rs").unwrap();
        assert_eq!(data, b"// modified");
    }

    #[test]
    fn overlay_adds_new_file() {
        let (mut fs, _dir) = setup();
        fs.overlay_mut().write("src/new.rs", b"// new file".to_vec());
        let data = fs.read_file("src/new.rs").unwrap();
        assert_eq!(data, b"// new file");
    }

    #[test]
    fn overlay_deletes_dag_file() {
        let (mut fs, _dir) = setup();
        fs.overlay_mut().delete("src/main.rs");
        assert!(fs.read_file("src/main.rs").is_err());
    }

    #[test]
    fn file_exists() {
        let (fs, _dir) = setup();
        assert!(fs.exists("src/main.rs"));
        assert!(fs.exists("src"));
        assert!(!fs.exists("missing"));
    }
}

use std::collections::HashMap;

use hd_cas::{ContentHash, ContentStore};

use crate::node::Node;

/// In-memory DAG with nodes indexed by content hash.
/// Nodes are also serialized into the CAS for persistence.
pub struct Dag {
    nodes: HashMap<ContentHash, Node>,
    store: ContentStore,
}

#[derive(Debug, thiserror::Error)]
pub enum DagError {
    #[error("store error: {0}")]
    Store(#[from] hd_cas::store::StoreError),
    #[error("serialization error: {0}")]
    Serialization(String),
}

impl Dag {
    pub fn new(store: ContentStore) -> Self {
        Dag {
            nodes: HashMap::new(),
            store,
        }
    }

    /// Insert a node into the DAG. Returns its content hash.
    /// Deduplicates: if a node with the same hash exists, returns the existing hash.
    pub fn insert(&mut self, node: Node) -> Result<ContentHash, DagError> {
        let hash = node.content_hash();
        if !self.nodes.contains_key(&hash) {
            // Persist to CAS
            let bytes = bincode::serde::encode_to_vec(&node, bincode::config::standard())
                .map_err(|e| DagError::Serialization(e.to_string()))?;
            self.store.put_chunk(&bytes)?;
            self.nodes.insert(hash, node);
        }
        Ok(hash)
    }

    /// Get a node by its content hash.
    pub fn get(&self, hash: &ContentHash) -> Option<&Node> {
        self.nodes.get(hash)
    }

    /// Get the direct children of a node (by content hash).
    pub fn children(&self, hash: &ContentHash) -> Vec<ContentHash> {
        match self.nodes.get(hash) {
            Some(Node::Dir { children, .. }) => children.iter().map(|(_, h)| *h).collect(),
            Some(Node::Env { children, .. }) => children.clone(),
            Some(Node::BuildStep { input_hashes, .. }) => input_hashes.clone(),
            _ => Vec::new(),
        }
    }

    /// Query the DAG for a node by path, starting from an env or dir root.
    /// Path format: "dir/subdir/file.rs"
    pub fn query(&self, root: &ContentHash, path: &str) -> Option<&Node> {
        let parts: Vec<&str> = path.split('/').collect();
        self.query_recursive(root, &parts, 0)
    }

    fn query_recursive(&self, current: &ContentHash, parts: &[&str], depth: usize) -> Option<&Node> {
        if depth >= parts.len() {
            return self.nodes.get(current);
        }

        let node = self.nodes.get(current)?;
        match node {
            Node::Env { children, .. } => {
                for child_hash in children {
                    if let Some(result) = self.query_recursive(child_hash, parts, depth) {
                        return Some(result);
                    }
                }
                None
            }
            Node::Dir { path: dir_path, children, .. } => {
                let dir_name = dir_path.rsplit('/').next().unwrap_or(dir_path);
                if dir_name == parts[depth] {
                    if depth == parts.len() - 1 {
                        return Some(node);
                    }
                    for (child_name, child_hash) in children {
                        if child_name == parts[depth + 1] {
                            if depth + 1 == parts.len() - 1 {
                                return self.nodes.get(child_hash);
                            }
                            return self.query_recursive(child_hash, parts, depth + 2);
                        }
                    }
                }
                // Try matching children directly
                for (child_name, child_hash) in children {
                    if child_name == parts[depth] {
                        if depth == parts.len() - 1 {
                            return self.nodes.get(child_hash);
                        }
                        return self.query_recursive(child_hash, parts, depth + 1);
                    }
                }
                None
            }
            _ => None,
        }
    }

    /// Return a reference to the underlying CAS.
    pub fn store(&self) -> &ContentStore {
        &self.store
    }

    /// Get all node hashes in the DAG.
    pub fn node_hashes(&self) -> Vec<ContentHash> {
        self.nodes.keys().copied().collect()
    }

    /// Get the parent hashes of a node (all nodes that reference this hash as a child).
    pub fn parents(&self, target: &ContentHash) -> Vec<ContentHash> {
        let mut parents = Vec::new();
        for (hash, node) in &self.nodes {
            let is_parent = match node {
                Node::Dir { children, .. } => children.iter().any(|(_, h)| h == target),
                Node::Env { children, .. } => children.contains(target),
                Node::BuildStep { input_hashes, .. } => input_hashes.contains(target),
                _ => false,
            };
            if is_parent {
                parents.push(*hash);
            }
        }
        parents
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hd_cas::ContentHash;
    use tempfile::TempDir;

    fn test_dag() -> (Dag, TempDir) {
        let dir = TempDir::new().unwrap();
        let store = hd_cas::ContentStore::open(dir.path()).unwrap();
        let dag = Dag::new(store);
        (dag, dir)
    }

    #[test]
    fn insert_and_get_node() {
        let (mut dag, _dir) = test_dag();
        let node = Node::file("main.rs", ContentHash::from_bytes(b"content"));
        let hash = dag.insert(node.clone()).unwrap();
        let retrieved = dag.get(&hash).unwrap();
        assert_eq!(retrieved.content_hash(), node.content_hash());
    }

    #[test]
    fn get_nonexistent_returns_none() {
        let (dag, _dir) = test_dag();
        let fake = ContentHash::from_bytes(b"nope");
        assert!(dag.get(&fake).is_none());
    }

    #[test]
    fn insert_deduplicates() {
        let (mut dag, _dir) = test_dag();
        let node = Node::file("lib.rs", ContentHash::from_bytes(b"same"));
        let h1 = dag.insert(node.clone()).unwrap();
        let h2 = dag.insert(node).unwrap();
        assert_eq!(h1, h2);
    }

    #[test]
    fn build_tree_and_query_path() {
        let (mut dag, _dir) = test_dag();

        let file_node = Node::file("src/main.rs", ContentHash::from_bytes(b"main"));
        let file_hash = dag.insert(file_node.clone()).unwrap();

        let dir_node = Node::dir("src", vec![("main.rs".into(), file_hash)]);
        let dir_hash = dag.insert(dir_node).unwrap();

        let env_node = Node::env("myapp", vec![dir_hash]);
        let env_hash = dag.insert(env_node).unwrap();

        let found = dag.query(&env_hash, "src/main.rs");
        assert!(found.is_some());
        assert_eq!(found.unwrap().content_hash(), file_node.content_hash());
    }

    #[test]
    fn children_of_env_node() {
        let (mut dag, _dir) = test_dag();
        let f1 = dag.insert(Node::file("a.rs", ContentHash::from_bytes(b"a"))).unwrap();
        let f2 = dag.insert(Node::file("b.rs", ContentHash::from_bytes(b"b"))).unwrap();
        let env = dag.insert(Node::env("test", vec![f1, f2])).unwrap();

        let children = dag.children(&env);
        assert_eq!(children.len(), 2);
    }
}

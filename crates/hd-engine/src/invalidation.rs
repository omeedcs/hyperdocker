use std::collections::HashSet;

use hd_cas::ContentHash;

use crate::dag::{Dag, DagError};
use crate::node::Node;

/// Describes a file-level change to apply to the DAG.
pub struct FileChange {
    pub old_hash: ContentHash,
    pub new_node: Node,
}

/// Result of an invalidation pass.
pub struct InvalidationResult {
    /// The new root hash after applying all changes.
    pub new_root: ContentHash,
    /// Set of old node hashes that were invalidated (stale).
    pub invalidated: HashSet<ContentHash>,
}

/// Apply a set of file changes to the DAG, propagating invalidation bottom-up.
///
/// For each changed file:
/// 1. Insert the new node
/// 2. Find all ancestors of the old node
/// 3. Rebuild each ancestor with the updated child hash
///
/// Returns the new root hash and the set of invalidated node hashes.
pub fn invalidate(
    dag: &mut Dag,
    root: &ContentHash,
    changes: &[FileChange],
) -> Result<InvalidationResult, DagError> {
    if changes.is_empty() {
        return Ok(InvalidationResult {
            new_root: *root,
            invalidated: HashSet::new(),
        });
    }

    let mut invalidated = HashSet::new();
    let mut remap: std::collections::HashMap<ContentHash, ContentHash> = std::collections::HashMap::new();

    // Phase 1: Insert new file nodes and seed the remap
    for change in changes {
        let new_hash = dag.insert(change.new_node.clone())?;
        invalidated.insert(change.old_hash);
        remap.insert(change.old_hash, new_hash);
    }

    // Phase 2: Walk up from changed nodes, rebuilding ancestors
    let mut current_level: Vec<ContentHash> = changes.iter().map(|c| c.old_hash).collect();

    loop {
        let mut next_level = Vec::new();

        for old_hash in &current_level {
            let parents = dag.parents(old_hash);
            for parent_hash in parents {
                if invalidated.contains(&parent_hash) {
                    continue;
                }
                let parent_node = match dag.get(&parent_hash) {
                    Some(n) => n.clone(),
                    None => continue,
                };

                let new_parent = rebuild_node(&parent_node, &remap);
                let new_parent_hash = dag.insert(new_parent)?;

                invalidated.insert(parent_hash);
                remap.insert(parent_hash, new_parent_hash);
                next_level.push(parent_hash);
            }
        }

        if next_level.is_empty() {
            break;
        }
        current_level = next_level;
    }

    let new_root = remap.get(root).copied().unwrap_or(*root);

    Ok(InvalidationResult {
        new_root,
        invalidated,
    })
}

/// Rebuild a node by replacing any child references found in the remap table.
fn rebuild_node(
    node: &Node,
    remap: &std::collections::HashMap<ContentHash, ContentHash>,
) -> Node {
    match node {
        Node::Dir { path, children } => {
            let new_children: Vec<(String, ContentHash)> = children
                .iter()
                .map(|(name, hash)| {
                    let new_hash = remap.get(hash).copied().unwrap_or(*hash);
                    (name.clone(), new_hash)
                })
                .collect();
            Node::dir(path, new_children)
        }
        Node::Env { name, children } => {
            let new_children: Vec<ContentHash> = children
                .iter()
                .map(|h| remap.get(h).copied().unwrap_or(*h))
                .collect();
            Node::env(name, new_children)
        }
        Node::BuildStep { command, input_hashes, env_vars } => {
            let new_inputs: Vec<ContentHash> = input_hashes
                .iter()
                .map(|h| remap.get(h).copied().unwrap_or(*h))
                .collect();
            Node::build_step(command, new_inputs, env_vars.clone())
        }
        other => other.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node::Node;
    use crate::dag::Dag;
    use hd_cas::ContentHash;
    use tempfile::TempDir;

    fn test_dag() -> (Dag, TempDir) {
        let dir = TempDir::new().unwrap();
        let store = hd_cas::ContentStore::open(dir.path()).unwrap();
        (Dag::new(store), dir)
    }

    #[test]
    fn invalidate_file_propagates_to_dir_and_env() {
        let (mut dag, _dir) = test_dag();

        let file = Node::file("src/main.rs", ContentHash::from_bytes(b"v1"));
        let file_hash = dag.insert(file).unwrap();
        let dir = Node::dir("src", vec![("main.rs".into(), file_hash)]);
        let dir_hash = dag.insert(dir).unwrap();
        let env = Node::env("app", vec![dir_hash]);
        let env_hash = dag.insert(env).unwrap();

        let new_file = Node::file("src/main.rs", ContentHash::from_bytes(b"v2"));
        let change = FileChange {
            old_hash: file_hash,
            new_node: new_file,
        };

        let result = invalidate(&mut dag, &env_hash, &[change]).unwrap();

        assert_ne!(result.new_root, env_hash);
        assert!(result.invalidated.contains(&file_hash));
        assert!(result.invalidated.contains(&dir_hash));
        assert!(result.invalidated.contains(&env_hash));
    }

    #[test]
    fn invalidate_preserves_unchanged_siblings() {
        let (mut dag, _dir) = test_dag();

        let file_a = Node::file("src/a.rs", ContentHash::from_bytes(b"a"));
        let file_b = Node::file("src/b.rs", ContentHash::from_bytes(b"b"));
        let hash_a = dag.insert(file_a).unwrap();
        let hash_b = dag.insert(file_b).unwrap();
        let dir = Node::dir("src", vec![
            ("a.rs".into(), hash_a),
            ("b.rs".into(), hash_b),
        ]);
        let dir_hash = dag.insert(dir).unwrap();
        let env = Node::env("app", vec![dir_hash]);
        let env_hash = dag.insert(env).unwrap();

        let new_a = Node::file("src/a.rs", ContentHash::from_bytes(b"a_v2"));
        let change = FileChange {
            old_hash: hash_a,
            new_node: new_a,
        };

        let result = invalidate(&mut dag, &env_hash, &[change]).unwrap();

        assert!(!result.invalidated.contains(&hash_b));
        assert!(result.invalidated.contains(&hash_a));
        assert!(result.invalidated.contains(&dir_hash));
        assert!(result.invalidated.contains(&env_hash));
    }

    #[test]
    fn invalidate_no_changes_returns_same_root() {
        let (mut dag, _dir) = test_dag();
        let file = Node::file("main.rs", ContentHash::from_bytes(b"v1"));
        let fh = dag.insert(file).unwrap();
        let env = Node::env("app", vec![fh]);
        let eh = dag.insert(env).unwrap();

        let result = invalidate(&mut dag, &eh, &[]).unwrap();
        assert_eq!(result.new_root, eh);
        assert!(result.invalidated.is_empty());
    }
}

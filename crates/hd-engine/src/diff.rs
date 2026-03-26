use std::collections::HashSet;

use hd_cas::ContentHash;

use crate::dag::Dag;

/// The result of diffing two DAG states.
#[derive(Debug)]
pub struct DagDiff {
    /// Node hashes present in `to` but not in `from`.
    pub added: HashSet<ContentHash>,
    /// Node hashes present in `from` but not in `to`.
    pub removed: HashSet<ContentHash>,
    /// Node hashes present in both but rebuilt (roots that changed).
    pub changed: HashSet<ContentHash>,
}

/// Diff two DAG states by comparing the reachable node sets.
pub fn dag_diff(dag: &Dag, from: &ContentHash, to: &ContentHash) -> DagDiff {
    if from == to {
        return DagDiff {
            added: HashSet::new(),
            removed: HashSet::new(),
            changed: HashSet::new(),
        };
    }

    let from_set = collect_reachable(dag, from);
    let to_set = collect_reachable(dag, to);

    // Nodes that changed between from and to are the roots themselves (and any
    // intermediate nodes that were rebuilt). We track the from/to roots in
    // `changed` so they don't pollute `added`/`removed` with spurious entries.
    let mut changed = HashSet::new();
    changed.insert(*from);
    changed.insert(*to);

    // Intermediate rebuilt nodes: present in exactly one side but are ancestors
    // of the roots (i.e. nodes that appear in both sides' reachable sets minus
    // the leaf-only nodes). We compute this by finding nodes that differ between
    // sides but are not pure leaf additions/removals.
    //
    // Strategy: for each node reachable from `to` that is not in `from_set`,
    // check if there is a "corresponding" node in `from_set` (same structural
    // role). We approximate this by computing the symmetric difference and
    // separating it into:
    //   - added: only in to_set, excluding the to-root and rebuilt intermediates
    //   - removed: only in from_set, excluding the from-root and rebuilt intermediates
    //
    // We detect rebuilt intermediates by checking if both sides differ at the
    // root level — any node that is an ancestor of both from and to roots (when
    // walking the DAG from respective roots) that differs is "changed".

    // Collect all rebuilt ancestors: walk both sides concurrently and find
    // paired nodes that differ.
    collect_changed_ancestors(dag, from, to, &mut changed);

    let added: HashSet<ContentHash> = to_set
        .difference(&from_set)
        .copied()
        .filter(|h| !changed.contains(h))
        .collect();
    let removed: HashSet<ContentHash> = from_set
        .difference(&to_set)
        .copied()
        .filter(|h| !changed.contains(h))
        .collect();

    // Only populate `changed` when there are actual structural differences.
    let changed = if !added.is_empty() || !removed.is_empty() {
        changed
    } else {
        HashSet::new()
    };

    DagDiff { added, removed, changed }
}

/// Walk from and to in parallel, collecting nodes that were rebuilt (same
/// position in the tree but different hashes). Only structural (non-leaf)
/// nodes are added to `changed`; leaf differences surface as add/remove.
fn collect_changed_ancestors(
    dag: &Dag,
    from: &ContentHash,
    to: &ContentHash,
    changed: &mut HashSet<ContentHash>,
) {
    if from == to {
        return;
    }

    let from_children = dag.children(from);
    let to_children = dag.children(to);

    // If either side has no children, these are leaf nodes — don't mark as
    // changed so they surface in added/removed instead.
    if from_children.is_empty() || to_children.is_empty() {
        return;
    }

    // This is a structural (non-leaf) node that was rebuilt.
    changed.insert(*from);
    changed.insert(*to);

    // Pair up children by index (structural position) and recurse.
    for (fc, tc) in from_children.iter().zip(to_children.iter()) {
        if fc != tc {
            collect_changed_ancestors(dag, fc, tc, changed);
        }
    }
}

/// Collect all node hashes reachable from a root.
fn collect_reachable(dag: &Dag, root: &ContentHash) -> HashSet<ContentHash> {
    let mut visited = HashSet::new();
    let mut stack = vec![*root];

    while let Some(hash) = stack.pop() {
        if !visited.insert(hash) {
            continue;
        }
        for child in dag.children(&hash) {
            if !visited.contains(&child) {
                stack.push(child);
            }
        }
    }

    visited
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node::Node;
    use tempfile::TempDir;

    fn test_dag() -> (Dag, TempDir) {
        let dir = TempDir::new().unwrap();
        let store = hd_cas::ContentStore::open(dir.path()).unwrap();
        (Dag::new(store), dir)
    }

    #[test]
    fn diff_identical_roots() {
        let (mut dag, _dir) = test_dag();
        let file = Node::file("a.rs", ContentHash::from_bytes(b"a"));
        let fh = dag.insert(file).unwrap();
        let env = Node::env("app", vec![fh]);
        let eh = dag.insert(env).unwrap();

        let diff = dag_diff(&dag, &eh, &eh);
        assert!(diff.added.is_empty());
        assert!(diff.removed.is_empty());
        assert!(diff.changed.is_empty());
    }

    #[test]
    fn diff_added_node() {
        let (mut dag, _dir) = test_dag();
        let f1 = Node::file("a.rs", ContentHash::from_bytes(b"a"));
        let fh1 = dag.insert(f1).unwrap();
        let env1 = Node::env("app", vec![fh1]);
        let eh1 = dag.insert(env1).unwrap();

        let f2 = Node::file("b.rs", ContentHash::from_bytes(b"b"));
        let fh2 = dag.insert(f2).unwrap();
        let env2 = Node::env("app", vec![fh1, fh2]);
        let eh2 = dag.insert(env2).unwrap();

        let diff = dag_diff(&dag, &eh1, &eh2);
        assert_eq!(diff.added.len(), 1);
        assert!(diff.added.contains(&fh2));
        assert!(diff.removed.is_empty());
    }

    #[test]
    fn diff_removed_node() {
        let (mut dag, _dir) = test_dag();
        let f1 = Node::file("a.rs", ContentHash::from_bytes(b"a"));
        let f2 = Node::file("b.rs", ContentHash::from_bytes(b"b"));
        let fh1 = dag.insert(f1).unwrap();
        let fh2 = dag.insert(f2).unwrap();
        let env1 = Node::env("app", vec![fh1, fh2]);
        let eh1 = dag.insert(env1).unwrap();

        let env2 = Node::env("app", vec![fh1]);
        let eh2 = dag.insert(env2).unwrap();

        let diff = dag_diff(&dag, &eh1, &eh2);
        assert!(diff.added.is_empty());
        assert_eq!(diff.removed.len(), 1);
        assert!(diff.removed.contains(&fh2));
    }

    #[test]
    fn diff_changed_is_symmetric_add_remove() {
        let (mut dag, _dir) = test_dag();
        let f1 = Node::file("a.rs", ContentHash::from_bytes(b"v1"));
        let fh1 = dag.insert(f1).unwrap();
        let env1 = Node::env("app", vec![fh1]);
        let eh1 = dag.insert(env1).unwrap();

        let f2 = Node::file("a.rs", ContentHash::from_bytes(b"v2"));
        let fh2 = dag.insert(f2).unwrap();
        let env2 = Node::env("app", vec![fh2]);
        let eh2 = dag.insert(env2).unwrap();

        let diff = dag_diff(&dag, &eh1, &eh2);
        assert!(diff.added.contains(&fh2));
        assert!(diff.removed.contains(&fh1));
    }
}

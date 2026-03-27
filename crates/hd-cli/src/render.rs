use hd_cas::ContentHash;
use hd_engine::{Dag, Node, DagDiff};

// ANSI color codes
const RESET: &str = "\x1b[0m";
const DIM: &str = "\x1b[2m";
const GREEN: &str = "\x1b[32m";
const YELLOW: &str = "\x1b[33m";
const RED: &str = "\x1b[31m";

/// Format a byte count as a human-readable string.
#[allow(dead_code)]
pub fn format_bytes(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{}B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1}KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1}MB", bytes as f64 / (1024.0 * 1024.0))
    }
}

fn short_hash(hash: &ContentHash) -> String {
    hash.to_hex()[..12].to_string()
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}...", &s[..max])
    }
}

fn node_label(node: &Node, hash: &ContentHash) -> String {
    let h = short_hash(hash);
    match node {
        Node::Env { name, .. } => format!("Env({})  {}", name, h),
        Node::Dir { path, .. } => format!("Dir({})  {}", path, h),
        Node::File { path, manifest_hash } => {
            format!("File({})  {}  manifest:{}", path, h, short_hash(manifest_hash))
        }
        Node::Package { provider, name, version, .. } => {
            format!("Pkg({}/{} {})  {}", provider, name, version, h)
        }
        Node::BuildStep { command, .. } => {
            format!("Build({})  {}", truncate(command, 40), h)
        }
    }
}

fn color_for(hash: &ContentHash, diff: &DagDiff) -> (&'static str, &'static str) {
    if diff.added.contains(hash) {
        (GREEN, "[NEW]")
    } else if diff.changed.contains(hash) {
        (YELLOW, "[CHANGED]")
    } else if diff.removed.contains(hash) {
        (RED, "[REMOVED]")
    } else {
        (DIM, "[ok]")
    }
}

fn walk_tree(dag: &Dag, hash: &ContentHash, depth: usize, diff: Option<&DagDiff>) {
    let node = match dag.get(hash) {
        Some(n) => n,
        None => return,
    };

    let label_str = node_label(node, hash);
    let (color, status) = if let Some(d) = diff {
        color_for(hash, d)
    } else {
        (DIM, "[ok]")
    };

    let indent = "  ".repeat(depth);
    println!("{}{}{}  {}{}{}",
        indent,
        color,
        label_str,
        DIM,
        status,
        RESET,
    );

    // Recurse into children
    match node {
        Node::Env { children, .. } => {
            for child in children {
                walk_tree(dag, child, depth + 1, diff);
            }
        }
        Node::Dir { children, .. } => {
            for (_name, child_hash) in children {
                walk_tree(dag, child_hash, depth + 1, diff);
            }
        }
        Node::BuildStep { input_hashes, .. } => {
            for child in input_hashes {
                walk_tree(dag, child, depth + 1, diff);
            }
        }
        Node::File { .. } | Node::Package { .. } => {
            // leaf nodes — no children to recurse
        }
    }
}

/// Print the DAG tree rooted at `root`, all nodes dimmed (no diff context).
pub fn render_tree(dag: &Dag, root: &ContentHash) {
    walk_tree(dag, root, 0, None);
}

/// Print the DAG tree with diff highlighting relative to old_root vs new_root.
/// Green = added, Yellow = changed, Red = removed, Dim = unchanged.
/// Also prints a summary line at the bottom.
#[allow(dead_code)]
pub fn render_diff(dag: &Dag, old_root: &ContentHash, new_root: &ContentHash) {
    let diff = hd_engine::dag_diff(dag, old_root, new_root);

    // Render the new tree (nodes reachable from new_root)
    walk_tree(dag, new_root, 0, Some(&diff));

    // Show any removed nodes that appear only in the old tree
    let removed: Vec<ContentHash> = diff.removed.iter().copied().collect();
    if !removed.is_empty() {
        println!("{}{}--- removed nodes ---{}", DIM, RED, RESET);
        for hash in &removed {
            if let Some(node) = dag.get(&hash) {
                let label_str = node_label(node, &hash);
                println!("{}{}  [REMOVED]{}", RED, label_str, RESET);
            }
        }
    }

    // Summary line
    println!();
    println!(
        "{}+ {} added{}  {}~ {} changed{}  {}- {} removed{}",
        GREEN, diff.added.len(), RESET,
        YELLOW, diff.changed.len(), RESET,
        RED, diff.removed.len(), RESET,
    );
}

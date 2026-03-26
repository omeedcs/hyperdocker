use hd_cas::{ContentHash, ContentStore};
use hd_engine::{Node, Dag, invalidate, dag_diff, FileChange};
use tempfile::TempDir;

#[test]
fn full_invalidation_cycle() {
    let dir = TempDir::new().unwrap();
    let store = ContentStore::open(dir.path()).unwrap();
    let mut dag = Dag::new(store);

    let main_v1 = Node::file("src/main.rs", ContentHash::from_bytes(b"main_v1"));
    let lib_v1 = Node::file("src/lib.rs", ContentHash::from_bytes(b"lib_v1"));
    let main_hash = dag.insert(main_v1).unwrap();
    let lib_hash = dag.insert(lib_v1).unwrap();

    let src_dir = Node::dir("src", vec![
        ("main.rs".into(), main_hash),
        ("lib.rs".into(), lib_hash),
    ]);
    let src_hash = dag.insert(src_dir).unwrap();
    let env = Node::env("app", vec![src_hash]);
    let env_hash = dag.insert(env).unwrap();

    let main_v2 = Node::file("src/main.rs", ContentHash::from_bytes(b"main_v2"));
    let change = FileChange {
        old_hash: main_hash,
        new_node: main_v2,
    };
    let result = invalidate(&mut dag, &env_hash, &[change]).unwrap();

    assert_ne!(result.new_root, env_hash);

    let diff = dag_diff(&dag, &env_hash, &result.new_root);
    assert!(!diff.added.is_empty());
    assert!(!diff.removed.is_empty());

    assert!(!diff.added.contains(&lib_hash));
    assert!(!diff.removed.contains(&lib_hash));
}

#[test]
fn multiple_changes_in_one_pass() {
    let dir = TempDir::new().unwrap();
    let store = ContentStore::open(dir.path()).unwrap();
    let mut dag = Dag::new(store);

    let f1 = Node::file("a.rs", ContentHash::from_bytes(b"a1"));
    let f2 = Node::file("b.rs", ContentHash::from_bytes(b"b1"));
    let fh1 = dag.insert(f1).unwrap();
    let fh2 = dag.insert(f2).unwrap();
    let dir_node = Node::dir("src", vec![
        ("a.rs".into(), fh1),
        ("b.rs".into(), fh2),
    ]);
    let dh = dag.insert(dir_node).unwrap();
    let env = Node::env("app", vec![dh]);
    let eh = dag.insert(env).unwrap();

    let changes = vec![
        FileChange {
            old_hash: fh1,
            new_node: Node::file("a.rs", ContentHash::from_bytes(b"a2")),
        },
        FileChange {
            old_hash: fh2,
            new_node: Node::file("b.rs", ContentHash::from_bytes(b"b2")),
        },
    ];

    let result = invalidate(&mut dag, &eh, &changes).unwrap();
    assert_ne!(result.new_root, eh);
    assert_eq!(result.invalidated.len(), 4); // 2 files + dir + env
}

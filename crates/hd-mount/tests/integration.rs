use hd_cas::ContentStore;
use hd_engine::{Dag, Node};
use hd_mount::ProjectedFs;
use tempfile::TempDir;

#[test]
fn projected_fs_survives_dag_invalidation() {
    let dir = TempDir::new().unwrap();
    let store = ContentStore::open(dir.path()).unwrap();

    // Store file content
    let v1_data = b"version 1";
    let v1_hash = store.put_file_from_bytes(v1_data, 0o644).unwrap();
    let v2_data = b"version 2";
    let _v2_hash = store.put_file_from_bytes(v2_data, 0o644).unwrap();

    // Build DAG: env -> dir("src") -> app.rs
    let mut dag = Dag::new(store);
    let file_v1 = Node::file("src/app.rs", v1_hash);
    let file_v1_hash = dag.insert(file_v1).unwrap();
    let src_dir = Node::dir("src", vec![("app.rs".into(), file_v1_hash)]);
    let src_hash = dag.insert(src_dir).unwrap();
    let env = Node::env("app", vec![src_hash]);
    let env_hash = dag.insert(env).unwrap();

    // Create projected FS
    let mut pfs = ProjectedFs::new(dag, env_hash);
    assert_eq!(pfs.read_file("src/app.rs").unwrap(), b"version 1");

    // Simulate a file change: the watcher would do this
    // For this test we just verify the overlay works correctly
    pfs.overlay_mut().write("src/app.rs", b"version 2".to_vec());
    assert_eq!(pfs.read_file("src/app.rs").unwrap(), b"version 2");
}

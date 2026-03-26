use hd_watch::{PathFilter, FileWatcher, PathMap};
use hd_cas::ContentHash;
use tempfile::TempDir;
use std::fs;
use std::time::Duration;

#[test]
fn watch_and_map_changes() {
    let dir = TempDir::new().unwrap();
    let file_path = dir.path().join("app.rs");
    fs::write(&file_path, b"v1").unwrap();

    // Set up path map
    let mut path_map = PathMap::new();
    let initial_hash = ContentHash::from_bytes(b"v1");
    path_map.insert("app.rs", initial_hash);

    // Set up watcher with fast polling for tests
    let filter = PathFilter::new(vec![], vec![]);
    let mut watcher = FileWatcher::with_poll_interval(
        dir.path(),
        filter,
        Duration::from_millis(100),
    ).unwrap();

    // Let the watcher do its initial scan
    std::thread::sleep(Duration::from_millis(500));

    // Modify file
    fs::write(&file_path, b"v2").unwrap();

    // Poll with retries
    let mut changes = vec![];
    for _ in 0..10 {
        std::thread::sleep(Duration::from_millis(300));
        changes = watcher.poll_changes();
        if !changes.is_empty() {
            break;
        }
    }
    assert!(!changes.is_empty());

    // Update path map with new hash
    for change in &changes {
        let rel_path = change.path.to_string_lossy();
        let new_hash = ContentHash::from_bytes(b"v2");
        path_map.insert(&rel_path, new_hash);
    }

    let updated = path_map.get_hash("app.rs").unwrap();
    assert_ne!(updated, &initial_hash);
}

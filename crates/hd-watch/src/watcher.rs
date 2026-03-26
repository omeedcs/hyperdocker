use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

use notify::{Config, PollWatcher, RecursiveMode, Watcher};

use crate::debounce::{ChangeKind, Debouncer, RawChange};
use crate::filter::PathFilter;

#[derive(Debug, thiserror::Error)]
pub enum WatchError {
    #[error("notify error: {0}")]
    Notify(#[from] notify::Error),
    #[error("watch root does not exist: {0}")]
    RootNotFound(String),
}

/// Watches a directory tree for filesystem changes, filters them,
/// debounces, and provides batched change events.
pub struct FileWatcher {
    _watcher: PollWatcher,
    receiver: mpsc::Receiver<notify::Result<notify::Event>>,
    root: PathBuf,
    filter: PathFilter,
    debouncer: Debouncer,
}

impl FileWatcher {
    pub fn new(root: &Path, filter: PathFilter) -> Result<Self, WatchError> {
        Self::with_poll_interval(root, filter, Duration::from_millis(200))
    }

    pub fn with_poll_interval(root: &Path, filter: PathFilter, interval: Duration) -> Result<Self, WatchError> {
        if !root.exists() {
            return Err(WatchError::RootNotFound(root.display().to_string()));
        }

        let (tx, rx) = mpsc::channel();
        let config = Config::default()
            .with_poll_interval(interval)
            .with_compare_contents(true);
        let mut watcher = PollWatcher::new(tx, config)?;
        watcher.watch(root, RecursiveMode::Recursive)?;

        Ok(FileWatcher {
            _watcher: watcher,
            receiver: rx,
            root: root.to_path_buf(),
            filter,
            debouncer: Debouncer::new(),
        })
    }

    /// Poll for pending filesystem changes. Non-blocking: drains the
    /// receiver and returns all coalesced changes.
    pub fn poll_changes(&mut self) -> Vec<RawChange> {
        // Drain all pending events from the notify watcher
        while let Ok(event_result) = self.receiver.try_recv() {
            if let Ok(event) = event_result {
                let kind = match event.kind {
                    notify::EventKind::Create(_) => ChangeKind::Created,
                    notify::EventKind::Modify(_) => ChangeKind::Modified,
                    notify::EventKind::Remove(_) => ChangeKind::Deleted,
                    _ => continue,
                };

                for path in event.paths {
                    // Convert absolute path to relative
                    if let Ok(relative) = path.strip_prefix(&self.root) {
                        let rel_str = relative.to_string_lossy().to_string();
                        if self.filter.is_included(&rel_str) {
                            self.debouncer.push(RawChange {
                                path: relative.to_path_buf(),
                                kind: kind.clone(),
                            });
                        }
                    }
                }
            }
        }

        self.debouncer.drain()
    }

    /// Get a reference to the watch root.
    pub fn root(&self) -> &Path {
        &self.root
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use std::fs;

    fn test_watcher(root: &Path, filter: PathFilter) -> FileWatcher {
        FileWatcher::with_poll_interval(root, filter, Duration::from_millis(100)).unwrap()
    }

    /// Helper: poll with retries to handle poll interval latency.
    fn poll_with_retry(watcher: &mut FileWatcher, retries: u32) -> Vec<RawChange> {
        for _ in 0..retries {
            std::thread::sleep(Duration::from_millis(300));
            let changes = watcher.poll_changes();
            if !changes.is_empty() {
                return changes;
            }
        }
        vec![]
    }

    #[test]
    fn detect_file_change() {
        let dir = TempDir::new().unwrap();
        let file_path = dir.path().join("test.rs");
        fs::write(&file_path, b"original").unwrap();

        let filter = PathFilter::new(vec![], vec![]);
        let mut watcher = test_watcher(dir.path(), filter);

        // Small delay to let the watcher do its initial scan
        std::thread::sleep(Duration::from_millis(500));

        // Modify file
        fs::write(&file_path, b"modified").unwrap();

        let changes = poll_with_retry(&mut watcher, 10);
        assert!(!changes.is_empty(), "should detect the file change");
    }

    #[test]
    fn filtered_files_ignored() {
        let dir = TempDir::new().unwrap();
        let log_path = dir.path().join("debug.log");
        let rs_path = dir.path().join("main.rs");
        fs::write(&log_path, b"log").unwrap();
        fs::write(&rs_path, b"code").unwrap();

        let filter = PathFilter::new(vec![], vec!["*.log".into()]);
        let mut watcher = test_watcher(dir.path(), filter);

        // Small delay to let the watcher do its initial scan
        std::thread::sleep(Duration::from_millis(500));

        fs::write(&log_path, b"more log").unwrap();
        fs::write(&rs_path, b"more code").unwrap();

        let changes = poll_with_retry(&mut watcher, 10);

        // Only the .rs file change should come through
        let paths: Vec<_> = changes.iter().map(|c| c.path.clone()).collect();
        assert!(!paths.iter().any(|p| p.to_string_lossy().contains(".log")),
            "log files should be filtered out");
    }

    #[test]
    fn detect_new_file() {
        let dir = TempDir::new().unwrap();
        let filter = PathFilter::new(vec![], vec![]);
        let mut watcher = test_watcher(dir.path(), filter);

        // Small delay to let the watcher do its initial scan
        std::thread::sleep(Duration::from_millis(500));

        // Create a new file
        let new_file = dir.path().join("new.rs");
        fs::write(&new_file, b"new content").unwrap();

        let changes = poll_with_retry(&mut watcher, 10);
        assert!(!changes.is_empty(), "should detect new file creation");
    }
}

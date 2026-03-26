use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq)]
pub enum ChangeKind {
    Created,
    Modified,
    Deleted,
    Renamed,
}

#[derive(Debug, Clone)]
pub struct RawChange {
    pub path: PathBuf,
    pub kind: ChangeKind,
}

/// Collects raw filesystem events and coalesces them by path.
/// When drained, emits one event per unique path (last event wins).
pub struct Debouncer {
    pending: HashMap<PathBuf, ChangeKind>,
}

impl Debouncer {
    pub fn new() -> Self {
        Debouncer {
            pending: HashMap::new(),
        }
    }

    /// Push a raw change event. If the same path was already pending,
    /// the new kind overwrites the old one.
    pub fn push(&mut self, change: RawChange) {
        self.pending.insert(change.path, change.kind);
    }

    /// Drain all pending changes, returning a batch of coalesced events.
    pub fn drain(&mut self) -> Vec<RawChange> {
        let batch: Vec<RawChange> = self.pending
            .drain()
            .map(|(path, kind)| RawChange { path, kind })
            .collect();
        batch
    }

    /// Check if there are pending changes.
    pub fn has_pending(&self) -> bool {
        !self.pending.is_empty()
    }
}

impl Default for Debouncer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn single_event_emitted_after_drain() {
        let mut debouncer = Debouncer::new();
        debouncer.push(RawChange {
            path: PathBuf::from("src/main.rs"),
            kind: ChangeKind::Modified,
        });
        let batch = debouncer.drain();
        assert_eq!(batch.len(), 1);
        assert_eq!(batch[0].path, PathBuf::from("src/main.rs"));
    }

    #[test]
    fn duplicate_paths_coalesced() {
        let mut debouncer = Debouncer::new();
        debouncer.push(RawChange {
            path: PathBuf::from("src/main.rs"),
            kind: ChangeKind::Modified,
        });
        debouncer.push(RawChange {
            path: PathBuf::from("src/main.rs"),
            kind: ChangeKind::Modified,
        });
        let batch = debouncer.drain();
        assert_eq!(batch.len(), 1);
    }

    #[test]
    fn different_paths_preserved() {
        let mut debouncer = Debouncer::new();
        debouncer.push(RawChange {
            path: PathBuf::from("a.rs"),
            kind: ChangeKind::Modified,
        });
        debouncer.push(RawChange {
            path: PathBuf::from("b.rs"),
            kind: ChangeKind::Created,
        });
        let batch = debouncer.drain();
        assert_eq!(batch.len(), 2);
    }

    #[test]
    fn drain_clears_buffer() {
        let mut debouncer = Debouncer::new();
        debouncer.push(RawChange {
            path: PathBuf::from("file.rs"),
            kind: ChangeKind::Modified,
        });
        let _ = debouncer.drain();
        let batch = debouncer.drain();
        assert!(batch.is_empty());
    }

    #[test]
    fn last_kind_wins_for_same_path() {
        let mut debouncer = Debouncer::new();
        debouncer.push(RawChange {
            path: PathBuf::from("file.rs"),
            kind: ChangeKind::Created,
        });
        debouncer.push(RawChange {
            path: PathBuf::from("file.rs"),
            kind: ChangeKind::Deleted,
        });
        let batch = debouncer.drain();
        assert_eq!(batch.len(), 1);
        assert_eq!(batch[0].kind, ChangeKind::Deleted);
    }
}

use std::collections::{HashMap, HashSet};

/// An in-memory overlay that captures writes on top of the immutable DAG.
/// Reads check the overlay first; if not found, fall through to the DAG/CAS.
pub struct Overlay {
    /// Written/modified files: path -> data
    files: HashMap<String, Vec<u8>>,
    /// Deleted paths (mask out DAG entries)
    deleted: HashSet<String>,
}

impl Overlay {
    pub fn new() -> Self {
        Overlay {
            files: HashMap::new(),
            deleted: HashSet::new(),
        }
    }

    /// Write data to a path in the overlay.
    pub fn write(&mut self, path: &str, data: Vec<u8>) {
        self.deleted.remove(path);
        self.files.insert(path.to_string(), data);
    }

    /// Read data from the overlay. Returns None if path is not in the overlay.
    pub fn read(&self, path: &str) -> Option<&[u8]> {
        if self.deleted.contains(path) {
            return None;
        }
        self.files.get(path).map(|v| v.as_slice())
    }

    /// Mark a path as deleted in the overlay.
    pub fn delete(&mut self, path: &str) {
        self.files.remove(path);
        self.deleted.insert(path.to_string());
    }

    /// Check if a path is explicitly deleted.
    pub fn is_deleted(&self, path: &str) -> bool {
        self.deleted.contains(path)
    }

    /// Return all modified paths (written + deleted).
    pub fn modified_paths(&self) -> Vec<&str> {
        let mut paths: Vec<&str> = self.files.keys().map(|s| s.as_str()).collect();
        paths.extend(self.deleted.iter().map(|s| s.as_str()));
        paths.sort();
        paths
    }

    /// Clear all overlay state.
    pub fn clear(&mut self) {
        self.files.clear();
        self.deleted.clear();
    }

    /// Check if the overlay has any modifications.
    pub fn is_empty(&self) -> bool {
        self.files.is_empty() && self.deleted.is_empty()
    }
}

impl Default for Overlay {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_and_read_back() {
        let mut overlay = Overlay::new();
        overlay.write("/app/new.txt", b"hello".to_vec());
        assert_eq!(overlay.read("/app/new.txt"), Some(b"hello".as_slice()));
    }

    #[test]
    fn read_nonexistent_returns_none() {
        let overlay = Overlay::new();
        assert!(overlay.read("/missing").is_none());
    }

    #[test]
    fn overwrite_replaces_data() {
        let mut overlay = Overlay::new();
        overlay.write("/file", b"v1".to_vec());
        overlay.write("/file", b"v2".to_vec());
        assert_eq!(overlay.read("/file"), Some(b"v2".as_slice()));
    }

    #[test]
    fn delete_marks_as_deleted() {
        let mut overlay = Overlay::new();
        overlay.write("/file", b"data".to_vec());
        overlay.delete("/file");
        assert!(overlay.is_deleted("/file"));
        assert!(overlay.read("/file").is_none());
    }

    #[test]
    fn list_modified_paths() {
        let mut overlay = Overlay::new();
        overlay.write("/a", b"1".to_vec());
        overlay.write("/b", b"2".to_vec());
        overlay.delete("/c");
        let paths = overlay.modified_paths();
        assert_eq!(paths.len(), 3);
    }

    #[test]
    fn clear_resets_state() {
        let mut overlay = Overlay::new();
        overlay.write("/file", b"data".to_vec());
        overlay.clear();
        assert!(overlay.read("/file").is_none());
        assert!(overlay.modified_paths().is_empty());
    }
}

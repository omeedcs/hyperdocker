use std::collections::HashMap;
use hd_cas::ContentHash;

/// Bidirectional mapping between host filesystem paths and DAG node hashes.
pub struct PathMap {
    path_to_hash: HashMap<String, ContentHash>,
    hash_to_path: HashMap<ContentHash, String>,
}

impl PathMap {
    pub fn new() -> Self {
        PathMap {
            path_to_hash: HashMap::new(),
            hash_to_path: HashMap::new(),
        }
    }

    pub fn insert(&mut self, path: &str, hash: ContentHash) {
        // Remove old mapping if path existed
        if let Some(old_hash) = self.path_to_hash.remove(path) {
            self.hash_to_path.remove(&old_hash);
        }
        self.path_to_hash.insert(path.to_string(), hash);
        self.hash_to_path.insert(hash, path.to_string());
    }

    pub fn get_hash(&self, path: &str) -> Option<&ContentHash> {
        self.path_to_hash.get(path)
    }

    pub fn get_path(&self, hash: &ContentHash) -> Option<&str> {
        self.hash_to_path.get(hash).map(|s| s.as_str())
    }

    pub fn remove(&mut self, path: &str) {
        if let Some(hash) = self.path_to_hash.remove(path) {
            self.hash_to_path.remove(&hash);
        }
    }

    pub fn all_paths(&self) -> Vec<&str> {
        self.path_to_hash.keys().map(|s| s.as_str()).collect()
    }
}

impl Default for PathMap {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hd_cas::ContentHash;

    #[test]
    fn insert_and_lookup_by_path() {
        let mut map = PathMap::new();
        let hash = ContentHash::from_bytes(b"file1");
        map.insert("src/main.rs", hash);
        assert_eq!(map.get_hash("src/main.rs"), Some(&hash));
    }

    #[test]
    fn insert_and_lookup_by_hash() {
        let mut map = PathMap::new();
        let hash = ContentHash::from_bytes(b"file1");
        map.insert("src/main.rs", hash);
        assert_eq!(map.get_path(&hash), Some("src/main.rs"));
    }

    #[test]
    fn update_replaces_mapping() {
        let mut map = PathMap::new();
        let h1 = ContentHash::from_bytes(b"v1");
        let h2 = ContentHash::from_bytes(b"v2");
        map.insert("file.rs", h1);
        map.insert("file.rs", h2);
        assert_eq!(map.get_hash("file.rs"), Some(&h2));
        assert!(map.get_path(&h1).is_none()); // old hash removed
    }

    #[test]
    fn remove_by_path() {
        let mut map = PathMap::new();
        let hash = ContentHash::from_bytes(b"data");
        map.insert("file.rs", hash);
        map.remove("file.rs");
        assert!(map.get_hash("file.rs").is_none());
        assert!(map.get_path(&hash).is_none());
    }

    #[test]
    fn all_paths() {
        let mut map = PathMap::new();
        map.insert("a.rs", ContentHash::from_bytes(b"a"));
        map.insert("b.rs", ContentHash::from_bytes(b"b"));
        let paths = map.all_paths();
        assert_eq!(paths.len(), 2);
    }
}

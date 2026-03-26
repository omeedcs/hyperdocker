const DEFAULT_EXCLUDES: &[&str] = &[
    ".git",
    "node_modules/.cache",
    ".DS_Store",
    "target",
];

/// Filters file paths based on include/exclude patterns from the EnvSpec.
pub struct PathFilter {
    include_prefixes: Vec<String>,
    exclude_patterns: Vec<String>,
}

impl PathFilter {
    pub fn new(include: Vec<String>, exclude: Vec<String>) -> Self {
        PathFilter {
            include_prefixes: include,
            exclude_patterns: exclude,
        }
    }

    /// Check if a path should be watched (included and not excluded).
    pub fn is_included(&self, path: &str) -> bool {
        // Check default excludes first
        for excl in DEFAULT_EXCLUDES {
            if path.starts_with(excl) || path.contains(&format!("/{}", excl)) {
                return false;
            }
        }

        // Check user excludes (glob-style suffix matching)
        for pattern in &self.exclude_patterns {
            if glob_matches(pattern, path) {
                return false;
            }
        }

        // Check includes: if empty, include everything. If specified, path must
        // start with one of the include prefixes or match exactly.
        if self.include_prefixes.is_empty() {
            return true;
        }

        self.include_prefixes.iter().any(|prefix| {
            path == prefix || path.starts_with(&format!("{}/", prefix))
        })
    }
}

/// Simple glob matching: supports *.ext patterns.
fn glob_matches(pattern: &str, path: &str) -> bool {
    if let Some(ext) = pattern.strip_prefix("*.") {
        let filename = path.rsplit('/').next().unwrap_or(path);
        filename.ends_with(&format!(".{}", ext))
    } else {
        path == pattern || path.starts_with(&format!("{}/", pattern))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn include_matches() {
        let filter = PathFilter::new(
            vec!["src".into(), "package.json".into()],
            vec![],
        );
        assert!(filter.is_included("src/main.rs"));
        assert!(filter.is_included("src/lib/utils.rs"));
        assert!(filter.is_included("package.json"));
        assert!(!filter.is_included("README.md"));
    }

    #[test]
    fn exclude_overrides_include() {
        let filter = PathFilter::new(
            vec!["src".into()],
            vec!["*.log".into()],
        );
        assert!(filter.is_included("src/main.rs"));
        assert!(!filter.is_included("src/debug.log"));
    }

    #[test]
    fn default_excludes() {
        let filter = PathFilter::new(vec![], vec![]);
        assert!(!filter.is_included(".git/config"));
        assert!(!filter.is_included("node_modules/.cache/foo"));
    }

    #[test]
    fn empty_include_means_all() {
        let filter = PathFilter::new(vec![], vec![]);
        assert!(filter.is_included("any/file.rs"));
        assert!(filter.is_included("other.txt"));
    }

    #[test]
    fn glob_patterns_in_exclude() {
        let filter = PathFilter::new(
            vec!["src".into()],
            vec!["*.tmp".into(), "*.swp".into()],
        );
        assert!(!filter.is_included("src/file.tmp"));
        assert!(!filter.is_included("src/.main.rs.swp"));
        assert!(filter.is_included("src/main.rs"));
    }
}

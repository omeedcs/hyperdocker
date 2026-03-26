use hd_cas::ContentHash;
use serde::{Deserialize, Serialize};

/// A node in the Merkle DAG. Each variant computes its content hash
/// deterministically from its inputs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Node {
    File {
        path: String,
        manifest_hash: ContentHash,
    },
    Dir {
        path: String,
        /// Sorted vec of (child_name, child_content_hash)
        children: Vec<(String, ContentHash)>,
    },
    Package {
        provider: String,
        name: String,
        version: String,
        artifact_hash: ContentHash,
    },
    BuildStep {
        command: String,
        input_hashes: Vec<ContentHash>,
        env_vars: Vec<(String, String)>,
    },
    Env {
        name: String,
        children: Vec<ContentHash>,
    },
}

impl Node {
    pub fn file(path: &str, manifest_hash: ContentHash) -> Self {
        Node::File {
            path: path.to_string(),
            manifest_hash,
        }
    }

    pub fn dir(path: &str, mut children: Vec<(String, ContentHash)>) -> Self {
        children.sort_by(|a, b| a.0.cmp(&b.0));
        Node::Dir {
            path: path.to_string(),
            children,
        }
    }

    pub fn package(provider: &str, name: &str, version: &str, artifact_hash: ContentHash) -> Self {
        Node::Package {
            provider: provider.to_string(),
            name: name.to_string(),
            version: version.to_string(),
            artifact_hash,
        }
    }

    pub fn build_step(command: &str, input_hashes: Vec<ContentHash>, mut env_vars: Vec<(String, String)>) -> Self {
        env_vars.sort_by(|a, b| a.0.cmp(&b.0));
        Node::BuildStep {
            command: command.to_string(),
            input_hashes,
            env_vars,
        }
    }

    pub fn env(name: &str, children: Vec<ContentHash>) -> Self {
        Node::Env {
            name: name.to_string(),
            children,
        }
    }

    /// Compute the content hash of this node.
    pub fn content_hash(&self) -> ContentHash {
        let mut hasher = blake3::Hasher::new();

        match self {
            Node::File { path, manifest_hash } => {
                hasher.update(b"file:");
                hasher.update(path.as_bytes());
                hasher.update(manifest_hash.as_bytes());
            }
            Node::Dir { path, children } => {
                hasher.update(b"dir:");
                hasher.update(path.as_bytes());
                for (name, hash) in children {
                    hasher.update(name.as_bytes());
                    hasher.update(hash.as_bytes());
                }
            }
            Node::Package { provider, name, version, artifact_hash } => {
                hasher.update(b"pkg:");
                hasher.update(provider.as_bytes());
                hasher.update(name.as_bytes());
                hasher.update(version.as_bytes());
                hasher.update(artifact_hash.as_bytes());
            }
            Node::BuildStep { command, input_hashes, env_vars } => {
                hasher.update(b"build:");
                hasher.update(command.as_bytes());
                for ih in input_hashes {
                    hasher.update(ih.as_bytes());
                }
                for (k, v) in env_vars {
                    hasher.update(k.as_bytes());
                    hasher.update(b"=");
                    hasher.update(v.as_bytes());
                }
            }
            Node::Env { name, children } => {
                hasher.update(b"env:");
                hasher.update(name.as_bytes());
                for child in children {
                    hasher.update(child.as_bytes());
                }
            }
        }

        ContentHash::from_raw(*hasher.finalize().as_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hd_cas::ContentHash;

    #[test]
    fn file_node_hash_deterministic() {
        let manifest_hash = ContentHash::from_bytes(b"manifest1");
        let n1 = Node::file("src/main.rs", manifest_hash);
        let n2 = Node::file("src/main.rs", manifest_hash);
        assert_eq!(n1.content_hash(), n2.content_hash());
    }

    #[test]
    fn file_node_hash_changes_with_content() {
        let n1 = Node::file("src/main.rs", ContentHash::from_bytes(b"v1"));
        let n2 = Node::file("src/main.rs", ContentHash::from_bytes(b"v2"));
        assert_ne!(n1.content_hash(), n2.content_hash());
    }

    #[test]
    fn dir_node_hash_from_sorted_children() {
        let child_a = ContentHash::from_bytes(b"a");
        let child_b = ContentHash::from_bytes(b"b");

        let n1 = Node::dir("src", vec![
            ("a.rs".into(), child_a),
            ("b.rs".into(), child_b),
        ]);
        let n2 = Node::dir("src", vec![
            ("b.rs".into(), child_b),
            ("a.rs".into(), child_a),
        ]);
        assert_eq!(n1.content_hash(), n2.content_hash());
    }

    #[test]
    fn build_step_hash_includes_command_and_inputs() {
        let input = ContentHash::from_bytes(b"input");
        let n1 = Node::build_step("npm install", vec![input], vec![]);
        let n2 = Node::build_step("npm install", vec![input], vec![]);
        assert_eq!(n1.content_hash(), n2.content_hash());

        let n3 = Node::build_step("npm ci", vec![input], vec![]);
        assert_ne!(n1.content_hash(), n3.content_hash());
    }

    #[test]
    fn build_step_hash_includes_env_vars() {
        let input = ContentHash::from_bytes(b"input");
        let n1 = Node::build_step("make", vec![input], vec![("CC".into(), "gcc".into())]);
        let n2 = Node::build_step("make", vec![input], vec![("CC".into(), "clang".into())]);
        assert_ne!(n1.content_hash(), n2.content_hash());
    }

    #[test]
    fn env_node_hash_from_children() {
        let child1 = ContentHash::from_bytes(b"child1");
        let child2 = ContentHash::from_bytes(b"child2");
        let env = Node::env("myapp", vec![child1, child2]);
        assert_eq!(env.content_hash(), env.content_hash());
    }

    #[test]
    fn package_node_hash() {
        let n1 = Node::package("npm", "express", "4.18.2", ContentHash::from_bytes(b"express-pkg"));
        let n2 = Node::package("npm", "express", "4.18.2", ContentHash::from_bytes(b"express-pkg"));
        assert_eq!(n1.content_hash(), n2.content_hash());

        let n3 = Node::package("npm", "express", "4.19.0", ContentHash::from_bytes(b"express-pkg-new"));
        assert_ne!(n1.content_hash(), n3.content_hash());
    }
}

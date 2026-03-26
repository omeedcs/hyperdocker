// OCI registry client — image reference parsing and manifest types.

#[derive(Debug, Clone)]
pub struct ImageRef {
    pub registry: String,
    pub repository: String,
    pub tag: String,
}

#[derive(Debug, thiserror::Error)]
pub enum RegistryError {
    #[error("invalid image reference: {0}")]
    InvalidRef(String),
    #[error("registry error: {0}")]
    Registry(String),
    #[error("HTTP error: {0}")]
    Http(String),
}

impl ImageRef {
    pub fn parse(reference: &str) -> Result<Self, RegistryError> {
        let (name, tag) = if let Some((n, t)) = reference.rsplit_once(':') {
            // Check if the colon is part of a port (e.g., localhost:5000/image)
            if n.contains('/') || !t.contains('/') {
                (n.to_string(), t.to_string())
            } else {
                (reference.to_string(), "latest".to_string())
            }
        } else {
            (reference.to_string(), "latest".to_string())
        };

        let (registry, repository) =
            if name.contains('.') || name.contains(':') || name.contains("localhost") {
                // Has a registry prefix
                if let Some((reg, repo)) = name.split_once('/') {
                    (reg.to_string(), repo.to_string())
                } else {
                    (name.clone(), name)
                }
            } else if name.contains('/') {
                // Docker Hub with org
                ("registry-1.docker.io".to_string(), name)
            } else {
                // Docker Hub official image
                (
                    "registry-1.docker.io".to_string(),
                    format!("library/{}", name),
                )
            };

        Ok(ImageRef {
            registry,
            repository,
            tag,
        })
    }
}

impl std::fmt::Display for ImageRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}/{}:{}", self.registry, self.repository, self.tag)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_simple_reference() {
        let r = ImageRef::parse("ubuntu:22.04").unwrap();
        assert_eq!(r.registry, "registry-1.docker.io");
        assert_eq!(r.repository, "library/ubuntu");
        assert_eq!(r.tag, "22.04");
    }

    #[test]
    fn parse_full_reference() {
        let r = ImageRef::parse("ghcr.io/owner/repo:v1.0").unwrap();
        assert_eq!(r.registry, "ghcr.io");
        assert_eq!(r.repository, "owner/repo");
        assert_eq!(r.tag, "v1.0");
    }

    #[test]
    fn parse_no_tag_defaults_latest() {
        let r = ImageRef::parse("alpine").unwrap();
        assert_eq!(r.tag, "latest");
    }

    #[test]
    fn parse_with_port() {
        let r = ImageRef::parse("localhost:5000/myimage:dev").unwrap();
        assert_eq!(r.registry, "localhost:5000");
        assert_eq!(r.repository, "myimage");
        assert_eq!(r.tag, "dev");
    }
}

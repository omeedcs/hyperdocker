// Dockerfile parsing and translation to hd.toml.

use std::collections::HashMap;

use hd_spec::{
    BuildConfig, EnvironmentConfig, EnvSpec, FilesConfig, OptionsConfig, RestartPolicy,
    ServiceConfig,
};

#[derive(Debug, thiserror::Error)]
pub enum DockerfileError {
    #[error("no FROM instruction found")]
    NoFrom,
    #[error("parse error: {0}")]
    ParseError(String),
}

/// Translate a Dockerfile into an EnvSpec (hd.toml equivalent).
/// This is a best-effort translation -- arbitrary shell commands become build steps.
pub fn translate_dockerfile(content: &str) -> Result<EnvSpec, DockerfileError> {
    let lines: Vec<&str> = content
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .collect();

    if lines.is_empty() {
        return Err(DockerfileError::NoFrom);
    }

    let mut base = String::new();
    let mut build_steps = Vec::new();
    let mut cmd = Vec::new();
    let mut name = "app".to_string();

    for line in &lines {
        let upper = line.to_uppercase();
        if upper.starts_with("FROM ") {
            base = line[5..].trim().to_string();
            // Derive name from base image
            if let Some(img_name) = base.split('/').next_back() {
                name = img_name.split(':').next().unwrap_or("app").to_string();
            }
        } else if upper.starts_with("RUN ") {
            build_steps.push(line[4..].trim().to_string());
        } else if upper.starts_with("CMD ") {
            let cmd_str = line[4..].trim();
            // Parse JSON array format: ["node", "server.js"] -> "node server.js"
            if cmd_str.starts_with('[') {
                let parsed: Vec<String> = cmd_str
                    .trim_start_matches('[')
                    .trim_end_matches(']')
                    .split(',')
                    .map(|s| s.trim().trim_matches('"').to_string())
                    .collect();
                cmd = parsed;
            } else {
                cmd = vec![cmd_str.to_string()];
            }
        }
        // COPY, WORKDIR, ENV, EXPOSE etc. are noted but don't map cleanly to hd.toml
        // They become part of the build context
    }

    if base.is_empty() {
        return Err(DockerfileError::NoFrom);
    }

    let mut services = HashMap::new();
    if !cmd.is_empty() {
        services.insert(
            "app".to_string(),
            ServiceConfig {
                command: cmd.join(" "),
                watch: vec![],
                port: None,
                depends_on: vec![],
                restart_policy: RestartPolicy::Always,
            },
        );
    }

    Ok(EnvSpec {
        environment: EnvironmentConfig { name, base },
        dependencies: HashMap::new(),
        build: BuildConfig {
            steps: build_steps,
            cache: vec![],
        },
        services,
        files: FilesConfig::default(),
        options: OptionsConfig::default(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translate_simple_dockerfile() {
        let dockerfile = r#"
FROM node:20-alpine
RUN npm install
COPY . .
CMD ["node", "server.js"]
"#;
        let spec = translate_dockerfile(dockerfile).unwrap();
        assert_eq!(spec.environment.base, "node:20-alpine");
        assert!(spec.build.steps.contains(&"npm install".to_string()));
        assert!(!spec.services.is_empty());
    }

    #[test]
    fn translate_with_dependencies() {
        let dockerfile = r#"
FROM ubuntu:22.04
RUN apt-get update && apt-get install -y curl git
RUN npm install
"#;
        let spec = translate_dockerfile(dockerfile).unwrap();
        assert_eq!(spec.environment.base, "ubuntu:22.04");
        assert_eq!(spec.build.steps.len(), 2);
    }

    #[test]
    fn translate_workdir_and_env() {
        let dockerfile = r#"
FROM python:3.11
WORKDIR /app
ENV PORT=8080
RUN pip install flask
CMD ["python", "app.py"]
"#;
        let spec = translate_dockerfile(dockerfile).unwrap();
        assert_eq!(spec.environment.base, "python:3.11");
    }

    #[test]
    fn translate_empty_dockerfile_errors() {
        let result = translate_dockerfile("");
        assert!(result.is_err());
    }
}

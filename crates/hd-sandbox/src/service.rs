// crates/hd-sandbox/src/service.rs
use crate::process::{ManagedProcess, ProcessError};

#[derive(Debug, Clone, PartialEq)]
pub enum ServiceState {
    Stopped,
    Running,
    Failed(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum RestartPolicy {
    Always,
    OnFailure,
    Never,
}

#[derive(Debug, Clone)]
pub struct ServiceDef {
    pub name: String,
    pub command: String,
    pub args: Vec<String>,
    pub watch_patterns: Vec<String>,
    pub depends_on: Vec<String>,
    pub restart_policy: RestartPolicy,
}

pub struct Service {
    def: ServiceDef,
    process: Option<ManagedProcess>,
    state: ServiceState,
}

#[derive(Debug, thiserror::Error)]
pub enum ServiceError {
    #[error("process error: {0}")]
    Process(#[from] ProcessError),
    #[error("service '{0}' is not running")]
    NotRunning(String),
}

impl Service {
    pub fn new(def: ServiceDef) -> Self {
        Service {
            def,
            process: None,
            state: ServiceState::Stopped,
        }
    }

    pub fn start(&mut self) -> Result<(), ServiceError> {
        let args: Vec<&str> = self.def.args.iter().map(|s| s.as_str()).collect();
        let proc = ManagedProcess::spawn(&self.def.command, &args)?;
        self.process = Some(proc);
        self.state = ServiceState::Running;
        Ok(())
    }

    pub fn stop(&mut self) -> Result<(), ServiceError> {
        if let Some(mut proc) = self.process.take() {
            proc.kill()?;
        }
        self.state = ServiceState::Stopped;
        Ok(())
    }

    pub fn restart(&mut self) -> Result<(), ServiceError> {
        self.stop()?;
        self.start()
    }

    pub fn state(&self) -> ServiceState {
        self.state.clone()
    }

    pub fn pid(&self) -> Option<u32> {
        self.process.as_ref().map(|p| p.pid())
    }

    pub fn name(&self) -> &str {
        &self.def.name
    }

    pub fn depends_on(&self) -> &[String] {
        &self.def.depends_on
    }

    /// Check if this service should restart based on a changed path.
    /// Uses simple glob-style matching on watch_patterns.
    pub fn should_restart_for(&self, changed_path: &str) -> bool {
        if self.def.watch_patterns.is_empty() {
            return false;
        }
        self.def
            .watch_patterns
            .iter()
            .any(|pattern| glob_match(pattern, changed_path))
    }
}

/// Simple glob matching supporting ** and * patterns.
fn glob_match(pattern: &str, path: &str) -> bool {
    if pattern.contains("**") {
        // "src/**/*.ts" matches "src/foo/bar.ts"
        let parts: Vec<&str> = pattern.split("**").collect();
        if parts.len() == 2 {
            let prefix = parts[0].trim_end_matches('/');
            let suffix_pattern = parts[1].trim_start_matches('/');
            if !path.starts_with(prefix) {
                return false;
            }
            let remaining = &path[prefix.len()..].trim_start_matches('/');
            if let Some(ext) = suffix_pattern.strip_prefix("*.") {
                return remaining.ends_with(&format!(".{}", ext));
            }
            return true;
        }
    }
    if let Some(ext) = pattern.strip_prefix("*.") {
        return path.ends_with(&format!(".{}", ext));
    }
    path == pattern || path.starts_with(&format!("{}/", pattern))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn service_starts_and_stops() {
        let config = ServiceDef {
            name: "echo-svc".into(),
            command: "sleep".into(),
            args: vec!["10".into()],
            watch_patterns: vec![],
            depends_on: vec![],
            restart_policy: RestartPolicy::Always,
        };
        let mut svc = Service::new(config);
        svc.start().unwrap();
        assert_eq!(svc.state(), ServiceState::Running);
        svc.stop().unwrap();
        assert_eq!(svc.state(), ServiceState::Stopped);
    }

    #[test]
    fn service_restart() {
        let config = ServiceDef {
            name: "restart-test".into(),
            command: "sleep".into(),
            args: vec!["10".into()],
            watch_patterns: vec![],
            depends_on: vec![],
            restart_policy: RestartPolicy::Always,
        };
        let mut svc = Service::new(config);
        svc.start().unwrap();
        let old_pid = svc.pid().unwrap();
        svc.restart().unwrap();
        let new_pid = svc.pid().unwrap();
        assert_ne!(old_pid, new_pid);
        svc.stop().unwrap();
    }

    #[test]
    fn watch_pattern_matching() {
        let config = ServiceDef {
            name: "web".into(),
            command: "sleep".into(),
            args: vec!["10".into()],
            watch_patterns: vec!["src/**/*.ts".into(), "src/**/*.tsx".into()],
            depends_on: vec![],
            restart_policy: RestartPolicy::Always,
        };
        let svc = Service::new(config);
        assert!(svc.should_restart_for("src/main.ts"));
        assert!(svc.should_restart_for("src/components/App.tsx"));
        assert!(!svc.should_restart_for("README.md"));
        assert!(!svc.should_restart_for("config.json"));
    }

    #[test]
    fn empty_watch_never_matches() {
        let config = ServiceDef {
            name: "static".into(),
            command: "sleep".into(),
            args: vec!["10".into()],
            watch_patterns: vec![],
            depends_on: vec![],
            restart_policy: RestartPolicy::Always,
        };
        let svc = Service::new(config);
        assert!(!svc.should_restart_for("anything.rs"));
    }

    #[test]
    fn stop_already_stopped_is_noop() {
        let config = ServiceDef {
            name: "noop".into(),
            command: "sleep".into(),
            args: vec!["10".into()],
            watch_patterns: vec![],
            depends_on: vec![],
            restart_policy: RestartPolicy::Always,
        };
        let mut svc = Service::new(config);
        assert!(svc.stop().is_ok()); // not started, should be fine
    }

    #[test]
    fn service_state_transitions() {
        let config = ServiceDef {
            name: "transitions".into(),
            command: "echo".into(),
            args: vec!["hi".into()],
            watch_patterns: vec![],
            depends_on: vec![],
            restart_policy: RestartPolicy::Always,
        };
        let svc = Service::new(config);
        assert_eq!(svc.state(), ServiceState::Stopped);
    }
}

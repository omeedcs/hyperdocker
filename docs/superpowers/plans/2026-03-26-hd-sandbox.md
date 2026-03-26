# hd-sandbox Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the `hd-sandbox` crate — long-lived process sandboxes that manage services, handle selective restarts based on DAG changes, and provide exec access.

**Architecture:** Platform-agnostic service management layer. On Linux, sandboxes use namespaces (mount, PID, network, user, IPC). On macOS, sandboxes use process groups with isolated environment variables. The core `Sandbox` struct manages service lifecycle: start in dependency order, selective restart based on changed paths, stop in reverse order. The platform-specific isolation is behind a trait so the service management logic is testable everywhere.

**Tech Stack:** Rust, hd-spec (for ServiceConfig), nix (for Unix process management)

---

## File Structure

```
crates/
  hd-sandbox/
    Cargo.toml
    src/
      lib.rs
      service.rs         # Service lifecycle: start, stop, restart, status
      sandbox.rs         # Sandbox struct: owns services + isolation context
      process.rs         # Process spawning and management (cross-platform)
    tests/
      integration.rs
```

---

## Task 1: Crate Setup

- [ ] Add `"crates/hd-sandbox"` to workspace members. Add `nix = { version = "0.29", features = ["signal", "process"] }` to workspace deps.
- [ ] Create `crates/hd-sandbox/Cargo.toml`:
```toml
[package]
name = "hd-sandbox"
edition.workspace = true
license.workspace = true
version.workspace = true

[dependencies]
hd-spec = { path = "../hd-spec" }
nix.workspace = true
thiserror.workspace = true

[dev-dependencies]
tempfile.workspace = true
```
- [ ] Create lib.rs (`pub mod service; pub mod sandbox; pub mod process;`) and empty module files.
- [ ] `cargo build` and commit: `"feat: add hd-sandbox crate to workspace"`

---

## Task 2: Process Management (`process.rs`)

Cross-platform process spawning with graceful shutdown.

- [ ] **Tests (5):**
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spawn_and_check_running() {
        let mut proc = ManagedProcess::spawn("sleep", &["10"]).unwrap();
        assert!(proc.is_running());
        proc.kill().unwrap();
    }

    #[test]
    fn spawn_and_wait() {
        let mut proc = ManagedProcess::spawn("echo", &["hello"]).unwrap();
        let status = proc.wait().unwrap();
        assert!(status.success());
    }

    #[test]
    fn kill_terminates() {
        let mut proc = ManagedProcess::spawn("sleep", &["60"]).unwrap();
        assert!(proc.is_running());
        proc.kill().unwrap();
        assert!(!proc.is_running());
    }

    #[test]
    fn pid_is_valid() {
        let mut proc = ManagedProcess::spawn("sleep", &["10"]).unwrap();
        assert!(proc.pid() > 0);
        proc.kill().unwrap();
    }

    #[test]
    fn spawn_nonexistent_command_fails() {
        let result = ManagedProcess::spawn("nonexistent_binary_xyz", &[]);
        assert!(result.is_err());
    }
}
```

- [ ] **Implementation:**
```rust
// crates/hd-sandbox/src/process.rs
use std::process::{Child, Command, ExitStatus};

#[derive(Debug, thiserror::Error)]
pub enum ProcessError {
    #[error("spawn failed: {0}")]
    SpawnFailed(#[from] std::io::Error),
    #[error("process not running")]
    NotRunning,
}

/// A managed child process with lifecycle control.
pub struct ManagedProcess {
    child: Child,
}

impl ManagedProcess {
    pub fn spawn(command: &str, args: &[&str]) -> Result<Self, ProcessError> {
        let child = Command::new(command)
            .args(args)
            .spawn()?;
        Ok(ManagedProcess { child })
    }

    pub fn pid(&self) -> u32 {
        self.child.id()
    }

    pub fn is_running(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }

    pub fn wait(&mut self) -> Result<ExitStatus, ProcessError> {
        self.child.wait().map_err(ProcessError::SpawnFailed)
    }

    pub fn kill(&mut self) -> Result<(), ProcessError> {
        let _ = self.child.kill();
        let _ = self.child.wait();
        Ok(())
    }
}

impl Drop for ManagedProcess {
    fn drop(&mut self) {
        let _ = self.kill();
    }
}
```

- [ ] 5 tests PASS. Commit: `"feat(hd-sandbox): add ManagedProcess for process lifecycle"`

---

## Task 3: Service Management (`service.rs`)

Service lifecycle: tracks state, handles restart matching against changed paths.

- [ ] **Tests (6):**
```rust
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
```

- [ ] **Implementation:**
```rust
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
        Service { def, process: None, state: ServiceState::Stopped }
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
        self.def.watch_patterns.iter().any(|pattern| {
            glob_match(pattern, changed_path)
        })
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
```

- [ ] 6 tests PASS. Commit: `"feat(hd-sandbox): add Service with lifecycle management and watch pattern matching"`

---

## Task 4: Sandbox (`sandbox.rs`)

The top-level Sandbox struct: owns services, handles dependency ordering, selective restarts.

- [ ] **Tests (4):**
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::service::{ServiceDef, RestartPolicy};

    fn test_services() -> Vec<ServiceDef> {
        vec![
            ServiceDef {
                name: "db".into(),
                command: "sleep".into(),
                args: vec!["10".into()],
                watch_patterns: vec![],
                depends_on: vec![],
                restart_policy: RestartPolicy::Always,
            },
            ServiceDef {
                name: "web".into(),
                command: "sleep".into(),
                args: vec!["10".into()],
                watch_patterns: vec!["src/**/*.rs".into()],
                depends_on: vec!["db".into()],
                restart_policy: RestartPolicy::Always,
            },
        ]
    }

    #[test]
    fn start_and_stop_all() {
        let mut sandbox = Sandbox::new(test_services());
        sandbox.start_all().unwrap();
        assert_eq!(sandbox.running_count(), 2);
        sandbox.stop_all().unwrap();
        assert_eq!(sandbox.running_count(), 0);
    }

    #[test]
    fn selective_restart() {
        let mut sandbox = Sandbox::new(test_services());
        sandbox.start_all().unwrap();
        let restarted = sandbox.restart_for_changes(&["src/main.rs".into()]).unwrap();
        assert_eq!(restarted, vec!["web"]);
        sandbox.stop_all().unwrap();
    }

    #[test]
    fn status_report() {
        let sandbox = Sandbox::new(test_services());
        let status = sandbox.status();
        assert_eq!(status.len(), 2);
        assert_eq!(status[0].1, "stopped");
    }

    #[test]
    fn empty_changes_no_restarts() {
        let mut sandbox = Sandbox::new(test_services());
        sandbox.start_all().unwrap();
        let restarted = sandbox.restart_for_changes(&[]).unwrap();
        assert!(restarted.is_empty());
        sandbox.stop_all().unwrap();
    }
}
```

- [ ] **Implementation:**
```rust
// crates/hd-sandbox/src/sandbox.rs
use crate::service::{Service, ServiceDef, ServiceError, ServiceState};

#[derive(Debug, thiserror::Error)]
pub enum SandboxError {
    #[error("service error: {0}")]
    Service(#[from] ServiceError),
}

/// A sandbox owns a set of services and manages their lifecycle.
pub struct Sandbox {
    services: Vec<Service>,
}

impl Sandbox {
    pub fn new(defs: Vec<ServiceDef>) -> Self {
        // Topological sort by depends_on for start order
        let sorted = topo_sort(&defs);
        let services = sorted.into_iter().map(Service::new).collect();
        Sandbox { services }
    }

    /// Start all services in dependency order.
    pub fn start_all(&mut self) -> Result<(), SandboxError> {
        for svc in &mut self.services {
            svc.start()?;
        }
        Ok(())
    }

    /// Stop all services in reverse dependency order.
    pub fn stop_all(&mut self) -> Result<(), SandboxError> {
        for svc in self.services.iter_mut().rev() {
            svc.stop()?;
        }
        Ok(())
    }

    /// Restart services affected by the given changed paths.
    /// Returns the names of restarted services.
    pub fn restart_for_changes(&mut self, changed_paths: &[String]) -> Result<Vec<String>, SandboxError> {
        let mut restarted = Vec::new();
        for svc in &mut self.services {
            let should_restart = changed_paths.iter().any(|p| svc.should_restart_for(p));
            if should_restart {
                svc.restart()?;
                restarted.push(svc.name().to_string());
            }
        }
        Ok(restarted)
    }

    /// Get the count of running services.
    pub fn running_count(&self) -> usize {
        self.services.iter().filter(|s| s.state() == ServiceState::Running).count()
    }

    /// Get status of all services: (name, state_string).
    pub fn status(&self) -> Vec<(String, String)> {
        self.services.iter().map(|s| {
            let state = match s.state() {
                ServiceState::Running => "running".to_string(),
                ServiceState::Stopped => "stopped".to_string(),
                ServiceState::Failed(msg) => format!("failed: {}", msg),
            };
            (s.name().to_string(), state)
        }).collect()
    }
}

/// Simple topological sort for service dependency ordering.
fn topo_sort(defs: &[ServiceDef]) -> Vec<ServiceDef> {
    use std::collections::{HashMap, HashSet};

    let name_to_def: HashMap<&str, &ServiceDef> = defs.iter().map(|d| (d.name.as_str(), d)).collect();
    let mut sorted = Vec::new();
    let mut visited = HashSet::new();

    fn visit<'a>(
        name: &str,
        name_to_def: &HashMap<&str, &'a ServiceDef>,
        visited: &mut HashSet<String>,
        sorted: &mut Vec<ServiceDef>,
    ) {
        if visited.contains(name) {
            return;
        }
        visited.insert(name.to_string());
        if let Some(def) = name_to_def.get(name) {
            for dep in &def.depends_on {
                visit(dep, name_to_def, visited, sorted);
            }
            sorted.push((*def).clone());
        }
    }

    for def in defs {
        visit(&def.name, &name_to_def, &mut visited, &mut sorted);
    }

    sorted
}
```

- [ ] 4 tests PASS. Commit: `"feat(hd-sandbox): add Sandbox with dependency-ordered service management"`

---

## Task 5: Public API & Integration Tests

- [ ] Update lib.rs with re-exports.
- [ ] Integration test: start sandbox, trigger selective restart, verify.
- [ ] Clippy clean.
- [ ] Commit: `"feat(hd-sandbox): add public API and integration tests"`

```rust
// crates/hd-sandbox/src/lib.rs
pub mod process;
pub mod service;
pub mod sandbox;

pub use process::ManagedProcess;
pub use service::{Service, ServiceDef, ServiceState, RestartPolicy};
pub use sandbox::Sandbox;
```

```rust
// crates/hd-sandbox/tests/integration.rs
use hd_sandbox::{Sandbox, ServiceDef, RestartPolicy};

#[test]
fn full_sandbox_lifecycle() {
    let services = vec![
        ServiceDef {
            name: "backend".into(),
            command: "sleep".into(),
            args: vec!["10".into()],
            watch_patterns: vec!["src/**/*.rs".into()],
            depends_on: vec![],
            restart_policy: RestartPolicy::Always,
        },
        ServiceDef {
            name: "frontend".into(),
            command: "sleep".into(),
            args: vec!["10".into()],
            watch_patterns: vec!["web/**/*.ts".into()],
            depends_on: vec!["backend".into()],
            restart_policy: RestartPolicy::Always,
        },
    ];

    let mut sandbox = Sandbox::new(services);

    // Start
    sandbox.start_all().unwrap();
    assert_eq!(sandbox.running_count(), 2);

    // Selective restart: only backend should restart
    let restarted = sandbox.restart_for_changes(&["src/main.rs".into()]).unwrap();
    assert_eq!(restarted, vec!["backend"]);

    // Frontend change: only frontend restarts
    let restarted = sandbox.restart_for_changes(&["web/app.ts".into()]).unwrap();
    assert_eq!(restarted, vec!["frontend"]);

    // Stop
    sandbox.stop_all().unwrap();
    assert_eq!(sandbox.running_count(), 0);
}
```

---

## Summary

| Task | Component | Tests |
|------|-----------|-------|
| 1 | Crate setup | compile check |
| 2 | ManagedProcess | 5 |
| 3 | Service | 6 |
| 4 | Sandbox | 4 |
| 5 | Public API + integration | 1 |

**Total: 5 tasks, ~16 tests, 5 commits.**

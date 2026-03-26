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
    pub fn restart_for_changes(
        &mut self,
        changed_paths: &[String],
    ) -> Result<Vec<String>, SandboxError> {
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
        self.services
            .iter()
            .filter(|s| s.state() == ServiceState::Running)
            .count()
    }

    /// Get status of all services: (name, state_string).
    pub fn status(&self) -> Vec<(String, String)> {
        self.services
            .iter()
            .map(|s| {
                let state = match s.state() {
                    ServiceState::Running => "running".to_string(),
                    ServiceState::Stopped => "stopped".to_string(),
                    ServiceState::Failed(msg) => format!("failed: {}", msg),
                };
                (s.name().to_string(), state)
            })
            .collect()
    }
}

/// Simple topological sort for service dependency ordering.
fn topo_sort(defs: &[ServiceDef]) -> Vec<ServiceDef> {
    use std::collections::{HashMap, HashSet};

    let name_to_def: HashMap<&str, &ServiceDef> =
        defs.iter().map(|d| (d.name.as_str(), d)).collect();
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::service::{RestartPolicy, ServiceDef};

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
        let restarted = sandbox
            .restart_for_changes(&["src/main.rs".into()])
            .unwrap();
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

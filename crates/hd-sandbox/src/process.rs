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

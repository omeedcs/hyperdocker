use std::path::{Path, PathBuf};

use hd_cas::ContentHash;

#[derive(Debug, Clone, PartialEq)]
pub enum MountState {
    Unmounted,
    Mounted,
}

/// Handle to a mounted filesystem.
pub struct MountHandle {
    pub mountpoint: PathBuf,
    pub env_id: ContentHash,
    pub state: MountState,
}

#[derive(Debug, thiserror::Error)]
pub enum MountError {
    #[error("mount failed: {0}")]
    MountFailed(String),
    #[error("already mounted at {0}")]
    AlreadyMounted(String),
    #[error("not mounted")]
    NotMounted,
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

/// Manages mount/unmount lifecycle for projected filesystems.
pub struct MountManager {
    mounts_dir: PathBuf,
    handles: Vec<MountHandle>,
}

impl MountManager {
    pub fn new(mounts_dir: &Path) -> Result<Self, MountError> {
        std::fs::create_dir_all(mounts_dir)?;
        Ok(MountManager {
            mounts_dir: mounts_dir.to_path_buf(),
            handles: Vec::new(),
        })
    }

    /// Get the mountpoint path for an environment.
    pub fn mountpoint_for(&self, env_id: &ContentHash) -> PathBuf {
        self.mounts_dir.join(&env_id.to_hex()[..12])
    }

    /// Register a mount (called after FUSE mount succeeds).
    pub fn register_mount(&mut self, env_id: ContentHash) -> Result<&MountHandle, MountError> {
        let mountpoint = self.mountpoint_for(&env_id);
        if self.is_mounted(&mountpoint) {
            return Err(MountError::AlreadyMounted(mountpoint.display().to_string()));
        }
        std::fs::create_dir_all(&mountpoint)?;
        self.handles.push(MountHandle {
            mountpoint,
            env_id,
            state: MountState::Mounted,
        });
        Ok(self.handles.last().unwrap())
    }

    /// Unregister a mount (called after FUSE unmount succeeds).
    pub fn unregister_mount(&mut self, mountpoint: &Path) -> Result<(), MountError> {
        if let Some(handle) = self.handles.iter_mut().find(|h| h.mountpoint == mountpoint) {
            handle.state = MountState::Unmounted;
            Ok(())
        } else {
            Err(MountError::NotMounted)
        }
    }

    /// Check if a mountpoint is currently mounted.
    pub fn is_mounted(&self, mountpoint: &Path) -> bool {
        self.handles.iter().any(|h| h.mountpoint == mountpoint && h.state == MountState::Mounted)
    }

    /// Get all active mount handles.
    pub fn active_mounts(&self) -> Vec<&MountHandle> {
        self.handles.iter().filter(|h| h.state == MountState::Mounted).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_and_unregister_mount() {
        let dir = tempfile::TempDir::new().unwrap();
        let mut manager = MountManager::new(dir.path()).unwrap();
        let env_id = ContentHash::from_bytes(b"test-env");

        let handle = manager.register_mount(env_id).unwrap();
        assert_eq!(handle.state, MountState::Mounted);

        let mountpoint = manager.mountpoint_for(&env_id);
        assert!(manager.is_mounted(&mountpoint));

        manager.unregister_mount(&mountpoint).unwrap();
        assert!(!manager.is_mounted(&mountpoint));
    }

    #[test]
    fn double_mount_errors() {
        let dir = tempfile::TempDir::new().unwrap();
        let mut manager = MountManager::new(dir.path()).unwrap();
        let env_id = ContentHash::from_bytes(b"test-env");

        manager.register_mount(env_id).unwrap();
        assert!(manager.register_mount(env_id).is_err());
    }

    #[test]
    fn active_mounts_filtering() {
        let dir = tempfile::TempDir::new().unwrap();
        let mut manager = MountManager::new(dir.path()).unwrap();

        let e1 = ContentHash::from_bytes(b"env1");
        let e2 = ContentHash::from_bytes(b"env2");
        manager.register_mount(e1).unwrap();
        manager.register_mount(e2).unwrap();
        assert_eq!(manager.active_mounts().len(), 2);

        let mp1 = manager.mountpoint_for(&e1);
        manager.unregister_mount(&mp1).unwrap();
        assert_eq!(manager.active_mounts().len(), 1);
    }
}

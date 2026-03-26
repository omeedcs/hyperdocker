//! FUSE filesystem adapter. Bridges fuser::Filesystem to ProjectedFs.
//!
//! This module is not tested with inline tests because FUSE operations
//! require root privileges and a mounted filesystem. The core logic
//! is tested via ProjectedFs in projected.rs.

use std::ffi::OsStr;
use std::time::{Duration, UNIX_EPOCH};

use fuser::{
    FileAttr, FileType, Filesystem, ReplyAttr, ReplyData, ReplyDirectory, ReplyEntry,
    Request,
};

use crate::projected::ProjectedFs;

const TTL: Duration = Duration::from_secs(1);

/// FUSE filesystem backed by ProjectedFs.
pub struct FuseFs {
    fs: ProjectedFs,
    // inode -> path mapping for FUSE (FUSE uses inodes, not paths)
    inodes: Vec<String>, // index = inode number (1-based, 0 unused)
}

impl FuseFs {
    pub fn new(fs: ProjectedFs) -> Self {
        // Build inode table from the DAG
        let mut inodes = vec![String::new()]; // inode 0 unused
        inodes.push(String::new()); // inode 1 = root ("")

        // Walk the DAG to assign inodes
        if let Ok(root_entries) = fs.list_dir("") {
            for entry in &root_entries {
                inodes.push(entry.clone());
                // If it's a directory, add its children
                if let Ok(children) = fs.list_dir(entry) {
                    for child in &children {
                        inodes.push(format!("{}/{}", entry, child));
                    }
                }
            }
        }

        FuseFs { fs, inodes }
    }

    fn path_to_inode(&self, path: &str) -> Option<u64> {
        self.inodes.iter().position(|p| p == path).map(|i| i as u64)
    }

    fn inode_to_path(&self, inode: u64) -> Option<&str> {
        self.inodes.get(inode as usize).map(|s| s.as_str())
    }

    fn make_attr(&self, inode: u64, path: &str) -> FileAttr {
        let is_dir = self.fs.list_dir(path).is_ok();
        let size = if is_dir {
            0
        } else {
            self.fs.read_file(path).map(|d| d.len() as u64).unwrap_or(0)
        };

        FileAttr {
            ino: inode,
            size,
            blocks: (size + 511) / 512,
            atime: UNIX_EPOCH,
            mtime: UNIX_EPOCH,
            ctime: UNIX_EPOCH,
            crtime: UNIX_EPOCH,
            kind: if is_dir { FileType::Directory } else { FileType::RegularFile },
            perm: if is_dir { 0o755 } else { 0o644 },
            nlink: 1,
            uid: 0,
            gid: 0,
            rdev: 0,
            blksize: 512,
            flags: 0,
        }
    }
}

impl Filesystem for FuseFs {
    fn getattr(&mut self, _req: &Request, ino: u64, _fh: Option<u64>, reply: ReplyAttr) {
        if let Some(path) = self.inode_to_path(ino) {
            let path = path.to_string();
            if ino == 1 || self.fs.exists(&path) {
                reply.attr(&TTL, &self.make_attr(ino, &path));
            } else {
                reply.error(libc::ENOENT);
            }
        } else {
            reply.error(libc::ENOENT);
        }
    }

    fn lookup(&mut self, _req: &Request, parent: u64, name: &OsStr, reply: ReplyEntry) {
        let parent_path = match self.inode_to_path(parent) {
            Some(p) => p.to_string(),
            None => { reply.error(libc::ENOENT); return; }
        };

        let child_path = if parent_path.is_empty() {
            name.to_string_lossy().to_string()
        } else {
            format!("{}/{}", parent_path, name.to_string_lossy())
        };

        if self.fs.exists(&child_path) {
            // Ensure inode exists
            let inode = if let Some(ino) = self.path_to_inode(&child_path) {
                ino
            } else {
                self.inodes.push(child_path.clone());
                (self.inodes.len() - 1) as u64
            };
            reply.entry(&TTL, &self.make_attr(inode, &child_path), 0);
        } else {
            reply.error(libc::ENOENT);
        }
    }

    fn readdir(
        &mut self,
        _req: &Request,
        ino: u64,
        _fh: u64,
        offset: i64,
        mut reply: ReplyDirectory,
    ) {
        let path = match self.inode_to_path(ino) {
            Some(p) => p.to_string(),
            None => { reply.error(libc::ENOENT); return; }
        };

        let entries = match self.fs.list_dir(&path) {
            Ok(e) => e,
            Err(_) => { reply.error(libc::ENOENT); return; }
        };

        let mut full_entries = vec![
            (ino, FileType::Directory, ".".to_string()),
            (ino, FileType::Directory, "..".to_string()),
        ];

        for entry_name in entries {
            let child_path = if path.is_empty() {
                entry_name.clone()
            } else {
                format!("{}/{}", path, entry_name)
            };

            let inode = if let Some(ino) = self.path_to_inode(&child_path) {
                ino
            } else {
                self.inodes.push(child_path.clone());
                (self.inodes.len() - 1) as u64
            };

            let kind = if self.fs.list_dir(&child_path).is_ok() {
                FileType::Directory
            } else {
                FileType::RegularFile
            };

            full_entries.push((inode, kind, entry_name));
        }

        for (i, (inode, kind, name)) in full_entries.iter().enumerate().skip(offset as usize) {
            if reply.add(*inode, (i + 1) as i64, *kind, name) {
                break;
            }
        }
        reply.ok();
    }

    fn read(
        &mut self,
        _req: &Request,
        ino: u64,
        _fh: u64,
        offset: i64,
        size: u32,
        _flags: i32,
        _lock_owner: Option<u64>,
        reply: ReplyData,
    ) {
        let path = match self.inode_to_path(ino) {
            Some(p) => p.to_string(),
            None => { reply.error(libc::ENOENT); return; }
        };

        match self.fs.read_file(&path) {
            Ok(data) => {
                let start = offset as usize;
                if start >= data.len() {
                    reply.data(&[]);
                } else {
                    let end = (start + size as usize).min(data.len());
                    reply.data(&data[start..end]);
                }
            }
            Err(_) => reply.error(libc::ENOENT),
        }
    }
}

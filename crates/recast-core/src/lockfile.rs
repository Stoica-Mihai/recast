//! Advisory workspace lock guarding concurrent `--apply` invocations
//! against the same tree.
//!
//! Two `recast --apply` processes touching overlapping paths would
//! interleave their rename / backup steps unpredictably. The lock
//! is purely advisory (other tools won't see it), but every recast
//! `--apply` checks it, so the common case (two agents on the same
//! repo) is caught immediately with a clear error instead of leaving
//! the tree in a partial state.

use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};

use fs2::FileExt;

use crate::error::{Error, IoCtx, Result};

/// RAII guard around an exclusively-locked lockfile. Drop to release.
#[derive(Debug)]
#[must_use = "lock is released as soon as the guard is dropped"]
pub struct WorkspaceLock {
    file: File,
    path: PathBuf,
}

impl WorkspaceLock {
    /// Path to the lockfile this guard is holding.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for WorkspaceLock {
    fn drop(&mut self) {
        let _ = self.file.unlock();
    }
}

/// Try to take an exclusive non-blocking lock on `lock_path`. Returns
/// [`Error::Locked`] immediately if another process already holds it.
pub fn acquire_workspace_lock(lock_path: &Path) -> Result<WorkspaceLock> {
    if let Some(parent) = lock_path.parent() {
        std::fs::create_dir_all(parent).io_ctx(lock_path)?;
    }
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(lock_path)
        .io_ctx(lock_path)?;

    // try_lock_exclusive surfaces WouldBlock when the lock is held by
    // another process; every other io::Error (EPERM, ENOSPC, EIO, …)
    // is a real failure that should propagate as Error::Io instead of
    // being misclassified as "another recast is already applying".
    if let Err(e) = file.try_lock_exclusive() {
        return match e.kind() {
            std::io::ErrorKind::WouldBlock => Err(Error::Locked { path: lock_path.to_path_buf() }),
            _ => Err(Error::Io { path: lock_path.to_path_buf(), source: e }),
        };
    }
    Ok(WorkspaceLock { file, path: lock_path.to_path_buf() })
}

#[cfg(test)]
#[path = "lockfile_tests.rs"]
mod tests;

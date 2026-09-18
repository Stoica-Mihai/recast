//! Advisory workspace lock guarding concurrent `--apply` invocations
//! against the same tree.
//!
//! Two `recast --apply` processes touching overlapping paths would
//! interleave their rename / backup steps unpredictably. The lock
//! is purely advisory (other tools won't see it), but every recast
//! `--apply` checks it, so the common case (two agents on the same
//! repo) is caught immediately with a clear error instead of leaving
//! the tree in a partial state.
//!
//! The lock is keyed on the *workspace root* — the enclosing VCS
//! checkout — not on the paths a given invocation happens to name, so
//! `recast --apply … src/` and `recast --apply … src/sub/` contend with
//! each other. The lockfile itself lives outside the working tree, in
//! the user's runtime directory, so a rewrite never leaves an untracked
//! file behind.
//!
//! The file is deliberately never unlinked. Unlinking on release breaks
//! mutual exclusion: a process holding a descriptor on the old inode and
//! a process creating a fresh file at the same path lock two different
//! objects, and both succeed.

use std::fs::{File, OpenOptions};
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

use fs2::FileExt;

use crate::error::{Error, IoCtx, Result};

/// Directory entries that mark the root of a checkout.
const VCS_MARKERS: [&str; 4] = [".git", ".hg", ".jj", ".svn"];

/// RAII guard around an exclusively-locked lockfile. Drop to release.
#[derive(Debug)]
#[must_use = "lock is released as soon as the guard is dropped"]
pub struct WorkspaceLock {
    file: File,
    path: PathBuf,
    root: PathBuf,
}

impl WorkspaceLock {
    /// Path to the lockfile this guard is holding.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Workspace root the lock covers.
    pub fn root(&self) -> &Path {
        &self.root
    }
}

impl Drop for WorkspaceLock {
    fn drop(&mut self) {
        let _ = self.file.unlock();
    }
}

/// Deepest path prefix shared by every element of `paths`. Returns
/// `None` only if the slice is empty. For absolute paths the result is
/// at worst `"/"`; for purely-relative paths from the same CWD it can
/// degenerate to the empty path.
fn common_ancestor(paths: &[PathBuf]) -> Option<PathBuf> {
    let mut iter = paths.iter();
    let first = iter.next()?;
    let mut common: Vec<&std::ffi::OsStr> = first.iter().collect();
    for p in iter {
        let shared = common.iter().zip(p.iter()).take_while(|(a, b)| **a == *b).count();
        common.truncate(shared);
        if common.is_empty() {
            return None;
        }
    }
    let mut buf = PathBuf::new();
    for c in common {
        buf.push(c);
    }
    Some(buf)
}

/// Workspace root covering `paths`: canonicalize each one, take their
/// deepest common ancestor, then walk up to the nearest enclosing VCS
/// checkout. Falls back to the common ancestor itself when no marker is
/// found, which means an un-versioned tree still keys per-subtree — see
/// `docs/src/safety.md`.
pub fn workspace_root(paths: &[PathBuf]) -> PathBuf {
    let canonical: Vec<PathBuf> =
        paths.iter().map(|p| std::fs::canonicalize(p).unwrap_or_else(|_| p.clone())).collect();
    let start = common_ancestor(&canonical).unwrap_or_else(|| PathBuf::from("."));
    let start = if start.is_file() {
        start.parent().map(Path::to_path_buf).unwrap_or_else(|| PathBuf::from("/"))
    } else {
        start
    };

    let mut cursor = start.as_path();
    loop {
        if VCS_MARKERS.iter().any(|m| cursor.join(m).exists()) {
            return cursor.to_path_buf();
        }
        match cursor.parent() {
            Some(parent) => cursor = parent,
            None => return start,
        }
    }
}

/// FNV-1a over the raw path bytes.
///
/// Hand-rolled rather than `DefaultHasher` because std explicitly does
/// not guarantee that hasher's output across releases, and two recast
/// builds must agree on the lockfile name or they miss each other.
fn stable_hash(bytes: &[u8]) -> u64 {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;
    let mut h = OFFSET;
    for b in bytes {
        h ^= u64::from(*b);
        h = h.wrapping_mul(PRIME);
    }
    h
}

/// Directory holding recast's lockfiles. `$XDG_RUNTIME_DIR` when set —
/// it is per-user and mode 0700 — otherwise the system temp directory.
pub fn lock_dir() -> PathBuf {
    match std::env::var_os("XDG_RUNTIME_DIR") {
        Some(v) if !v.is_empty() => PathBuf::from(v).join("recast"),
        _ => std::env::temp_dir().join("recast"),
    }
}

/// Lockfile path for `root`. Outside the working tree, and stable for a
/// given root across processes and builds.
pub fn workspace_lock_path(root: &Path) -> PathBuf {
    lock_dir().join(format!("{:016x}.lock", stable_hash(root.as_os_str().as_bytes())))
}

/// Take an exclusive non-blocking lock covering the workspace `root`.
/// Returns [`Error::Locked`] immediately if another process holds it.
pub fn acquire_workspace_lock(root: &Path) -> Result<WorkspaceLock> {
    let lock_path = workspace_lock_path(root);
    if let Some(parent) = lock_path.parent() {
        std::fs::create_dir_all(parent).io_ctx(parent)?;
    }
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&lock_path)
        .io_ctx(&lock_path)?;

    // try_lock_exclusive surfaces WouldBlock when the lock is held by
    // another process; every other io::Error (EPERM, ENOSPC, EIO, …)
    // is a real failure that should propagate as Error::Io instead of
    // being misclassified as "another recast is already applying".
    if let Err(e) = file.try_lock_exclusive() {
        return match e.kind() {
            std::io::ErrorKind::WouldBlock => {
                Err(Error::Locked { path: lock_path, root: root.to_path_buf() })
            }
            _ => Err(Error::Io { path: lock_path, source: e }),
        };
    }
    Ok(WorkspaceLock { file, path: lock_path, root: root.to_path_buf() })
}

/// Resolve `paths` to their workspace root and lock it.
pub fn acquire_workspace_lock_for_paths(paths: &[PathBuf]) -> Result<WorkspaceLock> {
    acquire_workspace_lock(&workspace_root(paths))
}

#[cfg(test)]
#[path = "lockfile_tests.rs"]
mod tests;

#![allow(clippy::unwrap_used)]

use super::*;
use tempfile::TempDir;

#[test]
fn exclusive_lock_blocks_second_acquire() {
    let dir = TempDir::new().unwrap();
    let lock_path = dir.path().join(".recast.lock");

    let _first = acquire_workspace_lock(&lock_path).unwrap();
    let second = acquire_workspace_lock(&lock_path).unwrap_err();
    assert!(matches!(second, Error::Locked { .. }));
}

#[test]
fn lock_released_when_guard_dropped() {
    let dir = TempDir::new().unwrap();
    let lock_path = dir.path().join(".recast.lock");

    {
        let _guard = acquire_workspace_lock(&lock_path).unwrap();
    }
    let again = acquire_workspace_lock(&lock_path).unwrap();
    drop(again);
}

#[test]
fn lock_creates_lockfile_in_parent_dir() {
    let dir = TempDir::new().unwrap();
    let lock_path = dir.path().join(".recast.lock");
    let _guard = acquire_workspace_lock(&lock_path).unwrap();
    assert!(lock_path.exists());
}

//! Concurrent-apply safety: a second `recast --apply` against a tree
//! whose workspace lock is already held must exit non-zero with the
//! `Locked` error variant, not silently proceed and clobber the first
//! invocation's commit phase.

#![allow(clippy::unwrap_used)]

use std::fs::{self, OpenOptions};

use assert_cmd::Command;
use fs2::FileExt;
use tempfile::TempDir;

#[test]
fn second_apply_blocks_when_workspace_lock_held() {
    let dir = TempDir::new().unwrap();
    let target = dir.path().join("a.txt");
    fs::write(&target, "old\n").unwrap();

    // Hold the lock the binary will try to acquire. Using `fs2`
    // directly rather than spawning a long-running `recast --apply`
    // makes the race deterministic.
    let lock_path = dir.path().join(".recast.lock");
    let lock_file =
        OpenOptions::new().write(true).create(true).truncate(false).open(&lock_path).unwrap();
    FileExt::try_lock_exclusive(&lock_file).unwrap();

    let assert = Command::cargo_bin("recast")
        .unwrap()
        .arg("--apply")
        .arg("old")
        .arg("new")
        .arg(dir.path())
        .assert()
        .failure();

    let stderr = String::from_utf8_lossy(&assert.get_output().stderr).into_owned();
    assert!(
        stderr.contains("already applying") || stderr.contains("lockfile"),
        "stderr did not surface lock contention: {stderr}"
    );

    assert_eq!(fs::read_to_string(&target).unwrap(), "old\n", "target was modified despite lock");
}

#[test]
fn force_flag_bypasses_workspace_lock() {
    let dir = TempDir::new().unwrap();
    let target = dir.path().join("a.txt");
    fs::write(&target, "old\n").unwrap();

    let lock_path = dir.path().join(".recast.lock");
    let lock_file =
        OpenOptions::new().write(true).create(true).truncate(false).open(&lock_path).unwrap();
    FileExt::try_lock_exclusive(&lock_file).unwrap();

    Command::cargo_bin("recast")
        .unwrap()
        .arg("--force")
        .arg("--apply")
        .arg("old")
        .arg("new")
        .arg(dir.path())
        .assert()
        .success();

    assert_eq!(fs::read_to_string(&target).unwrap(), "new\n");
}

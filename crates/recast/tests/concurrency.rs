//! Concurrent-apply safety: a second `recast --apply` against a tree
//! whose workspace lock is already held must exit non-zero with the
//! `Locked` error variant, not silently proceed and clobber the first
//! invocation's commit phase.

#![allow(clippy::unwrap_used)]

use std::fs::{self, OpenOptions};
use std::path::Path;

use assert_cmd::Command;
use fs2::FileExt;
use recast_core::{workspace_lock_path, workspace_root};
use tempfile::TempDir;

/// A temp tree with a `.git` marker, so the workspace root resolves to
/// the tree itself rather than to whatever encloses `TMPDIR`.
fn repo() -> TempDir {
    let dir = TempDir::new().unwrap();
    fs::create_dir_all(dir.path().join(".git")).unwrap();
    dir
}

/// Hold the exclusive lock the binary will try to take. Using `fs2`
/// directly rather than spawning a long-running `recast --apply` makes
/// the race deterministic.
fn hold_lock_for(tree: &Path) -> std::fs::File {
    let lock_path = workspace_lock_path(&workspace_root(&[tree.to_path_buf()]));
    fs::create_dir_all(lock_path.parent().unwrap()).unwrap();
    let file =
        OpenOptions::new().write(true).create(true).truncate(false).open(&lock_path).unwrap();
    FileExt::try_lock_exclusive(&file).unwrap();
    file
}

#[test]
fn second_apply_blocks_when_workspace_lock_held() {
    let dir = repo();
    let target = dir.path().join("a.txt");
    fs::write(&target, "old\n").unwrap();

    let _held = hold_lock_for(dir.path());

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
fn lock_held_on_repo_root_blocks_an_apply_naming_only_a_subtree() {
    let dir = repo();
    let sub = dir.path().join("src/sub");
    fs::create_dir_all(&sub).unwrap();
    let target = sub.join("b.txt");
    fs::write(&target, "old\n").unwrap();

    let _held = hold_lock_for(dir.path());

    Command::cargo_bin("recast")
        .unwrap()
        .arg("--apply")
        .arg("old")
        .arg("new")
        .arg(&sub)
        .assert()
        .failure();

    assert_eq!(
        fs::read_to_string(&target).unwrap(),
        "old\n",
        "subtree was written despite a lock on the repo root"
    );
}

#[test]
fn apply_leaves_no_lockfile_inside_the_tree() {
    let dir = repo();
    let sub = dir.path().join("src/sub");
    fs::create_dir_all(&sub).unwrap();
    fs::write(sub.join("b.txt"), "old\n").unwrap();

    Command::cargo_bin("recast")
        .unwrap()
        .arg("--apply")
        .arg("old")
        .arg("new")
        .arg(&sub)
        .assert()
        .success();

    let strays: Vec<_> =
        walk(dir.path()).into_iter().filter(|p| p.ends_with(".recast.lock")).collect();
    assert!(strays.is_empty(), "lockfiles left in the tree: {strays:?}");
}

fn walk(root: &Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                stack.push(path);
            } else {
                out.push(path);
            }
        }
    }
    out
}

#[test]
fn force_flag_bypasses_workspace_lock() {
    let dir = repo();
    let target = dir.path().join("a.txt");
    fs::write(&target, "old\n").unwrap();

    let _held = hold_lock_for(dir.path());

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

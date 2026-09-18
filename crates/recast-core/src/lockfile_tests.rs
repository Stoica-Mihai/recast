#![allow(clippy::unwrap_used)]

use std::fs;

use super::*;
use tempfile::TempDir;

/// A temp tree with a `.git` marker at its top, so `workspace_root`
/// stops there instead of walking into whatever encloses `TMPDIR`.
fn repo() -> TempDir {
    let dir = TempDir::new().unwrap();
    fs::create_dir_all(dir.path().join(".git")).unwrap();
    dir
}

#[test]
fn exclusive_lock_blocks_second_acquire() {
    let dir = repo();
    let _first = acquire_workspace_lock(dir.path()).unwrap();
    let second = acquire_workspace_lock(dir.path()).unwrap_err();
    assert!(matches!(second, Error::Locked { .. }));
}

#[test]
fn lock_released_when_guard_dropped() {
    let dir = repo();
    {
        let _guard = acquire_workspace_lock(dir.path()).unwrap();
    }
    let again = acquire_workspace_lock(dir.path()).unwrap();
    drop(again);
}

#[test]
fn lock_file_is_never_created_inside_the_workspace() {
    let dir = repo();
    let guard = acquire_workspace_lock(dir.path()).unwrap();
    assert!(guard.path().exists());
    assert!(
        !guard.path().starts_with(dir.path()),
        "lockfile landed inside the tree: {}",
        guard.path().display()
    );
    assert!(!dir.path().join(".recast.lock").exists());
}

#[test]
fn workspace_root_walks_up_to_the_vcs_marker() {
    let dir = repo();
    let deep = dir.path().join("src/sub");
    fs::create_dir_all(&deep).unwrap();
    assert_eq!(workspace_root(&[deep]), fs::canonicalize(dir.path()).unwrap());
}

#[test]
fn a_git_file_worktree_marker_is_detected() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join(".git"), "gitdir: /elsewhere\n").unwrap();
    let deep = dir.path().join("src");
    fs::create_dir_all(&deep).unwrap();
    assert_eq!(workspace_root(&[deep]), fs::canonicalize(dir.path()).unwrap());
}

#[test]
fn a_subtree_and_its_parent_resolve_to_one_root() {
    let dir = repo();
    let src = dir.path().join("src");
    let sub = src.join("sub");
    fs::create_dir_all(&sub).unwrap();
    assert_eq!(workspace_root(&[src]), workspace_root(&[sub]));
}

#[test]
fn a_subtree_contends_with_a_lock_held_on_the_repo_root() {
    let dir = repo();
    let sub = dir.path().join("src/sub");
    fs::create_dir_all(&sub).unwrap();

    let _held = acquire_workspace_lock_for_paths(&[dir.path().to_path_buf()]).unwrap();
    let blocked = acquire_workspace_lock_for_paths(&[sub]).unwrap_err();
    assert!(matches!(blocked, Error::Locked { .. }), "{blocked:?}");
}

#[test]
fn workspace_root_falls_back_to_the_common_ancestor_without_a_marker() {
    let dir = TempDir::new().unwrap();
    let a = dir.path().join("a");
    let b = dir.path().join("b");
    fs::create_dir_all(&a).unwrap();
    fs::create_dir_all(&b).unwrap();
    assert_eq!(workspace_root(&[a, b]), fs::canonicalize(dir.path()).unwrap());
}

/// Known limitation, pinned so a future change to it is deliberate.
/// Paths that do not exist keep the walk off the ambient filesystem, so
/// the result does not depend on what encloses `TMPDIR`.
#[test]
fn an_unversioned_tree_still_keys_per_subtree() {
    let src = PathBuf::from("/recast-no-such-tree/src");
    let sub = PathBuf::from("/recast-no-such-tree/src/sub");
    assert_ne!(workspace_root(&[src]), workspace_root(&[sub]));
}

#[test]
fn lock_path_is_stable_for_one_root_and_distinct_across_roots() {
    let a = repo();
    let b = repo();
    assert_eq!(workspace_lock_path(a.path()), workspace_lock_path(a.path()));
    assert_ne!(workspace_lock_path(a.path()), workspace_lock_path(b.path()));
    assert!(workspace_lock_path(a.path()).starts_with(lock_dir()));
}

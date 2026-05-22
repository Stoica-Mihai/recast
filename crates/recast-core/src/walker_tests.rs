#![allow(clippy::unwrap_used)]

use std::fs;

use tempfile::TempDir;

use super::*;

#[test]
fn walker_collects_files_and_skips_directories() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    fs::write(root.join("a.txt"), b"hello").unwrap();
    fs::create_dir(root.join("nested")).unwrap();
    fs::write(root.join("nested/b.txt"), b"hello").unwrap();
    let files = walk_paths(&[root], &WalkOptions::default()).unwrap();
    assert_eq!(files.len(), 2);
    assert!(files.iter().any(|p| p.ends_with("a.txt")));
    assert!(files.iter().any(|p| p.ends_with("b.txt")));
}

#[test]
fn walker_respects_gitignore_by_default() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    fs::write(root.join(".gitignore"), b"ignored.txt\n").unwrap();
    fs::write(root.join("ignored.txt"), b"x").unwrap();
    fs::write(root.join("kept.txt"), b"x").unwrap();
    let files = walk_paths(&[root], &WalkOptions::default()).unwrap();
    assert!(files.iter().any(|p| p.ends_with("kept.txt")));
    assert!(!files.iter().any(|p| p.ends_with("ignored.txt")));
}

#[test]
fn walker_no_ignore_lists_gitignored_files() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    fs::write(root.join(".gitignore"), b"ignored.txt\n").unwrap();
    fs::write(root.join("ignored.txt"), b"x").unwrap();
    let opts = WalkOptions { no_ignore: true, ..Default::default() };
    let files = walk_paths(&[root], &opts).unwrap();
    assert!(files.iter().any(|p| p.ends_with("ignored.txt")));
}

#[test]
fn walker_type_select_keeps_only_matching_lang() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    fs::write(root.join("a.rs"), b"x").unwrap();
    fs::write(root.join("b.md"), b"x").unwrap();
    fs::write(root.join("c.toml"), b"x").unwrap();
    let opts = WalkOptions { types: vec!["rust".into()], ..Default::default() };
    let files = walk_paths(&[root], &opts).unwrap();
    assert!(files.iter().any(|p| p.ends_with("a.rs")));
    assert!(!files.iter().any(|p| p.ends_with("b.md")));
    assert!(!files.iter().any(|p| p.ends_with("c.toml")));
}

#[test]
fn walker_type_negate_excludes_lang() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    fs::write(root.join("a.rs"), b"x").unwrap();
    fs::write(root.join("b.md"), b"x").unwrap();
    let opts = WalkOptions { types_not: vec!["markdown".into()], ..Default::default() };
    let files = walk_paths(&[root], &opts).unwrap();
    assert!(files.iter().any(|p| p.ends_with("a.rs")));
    assert!(!files.iter().any(|p| p.ends_with("b.md")));
}

#[test]
fn walker_glob_include_only_rust_files() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    fs::write(root.join("a.rs"), b"x").unwrap();
    fs::write(root.join("b.md"), b"x").unwrap();
    let opts = WalkOptions { globs: vec!["*.rs".into()], ..Default::default() };
    let files = walk_paths(&[root], &opts).unwrap();
    assert!(files.iter().any(|p| p.ends_with("a.rs")));
    assert!(!files.iter().any(|p| p.ends_with("b.md")));
}

#[test]
fn walker_skips_symlinks_by_default() {
    use std::os::unix::fs::symlink;

    let dir = TempDir::new().unwrap();
    let root = dir.path();
    fs::write(root.join("real.txt"), b"x").unwrap();
    symlink(root.join("real.txt"), root.join("link.txt")).unwrap();
    let files = walk_paths(&[root], &WalkOptions::default()).unwrap();
    assert!(files.iter().any(|p| p.ends_with("real.txt")));
    assert!(!files.iter().any(|p| p.ends_with("link.txt")));
}

#[test]
fn walker_follow_symlinks_includes_targets() {
    use std::os::unix::fs::symlink;

    let dir = TempDir::new().unwrap();
    let root = dir.path();
    fs::write(root.join("real.txt"), b"x").unwrap();
    symlink(root.join("real.txt"), root.join("link.txt")).unwrap();
    let opts = WalkOptions { follow_symlinks: true, ..Default::default() };
    let files = walk_paths(&[root], &opts).unwrap();
    assert!(files.iter().any(|p| p.ends_with("link.txt")));
}

#[test]
fn walker_does_not_loop_on_symlink_cycle_when_following() {
    // Symlink cycle: link_a → link_b, link_b → link_a. With
    // follow_symlinks=true the walker must not recurse forever; the
    // ignore crate's WalkBuilder tracks visited directories and
    // breaks cycles via its built-in loop detection.
    use std::os::unix::fs::symlink;

    let dir = TempDir::new().unwrap();
    let root = dir.path();
    fs::write(root.join("real.txt"), b"x").unwrap();
    symlink(root.join("link_b"), root.join("link_a")).unwrap();
    symlink(root.join("link_a"), root.join("link_b")).unwrap();
    let opts = WalkOptions { follow_symlinks: true, ..Default::default() };
    // The walk completes (no infinite loop). Result may carry a Walk
    // error for the cycle, which we want surfaced rather than swallowed
    // — callers can decide whether a cycle is fatal.
    let _ = walk_paths(&[root], &opts);
}

#[test]
fn walker_handles_broken_symlink_without_panic() {
    // Dangling symlink: target doesn't exist. The walker must yield a
    // walk error (or skip silently) but never panic.
    use std::os::unix::fs::symlink;

    let dir = TempDir::new().unwrap();
    let root = dir.path();
    fs::write(root.join("real.txt"), b"x").unwrap();
    symlink(root.join("does_not_exist"), root.join("dangling")).unwrap();
    let opts = WalkOptions { follow_symlinks: true, ..Default::default() };
    let _ = walk_paths(&[root], &opts);
    // Default (no-follow) walk must succeed and skip the dangling link.
    let files = walk_paths(&[root], &WalkOptions::default()).unwrap();
    assert!(files.iter().any(|p| p.ends_with("real.txt")));
    assert!(!files.iter().any(|p| p.ends_with("dangling")));
}

#[test]
fn walker_follows_symlinks_to_outside_root_when_enabled() {
    // Symlink whose target lives outside the walker root. With
    // follow_symlinks=true the linked file is included (ignore's
    // default behavior). With follow_symlinks=false the link is
    // skipped, so nothing outside the root leaks in.
    use std::os::unix::fs::symlink;

    let outside = TempDir::new().unwrap();
    fs::write(outside.path().join("secret.txt"), b"hi").unwrap();

    let dir = TempDir::new().unwrap();
    let root = dir.path();
    fs::write(root.join("inside.txt"), b"x").unwrap();
    symlink(outside.path().join("secret.txt"), root.join("link.txt")).unwrap();

    let no_follow = walk_paths(&[root], &WalkOptions::default()).unwrap();
    assert!(!no_follow.iter().any(|p| p.ends_with("link.txt")));

    let follow =
        walk_paths(&[root], &WalkOptions { follow_symlinks: true, ..Default::default() }).unwrap();
    assert!(follow.iter().any(|p| p.ends_with("link.txt")));
}

#[test]
fn walker_follow_symlinks_still_honors_gitignore() {
    // Following symlinks must not bypass .gitignore filtering: an
    // ignored target should remain hidden whether reached directly or
    // via a symlink.
    use std::os::unix::fs::symlink;

    let dir = TempDir::new().unwrap();
    let root = dir.path();
    fs::write(root.join(".gitignore"), b"ignored.txt\n").unwrap();
    fs::write(root.join("ignored.txt"), b"x").unwrap();
    symlink(root.join("ignored.txt"), root.join("link.txt")).unwrap();
    let opts = WalkOptions { follow_symlinks: true, ..Default::default() };
    let files = walk_paths(&[root], &opts).unwrap();
    assert!(!files.iter().any(|p| p.ends_with("ignored.txt")));
}

#[test]
fn walker_glob_negate_excludes_vendor() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    fs::create_dir_all(root.join("src")).unwrap();
    fs::create_dir_all(root.join("vendor")).unwrap();
    fs::write(root.join("src/a.rs"), b"x").unwrap();
    fs::write(root.join("vendor/b.rs"), b"x").unwrap();
    let opts = WalkOptions { globs: vec!["!vendor/**".into()], ..Default::default() };
    let files = walk_paths(&[root], &opts).unwrap();
    assert!(files.iter().any(|p| p.ends_with("src/a.rs")));
    assert!(!files.iter().any(|p| p.ends_with("vendor/b.rs")));
}

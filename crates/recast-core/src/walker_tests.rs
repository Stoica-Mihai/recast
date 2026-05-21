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

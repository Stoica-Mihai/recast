use std::path::{Path, PathBuf};

use ignore::WalkBuilder;
use ignore::overrides::OverrideBuilder;
use ignore::types::TypesBuilder;

use crate::error::Result;

#[derive(Debug, Clone, Default)]
pub struct WalkOptions {
    pub hidden: bool,
    pub no_ignore: bool,
    pub follow_symlinks: bool,
    pub types: Vec<String>,
    pub types_not: Vec<String>,
    pub globs: Vec<String>,
}

pub fn walk_paths<P: AsRef<Path>>(roots: &[P], opts: &WalkOptions) -> Result<Vec<PathBuf>> {
    let mut iter = if let Some(first) = roots.first() {
        WalkBuilder::new(first.as_ref())
    } else {
        WalkBuilder::new(".")
    };
    for extra in roots.iter().skip(1) {
        iter.add(extra.as_ref());
    }
    iter.hidden(!opts.hidden)
        .ignore(!opts.no_ignore)
        .git_ignore(!opts.no_ignore)
        .git_global(!opts.no_ignore)
        .git_exclude(!opts.no_ignore)
        .require_git(false)
        .parents(!opts.no_ignore)
        .follow_links(opts.follow_symlinks);

    if !opts.types.is_empty() || !opts.types_not.is_empty() {
        let mut tb = TypesBuilder::new();
        tb.add_defaults();
        for t in &opts.types {
            tb.select(t);
        }
        for t in &opts.types_not {
            tb.negate(t);
        }
        iter.types(tb.build()?);
    }

    if !opts.globs.is_empty() {
        let glob_root = roots.first().map(|p| p.as_ref()).unwrap_or_else(|| Path::new("."));
        let mut ob = OverrideBuilder::new(glob_root);
        for g in &opts.globs {
            ob.add(g)?;
        }
        iter.overrides(ob.build()?);
    }

    let mut out = Vec::new();
    for entry in iter.build() {
        let entry = entry?;
        let ft = match entry.file_type() {
            Some(ft) => ft,
            None => continue,
        };
        if !ft.is_file() {
            continue;
        }
        out.push(entry.into_path());
    }
    out.sort();
    Ok(out)
}

#[cfg(test)]
mod tests {
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
}

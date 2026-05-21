use std::path::{Path, PathBuf};

use ignore::WalkBuilder;

use crate::error::Result;

#[derive(Debug, Clone, Default)]
pub struct WalkOptions {
    pub hidden: bool,
    pub no_ignore: bool,
    pub follow_symlinks: bool,
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
}

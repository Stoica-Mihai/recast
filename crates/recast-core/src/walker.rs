//! File-system enumeration honoring `.gitignore`, `--type`, and `-g`.
//!
//! Thin wrapper around the `ignore` crate's `WalkBuilder` with the
//! ripgrep-equivalent defaults (`.gitignore` respected, hidden files
//! excluded, symlinks not followed). Globs use the override engine so
//! `!pattern` works as a per-invocation exclude.

use std::path::{Path, PathBuf};

use ignore::WalkBuilder;
use ignore::overrides::OverrideBuilder;
use ignore::types::TypesBuilder;

use crate::error::Result;

/// Filters applied while walking `roots`.
///
/// `types` and `types_not` use the same shorthand vocabulary as ripgrep
/// (`rust`, `js`, `py`, `markdown`, …). `globs` accept ripgrep-style
/// include/exclude patterns: `"!vendor/**"` excludes, anything else
/// includes. By default `.gitignore` is honored, hidden files are
/// excluded, and symlinks are not followed.
#[derive(Debug, Clone, Default)]
pub struct WalkOptions {
    pub hidden: bool,
    pub no_ignore: bool,
    pub follow_symlinks: bool,
    pub types: Vec<String>,
    pub types_not: Vec<String>,
    pub globs: Vec<String>,
}

/// Enumerate every regular file under `roots` (sorted, dedup'd by the
/// ignore crate) honoring `opts`. Directories, symlinks (unless
/// `follow_symlinks` is set), and anything filtered out by ignore /
/// globs / types are skipped.
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
#[path = "walker_tests.rs"]
mod tests;

//! File-system enumeration honoring `.gitignore`, `--type`, and `-g`.
//!
//! Thin wrapper around the `ignore` crate's `WalkBuilder` with the
//! ripgrep-equivalent defaults (`.gitignore` respected, hidden files
//! excluded, symlinks not followed). Globs use the override engine so
//! `!pattern` works as a per-invocation exclude.
//!
//! ## Symlink semantics
//!
//! `follow_symlinks=false` (default): symlinks are skipped entirely —
//! neither the link entry nor its target are visited. Safe by default
//! for `--apply`: a malicious or accidental link can't redirect a
//! rewrite onto a file outside the user's chosen root.
//!
//! `follow_symlinks=true`: the walker resolves links and visits their
//! targets (including targets outside the walker root), but still
//! honors `.gitignore` along the way and breaks cycles via the
//! `ignore` crate's built-in loop detection. Dangling links surface
//! as walk errors rather than panicking; cycles abort the walk with a
//! typed error instead of looping forever.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use ignore::overrides::OverrideBuilder;
use ignore::types::TypesBuilder;
use ignore::{WalkBuilder, WalkState};

use crate::error::{Error, Result};

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
///
/// Uses [`ignore::WalkParallel`] so the walk honors the surrounding
/// rayon pool's thread count instead of running single-threaded
/// regardless of `--threads N`. Output is sorted at the end so callers
/// (and snapshot tests) get a deterministic listing.
pub fn walk_paths<P: AsRef<Path>>(roots: &[P], opts: &WalkOptions) -> Result<Vec<PathBuf>> {
    let builder = build_walker(roots, opts)?;
    let collected: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());
    let first_error: Mutex<Option<ignore::Error>> = Mutex::new(None);

    builder.build_parallel().run(|| {
        Box::new(|result| match result {
            Ok(entry) => {
                if matches!(entry.file_type(), Some(ft) if ft.is_file())
                    && let Ok(mut sink) = collected.lock()
                {
                    sink.push(entry.into_path());
                }
                WalkState::Continue
            }
            Err(e) => {
                if let Ok(mut slot) = first_error.lock()
                    && slot.is_none()
                {
                    *slot = Some(e);
                }
                WalkState::Quit
            }
        })
    });

    if let Some(e) = first_error.into_inner().ok().flatten() {
        return Err(Error::Walk(e));
    }
    let mut out = collected.into_inner().unwrap_or_default();
    out.sort();
    Ok(out)
}

/// Build the [`WalkBuilder`] for `roots` with `opts` applied. Pulled
/// out of the public entry point so the parallel walker's setup stays
/// readable; nothing else calls it.
fn build_walker<P: AsRef<Path>>(roots: &[P], opts: &WalkOptions) -> Result<WalkBuilder> {
    let mut builder = if let Some(first) = roots.first() {
        WalkBuilder::new(first.as_ref())
    } else {
        WalkBuilder::new(".")
    };
    for extra in roots.iter().skip(1) {
        builder.add(extra.as_ref());
    }
    builder
        .hidden(!opts.hidden)
        .ignore(!opts.no_ignore)
        .git_ignore(!opts.no_ignore)
        .git_global(!opts.no_ignore)
        .git_exclude(!opts.no_ignore)
        .require_git(false)
        .parents(!opts.no_ignore)
        .follow_links(opts.follow_symlinks)
        // Honor the surrounding rayon pool's thread count. Falls back
        // to ignore's own default (num_cpus) outside a rayon scope.
        .threads(rayon::current_num_threads().max(1));

    if !opts.types.is_empty() || !opts.types_not.is_empty() {
        let mut tb = TypesBuilder::new();
        tb.add_defaults();
        for t in &opts.types {
            tb.select(t);
        }
        for t in &opts.types_not {
            tb.negate(t);
        }
        builder.types(tb.build()?);
    }

    if !opts.globs.is_empty() {
        let glob_root = roots.first().map(|p| p.as_ref()).unwrap_or_else(|| Path::new("."));
        let mut ob = OverrideBuilder::new(glob_root);
        for g in &opts.globs {
            ob.add(g)?;
        }
        builder.overrides(ob.build()?);
    }

    Ok(builder)
}

#[cfg(test)]
#[path = "walker_tests.rs"]
mod tests;

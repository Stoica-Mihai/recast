//! End-to-end planning: walk → compile → rewrite → guard → check
//! convergence.
//!
//! [`plan_rewrite`] is the single entry point most callers want. It
//! produces a [`Plan`] describing every file that would change without
//! touching the filesystem; pass the plan to
//! [`crate::apply_changes`] to commit.

use std::fs;
use std::path::{Path, PathBuf};

use rayon::prelude::*;
use tracing::{debug, trace};

use crate::error::{Error, Result};
use crate::pattern::{CompiledPattern, PatternOptions};
use crate::rewrite::{label_for_path, rewrite_text, unified_diff};
use crate::walker::{WalkOptions, walk_paths};

/// Knobs controlling a single [`plan_rewrite`] invocation.
///
/// Defaults are tuned for safety-by-default LLM use: `at_least = Some(1)`
/// makes a silent zero-match impossible, `max_bytes = 10 MiB`, and
/// `max_files = 1000` keep runaway pattern matches in check.
#[derive(Debug, Clone)]
pub struct PlanOptions {
    pub pattern_options: PatternOptions,
    pub walk_options: WalkOptions,
    /// Inclusive lower bound on total matches across all files. `None`
    /// disables the guard; `Some(0)` accepts zero-match runs explicitly.
    pub at_least: Option<usize>,
    /// Inclusive upper bound on total matches. `None` = unbounded.
    pub at_most: Option<usize>,
    /// Skip the convergence (idempotency) check. Off by default — a
    /// pattern like `a` → `aa` is rejected so re-runs cannot accidentally
    /// grow the file.
    pub allow_non_convergent: bool,
    /// Refuse to read any file larger than this many bytes.
    pub max_bytes: u64,
    /// Refuse to plan if the walk turns up more files than this.
    pub max_files: usize,
}

impl Default for PlanOptions {
    fn default() -> Self {
        Self {
            pattern_options: PatternOptions::default(),
            walk_options: WalkOptions::default(),
            at_least: Some(1),
            at_most: None,
            allow_non_convergent: false,
            max_bytes: 10 * 1024 * 1024,
            max_files: 1000,
        }
    }
}

/// One file's worth of pending rewrite work. `before` and `after` are
/// the full pre- and post-images; `diff` is the unified-diff rendering.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct FileChange {
    pub path: PathBuf,
    pub matches: usize,
    #[cfg_attr(feature = "serde", serde(skip))]
    pub before: String,
    #[cfg_attr(feature = "serde", serde(skip))]
    pub after: String,
    pub diff: String,
}

/// Top-level result classification for a [`Plan`].
///
/// `Changes` — at least one file would be rewritten.
/// `AlreadyApplied` — zero matches across the whole scan *and* the
/// pattern is convergent (re-applying it to its own replacement would
/// produce no further change), so the run is treated as a successful
/// no-op rather than a guard violation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum PlanOutcome {
    Changes,
    AlreadyApplied,
}

/// Output of [`plan_rewrite`]. Pass to [`crate::apply_changes`] to commit.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Plan {
    pub changes: Vec<FileChange>,
    pub total_matches: usize,
    pub files_scanned: usize,
    pub outcome: PlanOutcome,
}

/// Walk `roots`, compile `pattern`, and produce a [`Plan`] of every file
/// that would change when `replacement` is substituted. Honors the
/// match-count guard, the convergence check, and the file/byte limits in
/// `opts`. No filesystem writes happen here.
pub fn plan_rewrite<P: AsRef<Path>>(
    pattern: &str,
    replacement: &str,
    roots: &[P],
    opts: &PlanOptions,
) -> Result<Plan> {
    let compiled = CompiledPattern::compile(pattern, replacement, &opts.pattern_options)?;
    debug!(pattern, "compiled regex");
    let files = walk_paths(roots, &opts.walk_options)?;
    debug!(files_scanned = files.len(), "walk completed");
    if files.len() > opts.max_files {
        return Err(Error::TooManyFiles { count: files.len(), limit: opts.max_files });
    }
    let files_scanned = files.len();

    let results: Vec<Result<Option<FileChange>>> =
        files.par_iter().map(|path| process_one(&compiled, path, opts)).collect();

    let mut changes = Vec::new();
    for r in results {
        if let Some(change) = r? {
            changes.push(change);
        }
    }
    changes.sort_by(|a, b| a.path.cmp(&b.path));
    let total_matches: usize = changes.iter().map(|c| c.matches).sum();
    debug!(files_changed = changes.len(), total_matches, "rewrite plan ready");

    if total_matches == 0 && compiled.is_convergent() {
        debug!("already applied (zero matches, convergent pattern)");
        return Ok(Plan {
            changes: Vec::new(),
            total_matches: 0,
            files_scanned,
            outcome: PlanOutcome::AlreadyApplied,
        });
    }

    if let Some(min) = opts.at_least
        && total_matches < min
    {
        return Err(Error::TooFewMatches { found: total_matches, required: min });
    }
    if let Some(max) = opts.at_most
        && total_matches > max
    {
        return Err(Error::TooManyMatches { found: total_matches, allowed: max });
    }

    Ok(Plan { changes, total_matches, files_scanned, outcome: PlanOutcome::Changes })
}

fn process_one(
    pattern: &CompiledPattern,
    path: &Path,
    opts: &PlanOptions,
) -> Result<Option<FileChange>> {
    let metadata =
        fs::metadata(path).map_err(|e| Error::Io { path: path.to_path_buf(), source: e })?;
    if metadata.len() > opts.max_bytes {
        return Err(Error::FileTooLarge {
            path: path.to_path_buf(),
            size: metadata.len(),
            limit: opts.max_bytes,
        });
    }
    let before = match fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::InvalidData => return Ok(None),
        Err(e) => return Err(Error::Io { path: path.to_path_buf(), source: e }),
    };

    let outcome = rewrite_text(pattern, &before);
    if !outcome.changed() {
        return Ok(None);
    }
    trace!(path = %path.display(), matches = outcome.matches, "file would change");

    if !opts.allow_non_convergent {
        let second = rewrite_text(pattern, &outcome.after);
        if second.changed() {
            return Err(Error::NonConvergent { path: path.to_path_buf(), extra: second.matches });
        }
    }

    let label = label_for_path(path);
    let diff = unified_diff(&label, &outcome.before, &outcome.after);
    Ok(Some(FileChange {
        path: path.to_path_buf(),
        matches: outcome.matches,
        before: outcome.before,
        after: outcome.after,
        diff,
    }))
}

#[cfg(test)]
#[path = "plan_tests.rs"]
mod tests;

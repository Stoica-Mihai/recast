//! End-to-end planning: walk → compile → rewrite → guard → check
//! convergence.
//!
//! [`plan_rewrite`] is the single entry point most callers want. It
//! produces a [`Plan`] describing every file that would change without
//! touching the filesystem; pass the plan to
//! [`crate::apply_changes`] to commit.

use std::fs::{self, Permissions};
use std::path::{Path, PathBuf};

use rayon::prelude::*;
use tracing::{debug, trace};

use crate::error::{Error, IoCtx, Result};
use crate::pattern::{CompiledPattern, PatternOptions};
#[cfg(feature = "script")]
use crate::rewrite::rewrite_text_scripted;
use crate::rewrite::{RewriteOutcome, label_for_path, rewrite_text, unified_diff};
#[cfg(feature = "script")]
use crate::script::ScriptRewriter;
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
    /// Skip the post-rewrite syntax-regression guard. Off by default —
    /// a rewrite whose output introduces *new* tree-sitter parse errors
    /// (relative to the pre-image) is rejected. Only files whose
    /// extension maps to a compiled grammar are checked; everything
    /// else passes through unguarded.
    pub allow_syntax_errors: bool,
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
            allow_syntax_errors: false,
            max_bytes: 10 * 1024 * 1024,
            max_files: 1000,
        }
    }
}

/// One file's worth of pending rewrite work. `after` is the full
/// post-image used by [`crate::apply_changes`]; `diff` is the
/// already-rendered unified-diff string. The pre-image is dropped
/// after the diff is built — `apply_changes` reads from `after`, not
/// from the original on disk, so retaining the pre-image would just
/// double the planner's peak memory. `permissions` is captured during
/// the planner's metadata read so [`crate::apply_changes`] doesn't
/// have to issue a second `fs::metadata` syscall just to preserve
/// the mode bits.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct FileChange {
    pub path: PathBuf,
    pub matches: usize,
    #[cfg_attr(feature = "serde", serde(skip))]
    pub after: String,
    pub diff: String,
    #[cfg_attr(feature = "serde", serde(skip))]
    pub permissions: Option<Permissions>,
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
    let files = scan(roots, opts)?;
    let files_scanned = files.len();

    let results: Vec<Result<Option<FileChange>>> = files
        .par_iter()
        .map(|path| {
            process_one(
                &compiled,
                path,
                opts,
                |p, s| Ok(rewrite_text(p, s)),
                regex_convergence_check,
            )
        })
        .collect();
    let changes = collect_changes(results)?;
    finalize_plan(changes, compiled.is_convergent(), files_scanned, opts)
}

fn regex_convergence_check(pattern: &CompiledPattern, after: &str) -> Result<usize> {
    Ok(pattern.regex().find_iter(after).count())
}

/// Like [`plan_rewrite`] but each match drives a Rhai script callback
/// instead of a static template. The pattern's `replacement` field is
/// ignored.
///
/// Each rayon worker gets its own sandboxed Rhai `Engine` (via
/// [`ScriptRewriter::fresh`]) because `Engine` is `!Sync`; the compiled
/// AST is shared by reference across workers.
#[cfg(feature = "script")]
pub fn plan_rewrite_scripted<P: AsRef<Path>>(
    pattern: &str,
    script: &ScriptRewriter,
    roots: &[P],
    opts: &PlanOptions,
) -> Result<Plan> {
    let compiled = CompiledPattern::compile(pattern, "", &opts.pattern_options)?;
    debug!(pattern, "compiled regex (scripted)");
    let files = scan(roots, opts)?;
    let files_scanned = files.len();

    let results: Vec<Result<Option<FileChange>>> = files
        .par_iter()
        .map_init(
            || script.fresh(),
            |worker, path| {
                let rewrite = |p: &CompiledPattern, s: &str| rewrite_text_scripted(p, worker, s);
                let converge = |p: &CompiledPattern, s: &str| -> Result<usize> {
                    let outcome = rewrite_text_scripted(p, worker, s)?;
                    Ok(if outcome.after != s { outcome.matches } else { 0 })
                };
                process_one(&compiled, path, opts, rewrite, converge)
            },
        )
        .collect();
    let changes = collect_changes(results)?;
    // Scripts can't be probed statically; trust the per-file dynamic
    // convergence check inside process_one and treat zero matches as
    // an already-applied no-op.
    finalize_plan(changes, true, files_scanned, opts)
}

fn scan<P: AsRef<Path>>(roots: &[P], opts: &PlanOptions) -> Result<Vec<PathBuf>> {
    let files = walk_paths(roots, &opts.walk_options)?;
    debug!(files_scanned = files.len(), "walk completed");
    if files.len() > opts.max_files {
        return Err(Error::TooManyFiles { count: files.len(), limit: opts.max_files });
    }
    Ok(files)
}

fn collect_changes(results: Vec<Result<Option<FileChange>>>) -> Result<Vec<FileChange>> {
    let mut changes = Vec::new();
    for r in results {
        if let Some(change) = r? {
            changes.push(change);
        }
    }
    Ok(changes)
}

fn finalize_plan(
    changes: Vec<FileChange>,
    convergent_or_scripted: bool,
    files_scanned: usize,
    opts: &PlanOptions,
) -> Result<Plan> {
    let total_matches: usize = changes.iter().map(|c| c.matches).sum();
    debug!(files_changed = changes.len(), total_matches, "rewrite plan ready");

    if total_matches == 0 && convergent_or_scripted {
        debug!("already applied (zero matches)");
        return Ok(Plan {
            changes: Vec::new(),
            total_matches: 0,
            files_scanned,
            outcome: PlanOutcome::AlreadyApplied,
        });
    }

    check_match_counts(total_matches, opts.at_least, opts.at_most)?;

    Ok(Plan { changes, total_matches, files_scanned, outcome: PlanOutcome::Changes })
}

/// Enforce the `--at-least` / `--at-most` match-count guard. Returns
/// [`Error::TooFewMatches`] / [`Error::TooManyMatches`] when the
/// guard is violated; both variants map to the
/// `EXIT_GUARD_VIOLATED` (2) exit code at the binary boundary.
pub fn check_match_counts(
    found: usize,
    at_least: Option<usize>,
    at_most: Option<usize>,
) -> Result<()> {
    if let Some(min) = at_least
        && found < min
    {
        return Err(Error::TooFewMatches { found, required: min });
    }
    if let Some(max) = at_most
        && found > max
    {
        return Err(Error::TooManyMatches { found, allowed: max });
    }
    Ok(())
}

fn process_one<R, C>(
    pattern: &CompiledPattern,
    path: &Path,
    opts: &PlanOptions,
    rewrite: R,
    convergence_check: C,
) -> Result<Option<FileChange>>
where
    R: Fn(&CompiledPattern, &str) -> Result<RewriteOutcome>,
    C: Fn(&CompiledPattern, &str) -> Result<usize>,
{
    let (before, permissions) = match read_text_or_skip_binary(path, opts.max_bytes)? {
        Some(pair) => pair,
        None => return Ok(None),
    };

    let outcome = rewrite(pattern, &before)?;
    if outcome.matches == 0 || outcome.after == before {
        return Ok(None);
    }
    trace!(path = %path.display(), matches = outcome.matches, "file would change");

    if !opts.allow_non_convergent {
        let extra = convergence_check(pattern, &outcome.after)?;
        if extra > 0 {
            return Err(Error::NonConvergent { path: path.to_path_buf(), extra });
        }
    }

    #[cfg(any(
        feature = "lang-rust",
        feature = "lang-ts",
        feature = "lang-js",
        feature = "lang-python",
    ))]
    if !opts.allow_syntax_errors {
        crate::structural::guard_syntax(path, &before, &outcome.after)?;
    }

    let label = label_for_path(path);
    let diff = unified_diff(&label, &before, &outcome.after);
    Ok(Some(FileChange {
        path: path.to_path_buf(),
        matches: outcome.matches,
        after: outcome.after,
        diff,
        permissions: Some(permissions),
    }))
}

/// Read a candidate file, enforce the per-file byte limit, and yield
/// `None` for paths whose contents aren't valid UTF-8 (binary skip).
/// Returns the file contents alongside the permissions captured from
/// the same metadata call so the commit phase doesn't have to stat the
/// file again. Shared by the regex and structural pipelines.
pub(crate) fn read_text_or_skip_binary(
    path: &Path,
    max_bytes: u64,
) -> Result<Option<(String, Permissions)>> {
    let metadata = fs::metadata(path).io_ctx(path)?;
    if metadata.len() > max_bytes {
        return Err(Error::FileTooLarge {
            path: path.to_path_buf(),
            size: metadata.len(),
            limit: max_bytes,
        });
    }
    let permissions = metadata.permissions();
    match fs::read_to_string(path) {
        Ok(s) => Ok(Some((s, permissions))),
        Err(e) if e.kind() == std::io::ErrorKind::InvalidData => Ok(None),
        Err(e) => Err(Error::Io { path: path.to_path_buf(), source: e }),
    }
}

#[cfg(test)]
#[path = "plan_tests.rs"]
mod tests;

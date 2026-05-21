use std::fs;
use std::path::{Path, PathBuf};

use rayon::prelude::*;

use crate::error::{Error, Result};
use crate::pattern::{CompiledPattern, PatternOptions};
use crate::rewrite::{rewrite_text, unified_diff};
use crate::walker::{WalkOptions, walk_paths};

#[derive(Debug, Clone)]
pub struct PlanOptions {
    pub pattern_options: PatternOptions,
    pub walk_options: WalkOptions,
    pub at_least: Option<usize>,
    pub at_most: Option<usize>,
    pub allow_non_convergent: bool,
    pub max_bytes: u64,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum PlanOutcome {
    Changes,
    AlreadyApplied,
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Plan {
    pub changes: Vec<FileChange>,
    pub total_matches: usize,
    pub files_scanned: usize,
    pub outcome: PlanOutcome,
}

pub fn plan_rewrite<P: AsRef<Path>>(
    pattern: &str,
    replacement: &str,
    roots: &[P],
    opts: &PlanOptions,
) -> Result<Plan> {
    let compiled = CompiledPattern::compile(pattern, replacement, &opts.pattern_options)?;
    let files = walk_paths(roots, &opts.walk_options)?;
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

    if total_matches == 0 && compiled.is_convergent() {
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

    if !opts.allow_non_convergent {
        let second = rewrite_text(pattern, &outcome.after);
        if second.changed() {
            return Err(Error::NonConvergent { path: path.to_path_buf(), extra: second.matches });
        }
    }

    let label = path.to_string_lossy().into_owned();
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
mod tests {
    #![allow(clippy::unwrap_used, clippy::field_reassign_with_default)]

    use std::fs;

    use tempfile::TempDir;

    use super::*;

    fn fixture(files: &[(&str, &str)]) -> TempDir {
        let dir = TempDir::new().unwrap();
        for (name, body) in files {
            let path = dir.path().join(name);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            fs::write(&path, body).unwrap();
        }
        dir
    }

    #[test]
    fn plan_collects_changes_across_files() {
        let dir = fixture(&[("a.txt", "Old name\n"), ("b.txt", "Old Old\n")]);
        let plan = plan_rewrite("Old", "New", &[dir.path()], &PlanOptions::default()).unwrap();
        assert_eq!(plan.outcome, PlanOutcome::Changes);
        assert_eq!(plan.total_matches, 3);
        assert_eq!(plan.changes.len(), 2);
    }

    #[test]
    fn plan_already_applied_when_no_matches_and_convergent() {
        let dir = fixture(&[("a.txt", "New name\n")]);
        let plan = plan_rewrite("Old", "New", &[dir.path()], &PlanOptions::default()).unwrap();
        assert_eq!(plan.outcome, PlanOutcome::AlreadyApplied);
    }

    #[test]
    fn plan_rejects_non_convergent_pattern() {
        let dir = fixture(&[("a.txt", "abc\n")]);
        let err = plan_rewrite("a", "aa", &[dir.path()], &PlanOptions::default()).unwrap_err();
        assert!(matches!(err, Error::NonConvergent { .. }));
    }

    #[test]
    fn plan_match_guard_too_few() {
        let dir = fixture(&[("a.txt", "Old\n")]);
        let mut opts = PlanOptions::default();
        opts.at_least = Some(2);
        let err = plan_rewrite("Old", "New", &[dir.path()], &opts).unwrap_err();
        assert!(matches!(err, Error::TooFewMatches { found: 1, required: 2 }));
    }

    #[test]
    fn plan_match_guard_too_many() {
        let dir = fixture(&[("a.txt", "Old Old Old\n")]);
        let mut opts = PlanOptions::default();
        opts.at_most = Some(2);
        let err = plan_rewrite("Old", "New", &[dir.path()], &opts).unwrap_err();
        assert!(matches!(err, Error::TooManyMatches { found: 3, allowed: 2 }));
    }

    #[test]
    fn plan_match_guard_zero_allows_empty_match() {
        let dir = fixture(&[("a.txt", "unrelated\n")]);
        let mut opts = PlanOptions::default();
        opts.at_least = Some(0);
        let plan = plan_rewrite("Zzz", "Q", &[dir.path()], &opts).unwrap();
        assert_eq!(plan.outcome, PlanOutcome::AlreadyApplied);
    }

    #[test]
    fn plan_too_many_files() {
        let dir = fixture(&[("a.txt", "x"), ("b.txt", "x"), ("c.txt", "x")]);
        let mut opts = PlanOptions::default();
        opts.max_files = 2;
        opts.at_least = Some(0);
        let err = plan_rewrite("Z", "Q", &[dir.path()], &opts).unwrap_err();
        assert!(matches!(err, Error::TooManyFiles { count: 3, limit: 2 }));
    }
}

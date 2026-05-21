//! JSON output schema for the recast CLI.
//!
//! Stable, single-line-per-invocation. Every report carries a `kind`
//! discriminator. Non-error reports share `outcome` (`"changes"` or
//! `"already_applied"`), `files_scanned`, and `total_matches`. Each mode
//! adds the count it owns:
//!
//! - `plan`  → `files_changed`, `changes: [{path, matches}]`
//! - `apply` → `files_written`
//! - `check` → `files_would_change`
//!
//! Errors emit `{kind: "error", error: <snake_case kind>, message, exit_code}`.

use std::path::Path;

use serde::Serialize;

use crate::commit::ApplyOutcome;
use crate::error::Error;
use crate::plan::{Plan, PlanOutcome};

#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum JsonReport<'a> {
    Plan {
        outcome: PlanOutcome,
        files_scanned: usize,
        files_changed: usize,
        total_matches: usize,
        changes: Vec<JsonFile<'a>>,
    },
    Apply {
        outcome: PlanOutcome,
        files_scanned: usize,
        files_written: usize,
        total_matches: usize,
    },
    Check {
        outcome: PlanOutcome,
        files_scanned: usize,
        files_would_change: usize,
        total_matches: usize,
    },
    Error {
        error: ErrorKind,
        message: String,
        exit_code: u8,
    },
}

#[derive(Debug, Serialize)]
pub struct JsonFile<'a> {
    pub path: &'a Path,
    pub matches: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorKind {
    InvalidRegex,
    InvalidGlob,
    Walk,
    Io,
    FileTooLarge,
    TooManyFiles,
    NonConvergent,
    TooFewMatches,
    TooManyMatches,
}

impl JsonReport<'_> {
    pub fn to_line(&self) -> serde_json::Result<String> {
        serde_json::to_string(self)
    }
}

pub fn from_plan(plan: &Plan) -> JsonReport<'_> {
    JsonReport::Plan {
        outcome: plan.outcome,
        files_scanned: plan.files_scanned,
        files_changed: plan.changes.len(),
        total_matches: plan.total_matches,
        changes: plan
            .changes
            .iter()
            .map(|c| JsonFile { path: c.path.as_path(), matches: c.matches })
            .collect(),
    }
}

pub fn from_apply<'a>(plan: &'a Plan, outcome: &ApplyOutcome) -> JsonReport<'a> {
    JsonReport::Apply {
        outcome: plan.outcome,
        files_scanned: plan.files_scanned,
        files_written: outcome.files_written,
        total_matches: outcome.total_matches,
    }
}

pub fn from_check(plan: &Plan) -> JsonReport<'_> {
    JsonReport::Check {
        outcome: plan.outcome,
        files_scanned: plan.files_scanned,
        files_would_change: plan.changes.len(),
        total_matches: plan.total_matches,
    }
}

pub fn from_error(err: &Error, exit_code: u8) -> JsonReport<'static> {
    JsonReport::Error { error: error_kind(err), message: err.to_string(), exit_code }
}

pub fn error_kind(err: &Error) -> ErrorKind {
    match err {
        Error::InvalidRegex(_) => ErrorKind::InvalidRegex,
        Error::InvalidGlob(_) => ErrorKind::InvalidGlob,
        Error::Walk(_) => ErrorKind::Walk,
        Error::Io { .. } => ErrorKind::Io,
        Error::FileTooLarge { .. } => ErrorKind::FileTooLarge,
        Error::TooManyFiles { .. } => ErrorKind::TooManyFiles,
        Error::NonConvergent { .. } => ErrorKind::NonConvergent,
        Error::TooFewMatches { .. } => ErrorKind::TooFewMatches,
        Error::TooManyMatches { .. } => ErrorKind::TooManyMatches,
    }
}

#[cfg(test)]
#[path = "json_tests.rs"]
mod tests;

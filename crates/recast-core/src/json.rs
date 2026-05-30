//! JSON output schema for the recast CLI.
//!
//! Stable, single-line-per-invocation. Every report carries a `kind`
//! discriminator. Non-error reports share `outcome` (`"changes"` or
//! `"already_applied"`), `files_scanned`, and `total_matches` via the
//! shared [`JsonHeader`]. Each mode adds the count it owns:
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
pub use crate::error::ErrorKind;
use crate::plan::{Plan, PlanOutcome};
use crate::search::SearchPlan;

/// Fields shared by every non-error report. Flattened into the wire JSON
/// so consumers see the same flat object they always have.
#[derive(Debug, Serialize)]
pub struct JsonHeader {
    pub outcome: PlanOutcome,
    pub files_scanned: usize,
    pub total_matches: usize,
}

#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum JsonReport<'a> {
    Plan {
        #[serde(flatten)]
        header: JsonHeader,
        files_changed: usize,
        changes: Vec<JsonFile<'a>>,
    },
    Apply {
        #[serde(flatten)]
        header: JsonHeader,
        files_written: usize,
    },
    Check {
        #[serde(flatten)]
        header: JsonHeader,
        files_would_change: usize,
    },
    Search {
        files_scanned: usize,
        total_matches: usize,
        files: Vec<JsonSearchFile>,
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

#[derive(Debug, Serialize)]
pub struct JsonSearchMatch {
    pub line: usize,
    pub column: usize,
    pub snippet: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capture: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct JsonSearchFile {
    pub path: String,
    pub matches: Vec<JsonSearchMatch>,
}

impl JsonReport<'_> {
    pub fn to_line(&self) -> serde_json::Result<String> {
        serde_json::to_string(self)
    }
}

fn header(plan: &Plan) -> JsonHeader {
    JsonHeader {
        outcome: plan.outcome,
        files_scanned: plan.files_scanned,
        total_matches: plan.total_matches,
    }
}

pub fn from_plan(plan: &Plan) -> JsonReport<'_> {
    JsonReport::Plan {
        header: header(plan),
        files_changed: plan.changes.len(),
        changes: plan
            .changes
            .iter()
            .map(|c| JsonFile { path: c.path.as_path(), matches: c.matches })
            .collect(),
    }
}

pub fn from_apply<'a>(plan: &'a Plan, outcome: &ApplyOutcome) -> JsonReport<'a> {
    // `outcome.total_matches` is always sourced from `plan.total_matches`
    // (see commit::apply_changes); routing through `header(plan)` keeps
    // a single source of truth so the two paths can't drift.
    JsonReport::Apply { header: header(plan), files_written: outcome.files_written }
}

pub fn from_check(plan: &Plan) -> JsonReport<'_> {
    JsonReport::Check { header: header(plan), files_would_change: plan.changes.len() }
}

pub fn from_error(err: &Error, exit_code: u8) -> JsonReport<'static> {
    JsonReport::Error { error: err.kind(), message: err.to_string(), exit_code }
}

pub fn from_search(plan: &SearchPlan) -> JsonReport<'static> {
    JsonReport::Search {
        files_scanned: plan.files_scanned,
        total_matches: plan.total_matches,
        files: plan
            .files
            .iter()
            .map(|f| JsonSearchFile {
                path: f.path.display().to_string(),
                matches: f
                    .matches
                    .iter()
                    .map(|m| JsonSearchMatch {
                        line: m.line,
                        column: m.column,
                        snippet: m.snippet.clone(),
                        capture: m.capture.clone(),
                    })
                    .collect(),
            })
            .collect(),
    }
}

#[cfg(test)]
#[path = "json_tests.rs"]
mod tests;

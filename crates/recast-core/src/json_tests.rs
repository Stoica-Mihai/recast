#![allow(clippy::unwrap_used)]

use std::path::PathBuf;

use insta::assert_snapshot;

use super::*;
use crate::commit::ApplyOutcome;
use crate::error::Error;
use crate::plan::{FileChange, Plan, PlanOutcome};

fn sample_changes() -> Vec<FileChange> {
    vec![
        FileChange {
            path: PathBuf::from("src/a.rs"),
            matches: 2,
            before: String::new(),
            after: String::new(),
            diff: String::new(),
        },
        FileChange {
            path: PathBuf::from("src/b.rs"),
            matches: 1,
            before: String::new(),
            after: String::new(),
            diff: String::new(),
        },
    ]
}

fn changes_plan() -> Plan {
    Plan {
        changes: sample_changes(),
        total_matches: 3,
        files_scanned: 5,
        outcome: PlanOutcome::Changes,
    }
}

fn already_applied_plan() -> Plan {
    Plan {
        changes: vec![],
        total_matches: 0,
        files_scanned: 5,
        outcome: PlanOutcome::AlreadyApplied,
    }
}

#[test]
fn plan_json_with_changes() {
    assert_snapshot!(from_plan(&changes_plan()).to_line().unwrap());
}

#[test]
fn plan_json_already_applied() {
    assert_snapshot!(from_plan(&already_applied_plan()).to_line().unwrap());
}

#[test]
fn apply_json_with_changes() {
    let plan = changes_plan();
    let outcome = ApplyOutcome { files_written: 2, total_matches: 3 };
    assert_snapshot!(from_apply(&plan, &outcome).to_line().unwrap());
}

#[test]
fn apply_json_already_applied() {
    let plan = already_applied_plan();
    let outcome = ApplyOutcome { files_written: 0, total_matches: 0 };
    assert_snapshot!(from_apply(&plan, &outcome).to_line().unwrap());
}

#[test]
fn check_json_would_change() {
    assert_snapshot!(from_check(&changes_plan()).to_line().unwrap());
}

#[test]
fn check_json_already_applied() {
    assert_snapshot!(from_check(&already_applied_plan()).to_line().unwrap());
}

#[test]
fn error_json_too_few_matches() {
    let err = Error::TooFewMatches { found: 0, required: 1 };
    assert_snapshot!(from_error(&err, 2).to_line().unwrap());
}

#[test]
fn error_json_too_many_matches() {
    let err = Error::TooManyMatches { found: 5, allowed: 3 };
    assert_snapshot!(from_error(&err, 2).to_line().unwrap());
}

#[test]
fn error_json_non_convergent() {
    let err = Error::NonConvergent { path: PathBuf::from("src/a.rs"), extra: 3 };
    assert_snapshot!(from_error(&err, 3).to_line().unwrap());
}

#[test]
fn error_json_too_many_files() {
    let err = Error::TooManyFiles { count: 1500, limit: 1000 };
    assert_snapshot!(from_error(&err, 3).to_line().unwrap());
}

#[test]
fn error_json_file_too_large() {
    let err =
        Error::FileTooLarge { path: PathBuf::from("big.bin"), size: 20_000_000, limit: 10_485_760 };
    assert_snapshot!(from_error(&err, 3).to_line().unwrap());
}

#[test]
fn error_kind_covers_every_error_variant() {
    let cases = [
        (Error::TooFewMatches { found: 0, required: 1 }, ErrorKind::TooFewMatches),
        (Error::TooManyMatches { found: 5, allowed: 3 }, ErrorKind::TooManyMatches),
        (Error::NonConvergent { path: PathBuf::from("x"), extra: 1 }, ErrorKind::NonConvergent),
        (Error::TooManyFiles { count: 2, limit: 1 }, ErrorKind::TooManyFiles),
        (
            Error::FileTooLarge { path: PathBuf::from("x"), size: 2, limit: 1 },
            ErrorKind::FileTooLarge,
        ),
    ];
    for (err, expected) in cases {
        assert_eq!(error_kind(&err), expected, "wrong ErrorKind for {err:?}");
    }
}

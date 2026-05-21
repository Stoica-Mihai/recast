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
fn plan_skips_non_utf8_binary_files() {
    let dir = TempDir::new().unwrap();
    let text = dir.path().join("a.txt");
    let bin = dir.path().join("b.bin");
    fs::write(&text, b"Old name\n").unwrap();
    fs::write(&bin, [0xFF, 0xFE, 0x00, 0x01, 0x4F, 0x6C, 0x64]).unwrap();
    let plan = plan_rewrite("Old", "New", &[dir.path()], &PlanOptions::default()).unwrap();
    assert_eq!(plan.outcome, PlanOutcome::Changes);
    assert_eq!(plan.changes.len(), 1);
    assert!(plan.changes[0].path.ends_with("a.txt"));
    let before = fs::read(&bin).unwrap();
    assert_eq!(before, vec![0xFF, 0xFE, 0x00, 0x01, 0x4F, 0x6C, 0x64]);
}

#[test]
fn plan_rejects_files_over_max_bytes() {
    let dir = fixture(&[("big.txt", "Old".repeat(2000).as_str())]);
    let mut opts = PlanOptions::default();
    opts.max_bytes = 64;
    let err = plan_rewrite("Old", "New", &[dir.path()], &opts).unwrap_err();
    assert!(matches!(err, Error::FileTooLarge { size: 6000, limit: 64, .. }));
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

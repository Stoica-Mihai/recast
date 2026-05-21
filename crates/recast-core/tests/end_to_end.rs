#![allow(clippy::unwrap_used)]

use std::fs;

use recast_core::{PlanOptions, PlanOutcome, apply_changes, plan_rewrite};
use tempfile::TempDir;

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
fn end_to_end_rename_across_nested_tree() {
    let dir = fixture(&[
        ("src/lib.rs", "fn OldName() {}\n"),
        ("src/sub/mod.rs", "use crate::OldName;\nfn other() { OldName(); }\n"),
        ("README.md", "# OldName project\n"),
    ]);
    let plan =
        plan_rewrite(r"\bOldName\b", "NewName", &[dir.path()], &PlanOptions::default()).unwrap();
    assert_eq!(plan.outcome, PlanOutcome::Changes);
    assert_eq!(plan.changes.len(), 3);
    assert_eq!(plan.total_matches, 4);

    let outcome = apply_changes(&plan).unwrap();
    assert_eq!(outcome.files_written, 3);
    assert_eq!(outcome.total_matches, 4);

    let lib = fs::read_to_string(dir.path().join("src/lib.rs")).unwrap();
    assert_eq!(lib, "fn NewName() {}\n");
    let modr = fs::read_to_string(dir.path().join("src/sub/mod.rs")).unwrap();
    assert_eq!(modr, "use crate::NewName;\nfn other() { NewName(); }\n");
    let readme = fs::read_to_string(dir.path().join("README.md")).unwrap();
    assert_eq!(readme, "# NewName project\n");

    let replay =
        plan_rewrite(r"\bOldName\b", "NewName", &[dir.path()], &PlanOptions::default()).unwrap();
    assert_eq!(replay.outcome, PlanOutcome::AlreadyApplied);
}

#[test]
fn capture_groups_interpolate() {
    let dir = fixture(&[("a.txt", "version = 1.2.3\n")]);
    let plan = plan_rewrite(
        r"^version = (\d+)\.(\d+)\.(\d+)$",
        "version = $1.$2.${3}-dev",
        &[dir.path()],
        &PlanOptions::default(),
    )
    .unwrap();
    apply_changes(&plan).unwrap();
    let s = fs::read_to_string(dir.path().join("a.txt")).unwrap();
    assert_eq!(s, "version = 1.2.3-dev\n");
}

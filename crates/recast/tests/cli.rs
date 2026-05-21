#![allow(clippy::unwrap_used)]

use std::fs;

use assert_cmd::Command;
use predicates::prelude::*;
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

fn recast() -> Command {
    Command::cargo_bin("recast").unwrap()
}

#[test]
fn diff_mode_exits_zero_and_shows_changes() {
    let dir = fixture(&[("a.txt", "Old name\n")]);
    recast()
        .arg("Old")
        .arg("New")
        .arg(dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("-Old name").and(predicate::str::contains("+New name")));
}

#[test]
fn apply_mode_writes_files_and_reports_to_stderr() {
    let dir = fixture(&[("a.txt", "Old\n")]);
    recast()
        .arg("--apply")
        .arg("Old")
        .arg("New")
        .arg(dir.path())
        .assert()
        .success()
        .stderr(predicate::str::contains("applying 1 file"));
    assert_eq!(fs::read_to_string(dir.path().join("a.txt")).unwrap(), "New\n");
}

#[test]
fn check_mode_exit_one_when_changes_pending() {
    let dir = fixture(&[("a.txt", "Old\n")]);
    recast().arg("--check").arg("Old").arg("New").arg(dir.path()).assert().code(1);
}

#[test]
fn check_mode_exit_zero_when_already_applied() {
    let dir = fixture(&[("a.txt", "New\n")]);
    recast().arg("--check").arg("Old").arg("New").arg(dir.path()).assert().code(0);
}

#[test]
fn match_guard_violation_exits_two() {
    let dir = fixture(&[("a.txt", "Old\n")]);
    recast()
        .arg("--at-least")
        .arg("5")
        .arg("Old")
        .arg("New")
        .arg(dir.path())
        .assert()
        .code(2)
        .stderr(predicate::str::contains("match-count guard violated"));
}

#[test]
fn json_plan_emits_kind_plan() {
    let dir = fixture(&[("a.txt", "Old\n")]);
    recast()
        .arg("--json")
        .arg("Old")
        .arg("New")
        .arg(dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains(r#""kind":"plan""#));
}

#[test]
fn json_error_exits_two_with_machine_readable_kind() {
    let dir = fixture(&[("a.txt", "Old\n")]);
    recast()
        .arg("--json")
        .arg("--at-least")
        .arg("5")
        .arg("Old")
        .arg("New")
        .arg(dir.path())
        .assert()
        .code(2)
        .stdout(
            predicate::str::contains(r#""kind":"error""#)
                .and(predicate::str::contains(r#""error":"too_few_matches""#))
                .and(predicate::str::contains(r#""exit_code":2"#)),
        );
}

#[test]
fn already_applied_message_on_rerun() {
    let dir = fixture(&[("a.txt", "Old\n")]);
    recast().arg("--apply").arg("Old").arg("New").arg(dir.path()).assert().success();
    recast()
        .arg("Old")
        .arg("New")
        .arg(dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("already applied"));
}

#[test]
fn completions_flag_outputs_shell_script() {
    recast()
        .arg("--completions")
        .arg("bash")
        .assert()
        .success()
        .stdout(predicate::str::contains("_recast()"));
}

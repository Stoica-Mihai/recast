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
fn stdin_mode_rewrites_buffer_to_stdout() {
    recast()
        .arg("--stdin")
        .arg("Old")
        .arg("New")
        .write_stdin("fn OldName() { Old(); }\n")
        .assert()
        .success()
        .stdout("fn NewName() { New(); }\n");
}

#[test]
fn stdin_mode_guard_violation_exits_two() {
    recast()
        .arg("--stdin")
        .arg("--at-least")
        .arg("5")
        .arg("Old")
        .arg("New")
        .write_stdin("Old once\n")
        .assert()
        .code(2)
        .stderr(predicate::str::contains("match-count guard violated"));
}

#[test]
fn stdin_mode_zero_matches_with_at_least_zero() {
    recast()
        .arg("--stdin")
        .arg("--at-least")
        .arg("0")
        .arg("Zzz")
        .arg("Q")
        .write_stdin("unrelated\n")
        .assert()
        .success()
        .stdout("unrelated\n");
}

#[test]
fn script_mode_apply_uppercases_via_rhai() {
    let dir = fixture(&[("a.txt", "foo bar baz\n")]);
    let script = dir.path().join("uc.rhai");
    fs::write(&script, "captures[1].to_upper()").unwrap();
    recast()
        .arg("--script")
        .arg(&script)
        .arg("--apply")
        .arg(r"\b(\w+)\b")
        .arg("")
        .arg(dir.path().join("a.txt"))
        .assert()
        .success();
    assert_eq!(fs::read_to_string(dir.path().join("a.txt")).unwrap(), "FOO BAR BAZ\n");
}

#[test]
fn script_mode_stdin_increments_number() {
    let dir = fixture(&[("bump.rhai", "(parse_int(whole) + 1).to_string()")]);
    recast()
        .arg("--stdin")
        .arg("--script")
        .arg(dir.path().join("bump.rhai"))
        .arg(r"\d+")
        .arg("")
        .write_stdin("version 3\n")
        .assert()
        .success()
        .stdout("version 4\n");
}

#[test]
fn structural_mode_renames_function_via_tree_sitter() {
    let dir = fixture(&[("lib.rs", "fn old_fn() {}\nfn other() { old_fn(); }\n")]);
    recast()
        .arg("--lang")
        .arg("rust")
        .arg("--query")
        .arg(r#"((identifier) @id (#eq? @id "old_fn"))"#)
        .arg("--apply")
        .arg("ignored")
        .arg("new_fn")
        .arg(dir.path())
        .assert()
        .success();
    assert_eq!(
        fs::read_to_string(dir.path().join("lib.rs")).unwrap(),
        "fn new_fn() {}\nfn other() { new_fn(); }\n"
    );
}

#[test]
fn structural_mode_stdin_uses_capture_template() {
    recast()
        .arg("--lang")
        .arg("rust")
        .arg("--query")
        .arg(r#"(function_item name: (identifier) @name) @root"#)
        .arg("--stdin")
        .arg("ignored")
        .arg("fn ${name}_v2() {}")
        .write_stdin("fn foo() {}\n")
        .assert()
        .success()
        .stdout("fn foo_v2() {}\n");
}

#[test]
fn syntax_regression_guard_rejects_and_leaves_file_untouched() {
    let body = "fn a() {\n    work();\n}\nfn b() {}\n";
    let dir = fixture(&[("a.rs", body)]);
    recast()
        .arg("--apply")
        .arg(r"fn a\(\) \{\n")
        .arg("")
        .arg(dir.path())
        .assert()
        .code(3)
        .stderr(predicate::str::contains("syntax error"));
    assert_eq!(fs::read_to_string(dir.path().join("a.rs")).unwrap(), body);
}

#[test]
fn allow_syntax_errors_flag_overrides_guard() {
    let dir = fixture(&[("a.rs", "fn a() {\n    work();\n}\nfn b() {}\n")]);
    recast()
        .arg("--apply")
        .arg("--allow-syntax-errors")
        .arg(r"fn a\(\) \{\n")
        .arg("")
        .arg(dir.path())
        .assert()
        .success();
    assert_eq!(fs::read_to_string(dir.path().join("a.rs")).unwrap(), "    work();\n}\nfn b() {}\n");
}

#[test]
fn structural_mode_unknown_language_errors() {
    recast()
        .arg("--lang")
        .arg("klingon")
        .arg("--query")
        .arg("(identifier) @id")
        .arg("pat")
        .arg("rep")
        .assert()
        .code(3)
        .stderr(predicate::str::contains("unknown language"));
}

#[test]
fn structural_friendly_ast_pattern_renames_function() {
    let dir = fixture(&[("lib.rs", "fn old_thing() {}\nfn keep() {}\n")]);
    recast()
        .arg("--lang")
        .arg("rust")
        .arg("--ast")
        .arg("fn old_thing() {}")
        .arg("--apply")
        .arg("ignored")
        .arg("fn new_thing() {}")
        .arg(dir.path())
        .assert()
        .success();
    assert_eq!(
        fs::read_to_string(dir.path().join("lib.rs")).unwrap(),
        "fn new_thing() {}\nfn keep() {}\n"
    );
}

#[test]
fn structural_friendly_ast_metavar_captures_name() {
    let dir = fixture(&[("lib.rs", "fn foo() {}\nfn bar() {}\n")]);
    recast()
        .arg("--lang")
        .arg("rust")
        .arg("--ast")
        .arg("fn $NAME() {}")
        .arg("--apply")
        .arg("ignored")
        .arg("fn ${NAME}_v2() {}")
        .arg(dir.path())
        .assert()
        .success();
    assert_eq!(
        fs::read_to_string(dir.path().join("lib.rs")).unwrap(),
        "fn foo_v2() {}\nfn bar_v2() {}\n"
    );
}

#[test]
fn recover_flag_restores_orphan_backup() {
    let dir = TempDir::new().unwrap();
    let a = dir.path().join("a.txt");
    let bak = dir.path().join(".a.txt.recast.bak.42");
    fs::write(&bak, "Original\n").unwrap();
    recast()
        .arg("--recover")
        .arg(dir.path())
        .assert()
        .success()
        .stderr(predicate::str::contains("recovered 1 backup"));
    assert!(!bak.exists());
    assert_eq!(fs::read_to_string(&a).unwrap(), "Original\n");
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

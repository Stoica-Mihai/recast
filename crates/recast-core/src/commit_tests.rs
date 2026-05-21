#![allow(clippy::unwrap_used, clippy::field_reassign_with_default)]

use std::fs;

use tempfile::TempDir;

use super::*;
use crate::plan::{PlanOptions, plan_rewrite};

#[test]
fn apply_writes_changes_to_disk() {
    let dir = TempDir::new().unwrap();
    let file = dir.path().join("a.txt");
    fs::write(&file, "Old name\n").unwrap();
    let plan = plan_rewrite("Old", "New", &[dir.path()], &PlanOptions::default()).unwrap();
    let outcome = apply_changes(&plan).unwrap();
    assert_eq!(outcome.files_written, 1);
    assert_eq!(fs::read_to_string(&file).unwrap(), "New name\n");
}

#[test]
fn apply_with_no_changes_is_a_noop() {
    let dir = TempDir::new().unwrap();
    let file = dir.path().join("a.txt");
    fs::write(&file, "unrelated\n").unwrap();
    let mut opts = PlanOptions::default();
    opts.at_least = Some(0);
    let plan = plan_rewrite("Zzz", "Q", &[dir.path()], &opts).unwrap();
    let outcome = apply_changes(&plan).unwrap();
    assert_eq!(outcome.files_written, 0);
    assert_eq!(fs::read_to_string(&file).unwrap(), "unrelated\n");
}

#[test]
fn apply_preserves_file_permissions() {
    use std::os::unix::fs::PermissionsExt;

    let dir = TempDir::new().unwrap();
    let file = dir.path().join("script.sh");
    fs::write(&file, "Old\n").unwrap();
    fs::set_permissions(&file, fs::Permissions::from_mode(0o750)).unwrap();
    let plan = plan_rewrite("Old", "New", &[dir.path()], &PlanOptions::default()).unwrap();
    apply_changes(&plan).unwrap();
    let mode = fs::metadata(&file).unwrap().permissions().mode() & 0o7777;
    assert_eq!(mode, 0o750);
}

#[test]
fn apply_leaves_no_temp_or_backup_files_behind() {
    let dir = TempDir::new().unwrap();
    let file = dir.path().join("a.txt");
    fs::write(&file, "Old\n").unwrap();
    let plan = plan_rewrite("Old", "New", &[dir.path()], &PlanOptions::default()).unwrap();
    apply_changes(&plan).unwrap();
    let names: Vec<String> = fs::read_dir(dir.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, vec!["a.txt"]);
}

#[test]
fn rollback_restores_tree_when_commit_fails_midway() {
    let dir = TempDir::new().unwrap();
    let a = dir.path().join("a.txt");
    let b = dir.path().join("b.txt");
    let c = dir.path().join("c.txt");
    fs::write(&a, "Old A\n").unwrap();
    fs::write(&b, "Old B\n").unwrap();
    fs::write(&c, "Old C\n").unwrap();
    let plan = plan_rewrite("Old", "New", &[dir.path()], &PlanOptions::default()).unwrap();
    assert_eq!(plan.changes.len(), 3);

    let err = apply_inner(&plan, |i| {
        if i == 1 {
            Err(Error::Io {
                path: dir.path().to_path_buf(),
                source: std::io::Error::other("injected mid-commit failure"),
            })
        } else {
            Ok(())
        }
    })
    .unwrap_err();
    assert!(matches!(err, Error::Io { .. }));

    assert_eq!(fs::read_to_string(&a).unwrap(), "Old A\n");
    assert_eq!(fs::read_to_string(&b).unwrap(), "Old B\n");
    assert_eq!(fs::read_to_string(&c).unwrap(), "Old C\n");

    let names: Vec<String> = fs::read_dir(dir.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|n| {
            !n.starts_with(".a.txt.recast")
                && !n.starts_with(".b.txt.recast")
                && !n.starts_with(".c.txt.recast")
        })
        .collect();
    let originals: std::collections::HashSet<_> = names.into_iter().collect();
    assert!(originals.contains("a.txt"));
    assert!(originals.contains("b.txt"));
    assert!(originals.contains("c.txt"));
}

#[test]
fn rollback_leaves_originals_when_stage_fails() {
    let dir = TempDir::new().unwrap();
    let a = dir.path().join("a.txt");
    fs::write(&a, "Old\n").unwrap();
    let plan = plan_rewrite("Old", "New", &[dir.path()], &PlanOptions::default()).unwrap();
    let outcome = apply_inner(&plan, |_| Ok(())).unwrap();
    assert_eq!(outcome.files_written, 1);
    assert_eq!(fs::read_to_string(&a).unwrap(), "New\n");
}

#[test]
fn recover_deletes_orphan_temp_when_target_present() {
    let dir = TempDir::new().unwrap();
    let a = dir.path().join("a.txt");
    let tmp = dir.path().join(".a.txt.recast.tmp.99");
    fs::write(&a, "Original\n").unwrap();
    fs::write(&tmp, "Staged but never committed\n").unwrap();
    let summary = recover_sweep(&[dir.path()]).unwrap();
    assert_eq!(summary.temps_removed, 1);
    assert_eq!(summary.backups_restored, 0);
    assert!(!tmp.exists());
    assert_eq!(fs::read_to_string(&a).unwrap(), "Original\n");
}

#[test]
fn recover_restores_backup_when_target_missing() {
    let dir = TempDir::new().unwrap();
    let a = dir.path().join("a.txt");
    let bak = dir.path().join(".a.txt.recast.bak.42");
    let tmp = dir.path().join(".a.txt.recast.tmp.42");
    fs::write(&bak, "Original\n").unwrap();
    fs::write(&tmp, "New content (was about to land)\n").unwrap();
    assert!(!a.exists());
    let summary = recover_sweep(&[dir.path()]).unwrap();
    assert_eq!(summary.backups_restored, 1);
    assert_eq!(summary.temps_removed, 1);
    assert!(!bak.exists());
    assert!(!tmp.exists());
    assert_eq!(fs::read_to_string(&a).unwrap(), "Original\n");
}

#[test]
fn recover_deletes_orphan_backup_when_target_present() {
    let dir = TempDir::new().unwrap();
    let a = dir.path().join("a.txt");
    let bak = dir.path().join(".a.txt.recast.bak.7");
    fs::write(&a, "Committed\n").unwrap();
    fs::write(&bak, "Stale backup\n").unwrap();
    let summary = recover_sweep(&[dir.path()]).unwrap();
    assert_eq!(summary.backups_removed, 1);
    assert_eq!(summary.backups_restored, 0);
    assert!(!bak.exists());
    assert_eq!(fs::read_to_string(&a).unwrap(), "Committed\n");
}

#[test]
fn recover_ignores_unrelated_files() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("README.md"), "hi\n").unwrap();
    fs::write(dir.path().join(".gitignore"), "target/\n").unwrap();
    fs::write(dir.path().join(".env.bak"), "irrelevant\n").unwrap();
    let summary = recover_sweep(&[dir.path()]).unwrap();
    assert_eq!(summary.backups_restored, 0);
    assert_eq!(summary.backups_removed, 0);
    assert_eq!(summary.temps_removed, 0);
    assert!(dir.path().join(".env.bak").exists());
}

#[test]
fn recover_picks_newest_backup_nonce_when_multiple() {
    let dir = TempDir::new().unwrap();
    let a = dir.path().join("a.txt");
    let bak_old = dir.path().join(".a.txt.recast.bak.1");
    let bak_new = dir.path().join(".a.txt.recast.bak.999");
    fs::write(&bak_old, "older snapshot\n").unwrap();
    fs::write(&bak_new, "newer snapshot\n").unwrap();
    assert!(!a.exists());
    let summary = recover_sweep(&[dir.path()]).unwrap();
    assert_eq!(summary.backups_restored, 1);
    assert_eq!(summary.backups_removed, 1);
    assert_eq!(fs::read_to_string(&a).unwrap(), "newer snapshot\n");
}

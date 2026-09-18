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
fn plan_zero_matches_violates_default_guard() {
    let dir = fixture(&[("a.txt", "unrelated\n")]);
    let err = plan_rewrite("Zzz", "Q", &[dir.path()], &PlanOptions::default()).unwrap_err();
    assert!(matches!(err, Error::TooFewMatches { found: 0, required: 1 }), "{err:?}");
}

#[test]
fn plan_rerun_on_converted_tree_violates_default_guard() {
    let dir = fixture(&[("a.txt", "New name\n")]);
    let err = plan_rewrite("Old", "New", &[dir.path()], &PlanOptions::default()).unwrap_err();
    assert!(matches!(err, Error::TooFewMatches { found: 0, required: 1 }), "{err:?}");
}

#[test]
fn plan_zero_matches_honors_explicit_at_least() {
    let dir = fixture(&[("a.txt", "unrelated\n")]);
    let opts = PlanOptions { at_least: Some(2), ..Default::default() };
    let err = plan_rewrite("Zzz", "Q", &[dir.path()], &opts).unwrap_err();
    assert!(matches!(err, Error::TooFewMatches { found: 0, required: 2 }), "{err:?}");
}

#[test]
fn plan_zero_matches_allowed_when_guard_disabled() {
    let dir = fixture(&[("a.txt", "unrelated\n")]);
    let opts = PlanOptions { at_least: None, ..Default::default() };
    let plan = plan_rewrite("Zzz", "Q", &[dir.path()], &opts).unwrap();
    assert_eq!(plan.outcome, PlanOutcome::AlreadyApplied);
}

#[test]
fn plan_zero_matches_is_already_applied_for_non_convergent_pattern() {
    let dir = fixture(&[("a.txt", "unrelated\n")]);
    let opts = PlanOptions { at_least: Some(0), ..Default::default() };
    let plan = plan_rewrite("Zzz", "ZzzZzz", &[dir.path()], &opts).unwrap();
    assert_eq!(plan.outcome, PlanOutcome::AlreadyApplied);
    assert_eq!(plan.total_matches, 0);
}

#[cfg(feature = "script")]
#[test]
fn scripted_plan_zero_matches_violates_default_guard() {
    let dir = fixture(&[("a.txt", "unrelated\n")]);
    let script = crate::script::ScriptRewriter::from_source(r#""Q""#).unwrap();
    let err =
        plan_rewrite_scripted("Zzz", &script, &[dir.path()], &PlanOptions::default()).unwrap_err();
    assert!(matches!(err, Error::TooFewMatches { found: 0, required: 1 }), "{err:?}");
}

fn literal_opts() -> PlanOptions {
    PlanOptions {
        pattern_options: PatternOptions { literal: true, ..Default::default() },
        ..Default::default()
    }
}

#[test]
fn plan_rejects_non_convergent_pattern() {
    let dir = fixture(&[("a.txt", "abc\n")]);
    let err = plan_rewrite("a", "aa", &[dir.path()], &PlanOptions::default()).unwrap_err();
    assert!(matches!(err, Error::NonConvergentReplacement { .. }), "{err:?}");
}

#[test]
fn non_convergent_offers_the_override_as_a_typed_remedy() {
    use crate::error::Remedy;
    let dir = fixture(&[("a.txt", "abc\n")]);
    let err = plan_rewrite("a", "aa", &[dir.path()], &PlanOptions::default()).unwrap_err();
    assert!(err.remedies().contains(&Remedy::AllowNonConvergent), "{err:?}");
}

#[test]
fn the_two_non_convergence_causes_differ_in_message_and_in_remedy() {
    use crate::error::Remedy;
    let by_replacement = fixture(&[("a.txt", "let x = Outcome;\n")]);
    let replacement =
        plan_rewrite("Outcome", "ReadOutcome", &[by_replacement.path()], &literal_opts())
            .unwrap_err();

    let by_context = fixture(&[("b.txt", "aabb\n")]);
    let context =
        plan_rewrite("ab", "a", &[by_context.path()], &PlanOptions::default()).unwrap_err();

    assert_ne!(
        replacement.to_string(),
        context.to_string(),
        "both causes still print the same advice"
    );
    // Word boundaries fix the first and cannot fix the second, so the
    // remedies must differ too, not just the prose.
    assert!(replacement.remedies().contains(&Remedy::Word), "{replacement:?}");
    assert!(!context.remedies().contains(&Remedy::Word), "{context:?}");
}

#[test]
fn non_convergent_replacement_blames_the_replacement() {
    let dir = fixture(&[("a.txt", "let x = Outcome;\n")]);
    let err = plan_rewrite("Outcome", "ReadOutcome", &[dir.path()], &literal_opts()).unwrap_err();
    assert!(matches!(err, Error::NonConvergentReplacement { .. }), "{err:?}");
    assert!(err.to_string().contains("replacement"), "{err}");
}

#[test]
fn non_convergent_context_clears_the_replacement() {
    let dir = fixture(&[("a.txt", "aabb\n")]);
    let err = plan_rewrite("ab", "a", &[dir.path()], &PlanOptions::default()).unwrap_err();
    assert!(matches!(err, Error::NonConvergentContext { .. }), "{err:?}");
    assert!(err.to_string().contains("surrounding"), "{err}");
}

#[cfg(feature = "script")]
#[test]
fn non_convergent_script_is_not_blamed_on_a_static_replacement() {
    let dir = fixture(&[("a.txt", "a\n")]);
    let script = crate::script::ScriptRewriter::from_source(r#""aa""#).unwrap();
    let err =
        plan_rewrite_scripted("a", &script, &[dir.path()], &PlanOptions::default()).unwrap_err();
    assert!(matches!(err, Error::NonConvergentScript { .. }), "{err:?}");
    assert!(err.remedies().contains(&crate::error::Remedy::AllowNonConvergent), "{err:?}");
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

#[cfg(feature = "lang-rust")]
#[test]
fn plan_rejects_syntax_regression() {
    let dir = fixture(&[("a.rs", "fn a() {\n    work();\n}\nfn b() {}\n")]);
    let err =
        plan_rewrite(r"fn a\(\) \{\n", "", &[dir.path()], &PlanOptions::default()).unwrap_err();
    assert!(
        matches!(err, Error::SyntaxRegression { new_errors, .. } if new_errors >= 1),
        "{err:?}"
    );
}

#[cfg(feature = "lang-rust")]
#[test]
fn plan_allows_syntax_regression_with_override() {
    let dir = fixture(&[("a.rs", "fn a() {\n    work();\n}\nfn b() {}\n")]);
    let mut opts = PlanOptions::default();
    opts.allow_syntax_errors = true;
    let plan = plan_rewrite(r"fn a\(\) \{\n", "", &[dir.path()], &opts).unwrap();
    assert_eq!(plan.outcome, PlanOutcome::Changes);
}

#[cfg(feature = "lang-rust")]
#[test]
fn plan_clean_delete_passes_guard() {
    let dir = fixture(&[("a.rs", "fn a() {}\nfn b() {}\n")]);
    let plan =
        plan_rewrite("fn a\\(\\) \\{\\}\n", "", &[dir.path()], &PlanOptions::default()).unwrap();
    assert_eq!(plan.outcome, PlanOutcome::Changes);
}

#[cfg(feature = "lang-rust")]
#[test]
fn plan_orphan_attr_passes_guard_known_limitation() {
    let dir = fixture(&[("a.rs", "#[test]\nfn a() {}\nfn b() {}\n")]);
    let plan =
        plan_rewrite("fn a\\(\\) \\{\\}\n", "", &[dir.path()], &PlanOptions::default()).unwrap();
    assert_eq!(plan.outcome, PlanOutcome::Changes);
}

#[test]
fn plan_syntax_guard_skips_non_grammar_extension() {
    let dir = fixture(&[("a.txt", "fn a() {\n    work();\n}\n")]);
    let plan = plan_rewrite(r"fn a\(\) \{\n", "", &[dir.path()], &PlanOptions::default()).unwrap();
    assert_eq!(plan.outcome, PlanOutcome::Changes);
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

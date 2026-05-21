#![allow(clippy::unwrap_used)]

use super::*;

#[test]
fn literal_mode_escapes_metacharacters() {
    let p = CompiledPattern::compile(
        "a.b",
        "X",
        &PatternOptions { literal: true, ..Default::default() },
    )
    .unwrap();
    assert!(p.regex().is_match("a.b"));
    assert!(!p.regex().is_match("aXb"));
}

#[test]
fn convergent_rewrite_is_detected() {
    let p = CompiledPattern::compile("Old", "New", &PatternOptions::default()).unwrap();
    assert!(p.is_convergent());
}

#[test]
fn non_convergent_rewrite_is_detected() {
    let p = CompiledPattern::compile("a", "aa", &PatternOptions::default()).unwrap();
    assert!(!p.is_convergent());
}

#[test]
fn capture_group_in_replacement_does_not_break_convergence_probe() {
    let p = CompiledPattern::compile(r"foo(\d+)", "bar$1", &PatternOptions::default()).unwrap();
    assert!(p.is_convergent());
}

#[test]
fn dot_matches_newline_by_default() {
    let p = CompiledPattern::compile("a.b", "X", &PatternOptions::default()).unwrap();
    assert!(p.regex().is_match("a\nb"));
}

#[test]
fn convergence_probe_preserves_non_ascii_replacement() {
    // Regression: replacement_probe walked bytes and pushed each as
    // `char`, which corrupted multibyte UTF-8. The probe `foo` → `baré`
    // must yield a `baré` string that the regex `foo` cannot re-match,
    // i.e. the rewrite is convergent.
    let p = CompiledPattern::compile("foo", "baré", &PatternOptions::default()).unwrap();
    assert!(p.is_convergent());
}

#[test]
fn single_line_flag_disables_dotall() {
    let p = CompiledPattern::compile(
        "a.b",
        "X",
        &PatternOptions { single_line: true, ..Default::default() },
    )
    .unwrap();
    assert!(!p.regex().is_match("a\nb"));
}

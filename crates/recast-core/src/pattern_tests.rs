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
fn replacement_containing_the_pattern_is_statically_non_convergent() {
    let p = CompiledPattern::compile(
        "Outcome",
        "ReadOutcome",
        &PatternOptions { literal: true, ..Default::default() },
    )
    .unwrap();
    assert!(!p.is_convergent());
}

#[test]
fn context_driven_regrowth_is_invisible_to_the_static_probe() {
    let p = CompiledPattern::compile("ab", "a", &PatternOptions::default()).unwrap();
    assert!(p.is_convergent());
}

#[test]
fn capture_group_in_replacement_does_not_break_convergence_probe() {
    let p = CompiledPattern::compile(r"foo(\d+)", "bar$1", &PatternOptions::default()).unwrap();
    assert!(p.is_convergent());
}

fn word_opts() -> PatternOptions {
    PatternOptions { word: true, ..Default::default() }
}

#[test]
fn word_option_does_not_match_inside_a_longer_word() {
    let p = CompiledPattern::compile("foo", "X", &word_opts()).unwrap();
    assert!(!p.regex().is_match("foobar"));
    assert!(!p.regex().is_match("barfoo"));
    assert!(p.regex().is_match("foo bar"));
}

#[test]
fn word_option_uses_half_boundaries_not_plain_word_boundaries() {
    let haystack = "x -foo- y";
    let plain_b =
        CompiledPattern::compile(r"\b(?:-foo-)\b", "X", &PatternOptions::default()).unwrap();
    assert!(!plain_b.regex().is_match(haystack));

    let half = CompiledPattern::compile("-foo-", "X", &word_opts()).unwrap();
    assert!(half.regex().is_match(haystack));
}

#[test]
fn word_option_groups_alternation() {
    let p = CompiledPattern::compile("foo|bar", "X", &word_opts()).unwrap();
    assert!(p.regex().is_match("a bar b"));
    assert!(!p.regex().is_match("xbarx"));
}

#[test]
fn word_option_applies_after_literal_escaping() {
    let p = CompiledPattern::compile(
        "a.b",
        "X",
        &PatternOptions { literal: true, word: true, ..Default::default() },
    )
    .unwrap();
    assert!(p.regex().is_match("a.b"));
    assert!(!p.regex().is_match("axb"));
    assert!(!p.regex().is_match("za.bz"));
}

#[test]
fn word_option_makes_a_prefix_rename_convergent() {
    let plain = CompiledPattern::compile(
        "Outcome",
        "ReadOutcome",
        &PatternOptions { literal: true, ..Default::default() },
    )
    .unwrap();
    assert!(!plain.is_convergent());

    let worded = CompiledPattern::compile(
        "Outcome",
        "ReadOutcome",
        &PatternOptions { literal: true, word: true, ..Default::default() },
    )
    .unwrap();
    assert!(worded.is_convergent());
}

#[test]
fn word_option_preserves_capture_group_numbering() {
    let p = CompiledPattern::compile(r"(\w+)_old", "${1}_new", &word_opts()).unwrap();
    let caps = p.regex().captures("call foo_old here").unwrap();
    assert_eq!(&caps[1], "foo");
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

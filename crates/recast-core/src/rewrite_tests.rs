#![allow(clippy::unwrap_used)]

use super::*;
use crate::pattern::{CompiledPattern, PatternOptions};

#[test]
fn rewrite_counts_matches_and_replaces() {
    let p = CompiledPattern::compile("foo", "bar", &PatternOptions::default()).unwrap();
    let r = rewrite_text(&p, "foo and foo");
    assert_eq!(r.matches, 2);
    assert_eq!(r.after, "bar and bar");
    assert!(r.changed());
}

#[test]
fn rewrite_with_no_match_is_unchanged() {
    let p = CompiledPattern::compile("foo", "bar", &PatternOptions::default()).unwrap();
    let r = rewrite_text(&p, "nothing here");
    assert_eq!(r.matches, 0);
    assert!(!r.changed());
}

#[test]
fn unified_diff_has_file_header_and_hunk() {
    let d = unified_diff("a.txt", "alpha\n", "beta\n");
    assert!(d.contains("--- a/a.txt"));
    assert!(d.contains("+++ b/a.txt"));
    assert!(d.contains("-alpha"));
    assert!(d.contains("+beta"));
}

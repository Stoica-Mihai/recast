#![allow(clippy::unwrap_used)]

use std::path::Path;

use insta::assert_snapshot;

use super::*;
use crate::pattern::{CompiledPattern, PatternOptions};

#[test]
fn label_strips_leading_curdir() {
    assert_eq!(label_for_path(Path::new("./src/a.rs")), "src/a.rs");
}

#[test]
fn label_keeps_absolute_paths() {
    assert_eq!(label_for_path(Path::new("/tmp/x.rs")), "/tmp/x.rs");
}

#[test]
fn label_keeps_plain_relative_paths() {
    assert_eq!(label_for_path(Path::new("src/a.rs")), "src/a.rs");
}

#[test]
fn label_strips_only_one_leading_curdir() {
    assert_eq!(label_for_path(Path::new("./././x.rs")), "x.rs");
}

#[test]
fn label_for_curdir_alone_is_dot() {
    assert_eq!(label_for_path(Path::new(".")), ".");
}

#[test]
fn rewrite_counts_matches_and_replaces() {
    let p = CompiledPattern::compile("foo", "bar", &PatternOptions::default()).unwrap();
    let r = rewrite_text(&p, "foo and foo");
    assert_eq!(r.matches, 2);
    assert_eq!(r.after, "bar and bar");
}

#[test]
fn rewrite_with_no_match_is_unchanged() {
    let p = CompiledPattern::compile("foo", "bar", &PatternOptions::default()).unwrap();
    let input = "nothing here";
    let r = rewrite_text(&p, input);
    assert_eq!(r.matches, 0);
    assert_eq!(r.after, input);
}

#[test]
fn unified_diff_has_file_header_and_hunk() {
    let d = unified_diff("a.txt", "alpha\n", "beta\n");
    assert!(d.contains("--- a/a.txt"));
    assert!(d.contains("+++ b/a.txt"));
    assert!(d.contains("-alpha"));
    assert!(d.contains("+beta"));
}

#[test]
fn diff_snapshot_single_line_change() {
    let before = "fn OldName() {}\n";
    let after = "fn NewName() {}\n";
    assert_snapshot!(unified_diff("src/lib.rs", before, after));
}

#[test]
fn diff_snapshot_multi_hunk() {
    let before = "line 1\nline 2 OldName\nline 3\nline 4\nline 5\nline 6\nline 7\nline 8 OldName\nline 9\nline 10\n";
    let after = "line 1\nline 2 NewName\nline 3\nline 4\nline 5\nline 6\nline 7\nline 8 NewName\nline 9\nline 10\n";
    assert_snapshot!(unified_diff("src/big.rs", before, after));
}

#[test]
fn diff_snapshot_no_trailing_newline() {
    let before = "alpha";
    let after = "beta";
    assert_snapshot!(unified_diff("nonl.txt", before, after));
}

#[test]
fn diff_snapshot_added_lines_only() {
    let before = "header\n";
    let after = "header\nbody1\nbody2\n";
    assert_snapshot!(unified_diff("growing.txt", before, after));
}

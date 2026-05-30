#![allow(clippy::unwrap_used)]

use super::*;

#[test]
fn truncate_snippet_single_line() {
    assert_eq!(truncate_snippet("hello world"), "hello world");
}

#[test]
fn truncate_snippet_stops_at_newline() {
    assert_eq!(truncate_snippet("first line\nsecond line"), "first line");
}

#[test]
fn truncate_snippet_caps_at_200_chars() {
    let long = "a".repeat(250);
    let result = truncate_snippet(&long);
    assert_eq!(result.len(), 200);
}

#[test]
fn truncate_snippet_strips_whitespace() {
    assert_eq!(truncate_snippet("  hello  \n"), "hello");
}

#[test]
fn line_col_first_line() {
    assert_eq!(line_col("hello world", 0), (1, 1));
    assert_eq!(line_col("hello world", 6), (1, 7));
}

#[test]
fn line_col_second_line() {
    assert_eq!(line_col("hello\nworld", 6), (2, 1));
    assert_eq!(line_col("hello\nworld", 8), (2, 3));
}

#[test]
fn line_col_third_line() {
    assert_eq!(line_col("a\nb\nc", 4), (3, 1));
}

#[test]
fn plan_search_finds_matches_across_files() {
    use crate::search::{SearchOptions, plan_search};
    use std::fs;
    use tempfile::TempDir;

    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("a.txt"), "foo bar foo\n").unwrap();
    fs::write(dir.path().join("b.txt"), "baz\n").unwrap();

    let mut opts = SearchOptions::default();
    opts.at_least = Some(0);
    let plan = plan_search("foo", &[dir.path()], &opts).unwrap();

    assert_eq!(plan.total_matches, 2);
    assert_eq!(plan.files.len(), 1);
    assert_eq!(plan.files[0].matches.len(), 2);
    assert_eq!(plan.files[0].matches[0].line, 1);
    assert_eq!(plan.files[0].matches[0].column, 1);
    assert_eq!(plan.files[0].matches[0].snippet, "foo");
    assert!(plan.files[0].matches[0].capture.is_none());
    assert_eq!(plan.files_scanned, 2);
}

#[test]
fn plan_search_guard_fires_on_no_matches() {
    use crate::error::Error;
    use crate::search::{SearchOptions, plan_search};
    use std::fs;
    use tempfile::TempDir;

    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("a.txt"), "baz\n").unwrap();

    let plan = plan_search("foo", &[dir.path()], &SearchOptions::default());
    assert!(matches!(plan.unwrap_err(), Error::TooFewMatches { found: 0, required: 1 }));
}

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
    // "hello\nworld" — 'w' is at byte 6, line 2 col 1
    assert_eq!(line_col("hello\nworld", 6), (2, 1));
    // 'o' is at byte 8, line 2 col 3
    assert_eq!(line_col("hello\nworld", 8), (2, 3));
}

#[test]
fn line_col_third_line() {
    assert_eq!(line_col("a\nb\nc", 4), (3, 1));
}

#![allow(clippy::unwrap_used)]

use super::*;
use crate::error::Error;

#[test]
fn rename_identifier_via_query() {
    let source = "fn old_name() { old_name(); }";
    let out = structural_rewrite(
        Language::Rust,
        source,
        r#"((identifier) @id (#eq? @id "old_name"))"#,
        "new_name",
    )
    .unwrap();
    assert_eq!(out.text, "fn new_name() { new_name(); }");
    assert_eq!(out.matches, 2);
}

#[test]
fn template_substitutes_captures() {
    let source = "fn foo() {}\nfn bar() {}";
    let out = structural_rewrite(
        Language::Rust,
        source,
        r#"(function_item name: (identifier) @name) @root"#,
        "fn ${name}_renamed() {}",
    )
    .unwrap();
    assert_eq!(out.text, "fn foo_renamed() {}\nfn bar_renamed() {}");
    assert_eq!(out.matches, 2);
}

#[test]
fn unknown_capture_name_in_template_is_error() {
    let source = "fn foo() {}";
    let err = structural_rewrite(
        Language::Rust,
        source,
        r#"(function_item name: (identifier) @name) @root"#,
        "fn ${nonexistent}() {}",
    )
    .unwrap_err();
    assert!(matches!(err, Error::StructuralTemplate(_)));
}

#[test]
fn invalid_query_returns_query_error() {
    let source = "fn foo() {}";
    let err = structural_rewrite(Language::Rust, source, "(((((", "irrelevant").unwrap_err();
    assert!(matches!(err, Error::StructuralQuery(_)));
}

#[test]
fn no_matches_returns_unchanged_source() {
    let source = "fn foo() {}";
    let out =
        structural_rewrite(Language::Rust, source, r#"((identifier) @id (#eq? @id "zzz"))"#, "bar")
            .unwrap();
    assert_eq!(out.text, source);
    assert_eq!(out.matches, 0);
}

#[test]
fn overlapping_matches_are_skipped() {
    let source = "fn a() {}";
    let out =
        structural_rewrite(Language::Rust, source, r#"(function_item) @root"#, "EMPTY").unwrap();
    assert_eq!(out.text, "EMPTY");
    assert_eq!(out.matches, 1);
}

#[test]
fn language_from_name_parses_rust() {
    assert!(matches!(Language::from_name("rust"), Some(Language::Rust)));
    assert!(matches!(Language::from_name("Rust"), Some(Language::Rust)));
    assert!(Language::from_name("zzz").is_none());
}

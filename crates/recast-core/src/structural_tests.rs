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
fn query_error_includes_line_column_and_caret() {
    let err = structural_rewrite(
        Language::Rust,
        "fn foo() {}",
        "(not_a_real_node_kind) @x",
        "irrelevant",
    )
    .unwrap_err();
    let Error::StructuralQuery(msg) = err else { panic!("wrong error variant: {err:?}") };
    assert!(msg.contains("line 1"), "no line info: {msg}");
    assert!(msg.contains("column"), "no column info: {msg}");
    assert!(msg.contains("^"), "no caret: {msg}");
    assert!(
        msg.contains("not_a_real_node_kind") || msg.contains("unknown node type"),
        "no useful detail: {msg}"
    );
}

#[test]
fn unparseable_ast_pattern_mentions_substitution() {
    let err =
        structural_rewrite_friendly(Language::Rust, "fn foo() {}", "fn $$$ broken", "irrelevant")
            .unwrap_err();
    let Error::StructuralQuery(msg) = err else { panic!("wrong error variant: {err:?}") };
    assert!(msg.contains("`--ast` pattern"), "no --ast hint: {msg}");
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
    assert!(matches!(Language::from_name("rust"), Ok(Language::Rust)));
    assert!(matches!(Language::from_name("Rust"), Ok(Language::Rust)));
    assert!(matches!(Language::from_name("zzz"), Err(Error::UnknownLanguage(_))));
}

#[cfg(feature = "lang-ts")]
#[test]
fn typescript_rename_function() {
    let source = "function oldName(): void {}\nconst x = oldName();";
    let out = structural_rewrite(
        Language::TypeScript,
        source,
        r#"((identifier) @id (#eq? @id "oldName"))"#,
        "newName",
    )
    .unwrap();
    assert!(out.text.contains("function newName"));
    assert!(out.text.contains("newName()"));
    assert_eq!(out.matches, 2);
}

#[cfg(feature = "lang-js")]
#[test]
fn javascript_rename_identifier() {
    let source = "const oldName = 1;\nconsole.log(oldName);";
    let out = structural_rewrite(
        Language::JavaScript,
        source,
        r#"((identifier) @id (#eq? @id "oldName"))"#,
        "newName",
    )
    .unwrap();
    assert!(out.text.contains("const newName"));
    assert!(out.text.contains("console.log(newName)"));
    assert_eq!(out.matches, 2);
}

#[cfg(feature = "lang-bash")]
#[test]
fn bash_rename_identifier() {
    let source = "old_var=1\necho $old_var\n";
    let out = structural_rewrite(
        Language::Bash,
        source,
        r#"((variable_name) @id (#eq? @id "old_var"))"#,
        "new_var",
    )
    .unwrap();
    assert!(out.text.contains("new_var=1"));
    assert!(out.text.contains("$new_var"));
}

#[cfg(feature = "lang-go")]
#[test]
fn go_rename_function() {
    let source = "package main\n\nfunc oldFn() int { return 1 }\n\nfunc main() { oldFn() }\n";
    let out = structural_rewrite(
        Language::Go,
        source,
        r#"((identifier) @id (#eq? @id "oldFn"))"#,
        "newFn",
    )
    .unwrap();
    assert!(out.text.contains("func newFn"));
    assert!(out.text.contains("newFn()"));
}

#[cfg(feature = "lang-json")]
#[test]
fn json_rename_string_value() {
    let source = r#"{"name": "old", "kind": "thing"}"#;
    let out = structural_rewrite(
        Language::Json,
        source,
        r#"((string_content) @s (#eq? @s "old"))"#,
        "new",
    )
    .unwrap();
    assert_eq!(out.text, r#"{"name": "new", "kind": "thing"}"#);
}

#[cfg(feature = "lang-md")]
#[test]
fn markdown_rewrite_inline_text() {
    let source = "# Hello\n\nThis is foo. Replace foo.\n";
    let out =
        structural_rewrite(Language::Markdown, source, r#"((inline) @line)"#, "REPLACED").unwrap();
    assert!(out.matches >= 1);
}

#[cfg(feature = "lang-python")]
#[test]
fn python_rename_function() {
    let source = "def old_one():\n    return 1\n\nold_one()\n";
    let out = structural_rewrite(
        Language::Python,
        source,
        r#"((identifier) @id (#eq? @id "old_one"))"#,
        "new_one",
    )
    .unwrap();
    assert!(out.text.contains("def new_one"));
    assert!(out.text.contains("\nnew_one()"));
    assert_eq!(out.matches, 2);
}

#[test]
fn friendly_pattern_renames_function() {
    let source = "fn old_one() {}\nfn other() { old_one(); }";
    let out =
        structural_rewrite_friendly(Language::Rust, source, "fn old_one() {}", "fn new_one() {}")
            .unwrap();
    assert_eq!(out.text, "fn new_one() {}\nfn other() { old_one(); }");
    assert_eq!(out.matches, 1);
}

#[test]
fn friendly_pattern_captures_metavar() {
    let source = "fn foo() {}\nfn bar() {}";
    let out =
        structural_rewrite_friendly(Language::Rust, source, "fn $NAME() {}", "fn ${NAME}_v2() {}")
            .unwrap();
    assert_eq!(out.text, "fn foo_v2() {}\nfn bar_v2() {}");
    assert_eq!(out.matches, 2);
}

#[test]
fn friendly_pattern_unparseable_returns_query_error() {
    let err =
        structural_rewrite_friendly(Language::Rust, "fn foo() {}", "fn $$$ broken", "irrelevant")
            .unwrap_err();
    assert!(matches!(err, Error::StructuralQuery(_)));
}

#[test]
fn friendly_pattern_ellipsis_matches_any_args_and_body() {
    let source = "fn foo() {}\nfn bar(x: u32, y: u32) { println!(\"hi {x} {y}\"); }\n";
    // Ellipsis captures the whole wrapper node (parens or braces
    // included), so the template must NOT re-add `(...)` / `{...}`
    // around `$ARGS` / `$BODY`.
    let out = structural_rewrite_friendly(
        Language::Rust,
        source,
        "fn $NAME($$$ARGS) { $$$BODY }",
        "fn ${NAME}_v2$ARGS $BODY",
    )
    .unwrap();
    assert_eq!(out.matches, 2);
    assert!(out.text.contains("fn foo_v2()"), "got: {}", out.text);
    assert!(out.text.contains("fn bar_v2(x: u32, y: u32)"), "got: {}", out.text);
    assert!(out.text.contains("println!(\"hi {x} {y}\")"), "got: {}", out.text);
}

#[test]
fn friendly_pattern_ellipsis_preserves_body_text() {
    let source = "fn greet() { println!(\"hi\"); let n = 1 + 2; }\n";
    let out = structural_rewrite_friendly(
        Language::Rust,
        source,
        "fn $NAME() { $$$BODY }",
        "fn ${NAME}_renamed() $BODY",
    )
    .unwrap();
    assert_eq!(out.matches, 1);
    assert!(out.text.contains("fn greet_renamed()"));
    assert!(out.text.contains("println!(\"hi\")"));
    assert!(out.text.contains("let n = 1 + 2"));
}

#[test]
fn template_preserves_non_ascii_literal_bytes() {
    // Regression: the template scanner walked bytes and pushed each as
    // `char`, which mojibaked every multibyte UTF-8 sequence. With the
    // fix the literal `é` (0xC3 0xA9) round-trips unchanged.
    let source = "fn foo() {}";
    let out = structural_rewrite(
        Language::Rust,
        source,
        r#"(function_item name: (identifier) @name) @root"#,
        "fn ${name}_é() {}",
    )
    .unwrap();
    assert_eq!(out.text, "fn foo_é() {}");
    assert_eq!(out.matches, 1);
}

#[test]
fn ast_pattern_preserves_non_ascii_literal_bytes() {
    // Regression for the byte-walker in `substitute_metavars`: a literal
    // non-ASCII codepoint in an `--ast` pattern must survive the
    // metavar-substitution preprocess intact.
    let source = "// é\nfn foo() {}";
    let out = structural_rewrite_friendly(
        Language::Rust,
        source,
        "fn $NAME() {}",
        "fn ${NAME}_é() {}",
    )
    .unwrap();
    assert_eq!(out.text, "// é\nfn foo_é() {}");
    assert_eq!(out.matches, 1);
}

#[test]
fn friendly_pattern_no_matches_leaves_source_intact() {
    let source = "fn foo() {}";
    let out = structural_rewrite_friendly(
        Language::Rust,
        source,
        "struct $NAME {}",
        "struct ${NAME}V2 {}",
    )
    .unwrap();
    assert_eq!(out.text, source);
    assert_eq!(out.matches, 0);
}

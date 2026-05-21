#![allow(clippy::unwrap_used)]

use super::*;
use crate::error::Error;

#[test]
fn script_returns_uppercased_capture() {
    let s = ScriptRewriter::from_source("captures[1].to_upper()").unwrap();
    let r = s.replace(&["foo bar", "bar"]).unwrap();
    assert_eq!(r, "BAR");
}

#[test]
fn script_increments_numeric_capture() {
    let s = ScriptRewriter::from_source("(parse_int(captures[1]) + 1).to_string()").unwrap();
    let r = s.replace(&["v 3", "3"]).unwrap();
    assert_eq!(r, "4");
}

#[test]
fn script_can_access_full_match_var() {
    let s = ScriptRewriter::from_source(r#"`<${whole}>`"#).unwrap();
    let r = s.replace(&["hello", "hello"]).unwrap();
    assert_eq!(r, "<hello>");
}

#[test]
fn script_with_invalid_syntax_returns_parse_error() {
    let err = ScriptRewriter::from_source("@@@").unwrap_err();
    assert!(matches!(err, Error::ScriptParse(_)));
}

#[test]
fn script_with_runtime_panic_returns_runtime_error() {
    let s = ScriptRewriter::from_source("captures[999]").unwrap();
    let err = s.replace(&["x"]).unwrap_err();
    assert!(matches!(err, Error::ScriptRuntime(_)));
}

#[test]
fn script_conditional_replacement() {
    let src = r#"
        if captures[1] == "old" { "new" } else { captures[1] }
    "#;
    let s = ScriptRewriter::from_source(src).unwrap();
    assert_eq!(s.replace(&["old", "old"]).unwrap(), "new");
    assert_eq!(s.replace(&["keep", "keep"]).unwrap(), "keep");
}

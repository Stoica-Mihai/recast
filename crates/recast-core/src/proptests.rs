//! Property tests for the recast engine. Goal: prove that every public
//! entry point either succeeds or returns a typed [`crate::Error`] when
//! handed arbitrary input — no panics, no aborts, no infinite loops on
//! pathological data.

#![allow(clippy::unwrap_used)]

use std::path::Path;

use proptest::prelude::*;

use crate::pattern::{CompiledPattern, PatternOptions};
use crate::rewrite::{label_for_path, rewrite_text};

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 96,
        ..ProptestConfig::default()
    })]

    #[test]
    fn pattern_compile_never_panics_on_arbitrary_text(
        pattern in ".{0,32}",
        replacement in ".{0,32}",
        literal in any::<bool>(),
        ignore_case in any::<bool>(),
        single_line in any::<bool>(),
    ) {
        let opts = PatternOptions { literal, ignore_case, single_line };
        let _ = CompiledPattern::compile(&pattern, &replacement, &opts);
    }

    #[test]
    fn rewrite_text_with_literal_pattern_never_panics(
        pattern in ".{0,16}",
        replacement in ".{0,16}",
        input in ".{0,128}",
    ) {
        let opts = PatternOptions { literal: true, ..Default::default() };
        if let Ok(compiled) = CompiledPattern::compile(&pattern, &replacement, &opts) {
            let outcome = rewrite_text(&compiled, &input);
            prop_assert_eq!(outcome.before, input);
        }
    }

    #[test]
    fn label_for_path_never_panics_and_never_yields_curdir_prefix(
        s in proptest::string::string_regex("[./a-zA-Z0-9_-]{0,64}").unwrap(),
    ) {
        let label = label_for_path(Path::new(&s));
        prop_assert!(!label.starts_with("./"));
        prop_assert!(!label.is_empty());
    }
}

#[cfg(feature = "script")]
proptest! {
    #![proptest_config(ProptestConfig {
        cases: 48,
        ..ProptestConfig::default()
    })]

    #[test]
    fn script_compile_never_panics_on_arbitrary_source(
        source in ".{0,128}",
    ) {
        let _ = crate::script::ScriptRewriter::from_source(&source);
    }
}

#[cfg(feature = "lang-rust")]
proptest! {
    #![proptest_config(ProptestConfig {
        cases: 48,
        ..ProptestConfig::default()
    })]

    #[test]
    fn compile_friendly_query_never_panics_on_arbitrary_rust(
        pattern in ".{0,96}",
    ) {
        let _ = crate::structural::compile_friendly_query(crate::structural::Language::Rust, &pattern);
    }

    #[test]
    fn structural_rewrite_with_garbage_query_returns_err(
        source in ".{0,128}",
        query in proptest::string::string_regex("[^()@a-z ]{0,32}").unwrap(),
        template in ".{0,32}",
    ) {
        // The query is intentionally garbage; the function must either
        // return an Err or, if the parser happens to accept it as an
        // empty query, return Ok with zero matches. Either way, no panic.
        let _ = crate::structural::structural_rewrite(
            crate::structural::Language::Rust,
            &source,
            &query,
            &template,
        );
    }
}

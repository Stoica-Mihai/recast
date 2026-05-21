#![no_main]
//! Fuzz the friendly `--ast` pattern compiler. The compiler walks an
//! arbitrary user-supplied pattern, substitutes metavars, parses with
//! tree-sitter, and emits an S-expression query. None of that should
//! panic, allocate without bound, or recurse without bound on
//! adversarial input.

use libfuzzer_sys::fuzz_target;
use recast_core::{Language, compile_friendly_query};

fuzz_target!(|data: &[u8]| {
    let Ok(pattern) = std::str::from_utf8(data) else {
        return;
    };
    let _ = compile_friendly_query(Language::Rust, pattern);
});

#![no_main]
//! Fuzz the full friendly-mode structural pipeline: compile a pattern,
//! splice templates, parse a source file, and emit the rewritten
//! source. Splits the input into (source, pattern, template) by the
//! first two length-prefix bytes so the engine sees three independent
//! UTF-8 strings.

use libfuzzer_sys::fuzz_target;
use recast_core::{Language, structural_rewrite_friendly};

fuzz_target!(|data: &[u8]| {
    if data.len() < 2 {
        return;
    }
    let src_len = data[0] as usize;
    let pat_len = data[1] as usize;
    let rest = &data[2..];
    if src_len + pat_len > rest.len() {
        return;
    }
    let (source, tail) = rest.split_at(src_len);
    let (pattern, template) = tail.split_at(pat_len);
    let (Ok(source), Ok(pattern), Ok(template)) = (
        std::str::from_utf8(source),
        std::str::from_utf8(pattern),
        std::str::from_utf8(template),
    ) else {
        return;
    };
    let _ = structural_rewrite_friendly(Language::Rust, source, pattern, template);
});

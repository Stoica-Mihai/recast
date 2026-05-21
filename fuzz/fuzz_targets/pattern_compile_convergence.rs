#![no_main]
//! Fuzz `CompiledPattern::compile` + `is_convergent`. The convergence
//! probe walks the replacement template byte-by-byte to strip
//! `$N` / `${name}` placeholders — that walker was a UTF-8 corruption
//! site fixed in v0.1.8, and the fuzz target locks the fix in.

use libfuzzer_sys::fuzz_target;
use recast_core::{CompiledPattern, PatternOptions};

fuzz_target!(|data: &[u8]| {
    if data.is_empty() {
        return;
    }
    let split = data[0] as usize;
    let rest = &data[1..];
    if split > rest.len() {
        return;
    }
    let (pattern, replacement) = rest.split_at(split);
    let (Ok(pattern), Ok(replacement)) =
        (std::str::from_utf8(pattern), std::str::from_utf8(replacement))
    else {
        return;
    };
    let opts = PatternOptions::default();
    if let Ok(compiled) = CompiledPattern::compile(pattern, replacement, &opts) {
        let _ = compiled.is_convergent();
    }
});

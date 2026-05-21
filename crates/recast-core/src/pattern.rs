//! Regex compilation and the convergence (idempotency) check.
//!
//! [`CompiledPattern`] wraps a compiled `regex::Regex` plus its
//! replacement template. [`CompiledPattern::is_convergent`] strips
//! capture-group placeholders from the replacement and tests whether
//! the resulting probe string would re-match the pattern; if so, the
//! rewrite is non-convergent and the planner will reject it.

use regex::{Regex, RegexBuilder};

use crate::error::Result;

/// Knobs controlling how a pattern string is compiled into a regex.
///
/// - `literal` — treat the pattern (and replacement) as plain text;
///   metacharacters are escaped.
/// - `ignore_case` — case-insensitive matching.
/// - `single_line` — disable the implicit `(?s)`. With it off (the
///   default), `.` matches `\n`, which is what most LLM-driven rewrites
///   expect.
#[derive(Debug, Clone, Default)]
pub struct PatternOptions {
    pub literal: bool,
    pub ignore_case: bool,
    pub single_line: bool,
}

/// A compiled regex paired with its replacement template. Construct with
/// [`CompiledPattern::compile`]; use [`CompiledPattern::is_convergent`]
/// to check the idempotency invariant before scanning.
#[derive(Debug, Clone)]
pub struct CompiledPattern {
    regex: Regex,
    replacement: String,
    literal: bool,
}

impl CompiledPattern {
    /// Compile `pattern` into a regex and store `replacement` for later
    /// substitution. Returns [`crate::Error::InvalidRegex`] on syntax errors.
    pub fn compile(pattern: &str, replacement: &str, opts: &PatternOptions) -> Result<Self> {
        let source = if opts.literal { regex::escape(pattern) } else { pattern.to_owned() };
        let regex = RegexBuilder::new(&source)
            .case_insensitive(opts.ignore_case)
            .dot_matches_new_line(!opts.single_line)
            .multi_line(true)
            .build()?;
        Ok(Self { regex, replacement: replacement.to_owned(), literal: opts.literal })
    }

    pub fn regex(&self) -> &Regex {
        &self.regex
    }

    pub fn replacement(&self) -> &str {
        &self.replacement
    }

    /// True when the pattern is convergent given its replacement: re-applying
    /// the rewrite to its own output produces no further match. Catches
    /// non-idempotent rewrites such as `a` → `aa`.
    pub fn is_convergent(&self) -> bool {
        let replacement_probe = self.replacement_probe();
        !self.regex.is_match(&replacement_probe)
    }

    fn replacement_probe(&self) -> String {
        if self.literal {
            return self.replacement.clone();
        }
        let mut out = String::with_capacity(self.replacement.len());
        let bytes = self.replacement.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            let b = bytes[i];
            if b == b'$' && i + 1 < bytes.len() {
                let next = bytes[i + 1];
                if next == b'$' {
                    out.push('$');
                    i += 2;
                    continue;
                }
                if next.is_ascii_digit() {
                    i += 2;
                    while i < bytes.len() && bytes[i].is_ascii_digit() {
                        i += 1;
                    }
                    continue;
                }
                if next == b'{'
                    && let Some((_, _, after)) =
                        crate::template_scan::scan_braced_name(&self.replacement, i)
                {
                    i = after;
                    continue;
                }
            }
            let ch_len = crate::template_scan::utf8_char_len(b);
            out.push_str(&self.replacement[i..i + ch_len]);
            i += ch_len;
        }
        out
    }
}

#[cfg(test)]
#[path = "pattern_tests.rs"]
mod tests;

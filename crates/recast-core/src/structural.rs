//! Tree-sitter-backed structural rewrites (feature `structural`).
//!
//! Pattern syntax is tree-sitter's S-expression Query language.
//! Captures (`@name`) feed the rewrite template, which can reference
//! them as `$name`. The capture named `@root` (or, if absent, the
//! outermost match node) defines the byte range that gets replaced.

use tree_sitter::{Language as TsLanguage, Parser, Query, QueryCursor, StreamingIterator};

use crate::error::{Error, Result};

/// Language registry for structural rewrites. Add a variant per
/// supported tree-sitter grammar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    Rust,
}

impl Language {
    /// Resolve a CLI-friendly name (case-insensitive) to a language.
    pub fn from_name(name: &str) -> Option<Self> {
        match name.to_ascii_lowercase().as_str() {
            "rust" | "rs" => Some(Language::Rust),
            _ => None,
        }
    }

    fn ts_language(self) -> TsLanguage {
        match self {
            Language::Rust => tree_sitter_rust::LANGUAGE.into(),
        }
    }
}

/// Result of [`structural_rewrite`]: the new source text plus the
/// number of disjoint matches that were rewritten.
#[derive(Debug, Clone)]
pub struct StructuralOutcome {
    pub text: String,
    pub matches: usize,
}

/// Run a tree-sitter Query against `source`, substitute captures into
/// `template` per match, and splice the resulting text into the source
/// at each match's replacement range. Overlapping match ranges are
/// resolved greedy-first: the first match by start offset wins, later
/// overlaps are skipped.
pub fn structural_rewrite(
    lang: Language,
    source: &str,
    query_src: &str,
    template: &str,
) -> Result<StructuralOutcome> {
    let ts_lang = lang.ts_language();
    let mut parser = Parser::new();
    parser.set_language(&ts_lang).map_err(|e| Error::StructuralQuery(e.to_string()))?;

    let tree = parser.parse(source, None).ok_or(Error::StructuralParse)?;
    let query =
        Query::new(&ts_lang, query_src).map_err(|e| Error::StructuralQuery(e.to_string()))?;
    let capture_names: Vec<&str> = query.capture_names().to_vec();
    let root_capture_idx = capture_names.iter().position(|n| *n == "root");

    let mut cursor = QueryCursor::new();
    let mut hits: Vec<(usize, usize, String)> = Vec::new();
    let bytes = source.as_bytes();
    let mut iter = cursor.matches(&query, tree.root_node(), bytes);
    while let Some(m) = iter.next() {
        let primary_capture_idx = root_capture_idx
            .unwrap_or_else(|| m.captures.iter().map(|c| c.index as usize).max().unwrap_or(0));
        let primary =
            m.captures.iter().find(|c| c.index as usize == primary_capture_idx).ok_or_else(
                || {
                    Error::StructuralQuery(format!(
                        "match did not bind primary capture index {primary_capture_idx}"
                    ))
                },
            )?;
        let start = primary.node.start_byte();
        let end = primary.node.end_byte();

        let replacement = render_template(template, source, &capture_names, m.captures)?;
        hits.push((start, end, replacement));
    }
    hits.sort_by_key(|h| h.0);

    let mut out = String::with_capacity(source.len());
    let mut cursor_byte = 0usize;
    let mut applied = 0usize;
    for (start, end, replacement) in &hits {
        if *start < cursor_byte {
            continue;
        }
        out.push_str(&source[cursor_byte..*start]);
        out.push_str(replacement);
        cursor_byte = *end;
        applied += 1;
    }
    out.push_str(&source[cursor_byte..]);

    Ok(StructuralOutcome { text: out, matches: applied })
}

fn render_template(
    template: &str,
    source: &str,
    capture_names: &[&str],
    captures: &[tree_sitter::QueryCapture<'_>],
) -> Result<String> {
    let mut out = String::with_capacity(template.len());
    let bytes = template.as_bytes();
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
            if next == b'{' {
                let close = template[i + 2..].find('}').ok_or_else(|| {
                    Error::StructuralTemplate("unterminated `${...}` in template".into())
                })?;
                let name = &template[i + 2..i + 2 + close];
                let cap_idx = capture_names.iter().position(|n| *n == name).ok_or_else(|| {
                    Error::StructuralTemplate(format!("no capture named `${{{name}}}` in query"))
                })?;
                let cap =
                    captures.iter().find(|c| c.index as usize == cap_idx).ok_or_else(|| {
                        Error::StructuralTemplate(format!(
                            "capture `${{{name}}}` did not bind in this match"
                        ))
                    })?;
                let start = cap.node.start_byte();
                let end = cap.node.end_byte();
                out.push_str(&source[start..end]);
                i += 2 + close + 1;
                continue;
            }
            if next.is_ascii_alphabetic() || next == b'_' {
                let mut j = i + 1;
                while j < bytes.len() && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_') {
                    j += 1;
                }
                let name = &template[i + 1..j];
                let cap_idx = capture_names.iter().position(|n| *n == name).ok_or_else(|| {
                    Error::StructuralTemplate(format!("no capture named `${name}` in query"))
                })?;
                let cap =
                    captures.iter().find(|c| c.index as usize == cap_idx).ok_or_else(|| {
                        Error::StructuralTemplate(format!(
                            "capture `${name}` did not bind in this match"
                        ))
                    })?;
                let start = cap.node.start_byte();
                let end = cap.node.end_byte();
                out.push_str(&source[start..end]);
                i = j;
                continue;
            }
        }
        out.push(b as char);
        i += 1;
    }
    Ok(out)
}

#[cfg(test)]
#[path = "structural_tests.rs"]
mod tests;

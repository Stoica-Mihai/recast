//! Tree-sitter-backed structural rewrites (feature `structural`).
//!
//! Pattern syntax is tree-sitter's S-expression Query language.
//! Captures (`@name`) feed the rewrite template, which can reference
//! them as `$name`. The capture named `@root` (or, if absent, the
//! outermost match node) defines the byte range that gets replaced.

use tree_sitter::{Language as TsLanguage, Node, Parser, Query, QueryCursor, StreamingIterator};

use crate::error::{Error, Result};

const METAVAR_PREFIX: &str = "__RECAST_VAR_";
const ELLIPSIS_PREFIX: &str = "__RECAST_ELLIPSIS_";
const METAVAR_SUFFIX: &str = "__";

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

/// Friendlier counterpart to [`structural_rewrite`]: `pattern_source`
/// is written in the target language with `$NAME` placeholders. The
/// pattern is compiled to a tree-sitter Query under the hood; the
/// rewrite template uses the same `$NAME` / `${NAME}` substitution as
/// the raw API.
///
/// Example for Rust:
///
/// ```text
/// pattern:  "fn $NAME() {}"
/// template: "fn ${NAME}_v2() {}"
/// ```
///
/// Metavariables match a single AST node at the position where the
/// `$NAME` placeholder appeared in the parsed pattern (`(_)` wildcard
/// in the underlying query). Capture names are the placeholder name
/// minus the leading `$`.
pub fn structural_rewrite_friendly(
    lang: Language,
    source: &str,
    pattern_source: &str,
    template: &str,
) -> Result<StructuralOutcome> {
    let query = compile_friendly_query(lang, pattern_source)?;
    structural_rewrite(lang, source, &query, template)
}

/// Compile a friendly pattern (target-language source with `$NAME`
/// placeholders) into a tree-sitter Query string. Exposed for callers
/// that want to inspect or further manipulate the query.
pub fn compile_friendly_query(lang: Language, pattern: &str) -> Result<String> {
    compile_friendly_pattern(lang, pattern)
}

fn compile_friendly_pattern(lang: Language, pattern: &str) -> Result<String> {
    let substituted = substitute_metavars(pattern);
    let ts_lang = lang.ts_language();
    let mut parser = Parser::new();
    parser.set_language(&ts_lang).map_err(|e| Error::StructuralQuery(e.to_string()))?;
    let tree = parser
        .parse(&substituted, None)
        .ok_or_else(|| Error::StructuralQuery("could not parse friendly pattern".into()))?;
    let root = tree.root_node();
    if root.has_error() {
        return Err(Error::StructuralQuery(format!(
            "friendly pattern has parse errors (after metavar substitution): {}",
            root.to_sexp()
        )));
    }
    // Tree-sitter wraps top-level items in a `source_file` (or similar)
    // container; unwrap so the user-visible pattern matches the actual
    // item, not the whole file.
    let effective = if root.kind() == "source_file" && root.named_child_count() >= 1 {
        root.named_child(0).ok_or_else(|| Error::StructuralQuery("empty pattern".into()))?
    } else {
        root
    };

    let mut buf = String::new();
    let mut predicates: Vec<String> = Vec::new();
    let mut lit_counter: usize = 0;
    emit_node(&mut buf, &mut predicates, &mut lit_counter, effective, substituted.as_bytes());
    let trimmed = buf.trim_start();
    if predicates.is_empty() {
        Ok(format!("{trimmed} @root"))
    } else {
        Ok(format!("({trimmed} @root {})", predicates.join(" ")))
    }
}

fn substitute_metavars(pattern: &str) -> String {
    let mut out = String::with_capacity(pattern.len());
    let bytes = pattern.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        // $$$NAME — ellipsis metavar (variable-shape subtree)
        if b == b'$'
            && i + 3 < bytes.len()
            && bytes[i + 1] == b'$'
            && bytes[i + 2] == b'$'
            && (bytes[i + 3].is_ascii_alphabetic() || bytes[i + 3] == b'_')
        {
            let mut j = i + 3;
            while j < bytes.len() && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_') {
                j += 1;
            }
            let name = &pattern[i + 3..j];
            out.push_str(ELLIPSIS_PREFIX);
            out.push_str(name);
            out.push_str(METAVAR_SUFFIX);
            i = j;
            continue;
        }
        if b == b'$'
            && i + 1 < bytes.len()
            && (bytes[i + 1].is_ascii_alphabetic() || bytes[i + 1] == b'_')
        {
            let mut j = i + 1;
            while j < bytes.len() && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_') {
                j += 1;
            }
            let name = &pattern[i + 1..j];
            out.push_str(METAVAR_PREFIX);
            out.push_str(name);
            out.push_str(METAVAR_SUFFIX);
            i = j;
            continue;
        }
        out.push(b as char);
        i += 1;
    }
    out
}

fn emit_node(
    buf: &mut String,
    predicates: &mut Vec<String>,
    lit_counter: &mut usize,
    node: Node<'_>,
    src: &[u8],
) {
    if !node.is_named() {
        return;
    }
    if let Some(ellipsis) = subtree_ellipsis_capture(node, src) {
        buf.push_str(" (_) @");
        buf.push_str(&ellipsis);
        return;
    }
    if let Some(meta) = metavar_at(node, src) {
        buf.push_str(" (_) @");
        buf.push_str(&meta);
        return;
    }
    // Terminal named leaves (identifier, integer_literal, etc.) are
    // constrained to exact text via #eq? predicates so a literal in the
    // pattern doesn't match every same-kind sibling in the source.
    if node.named_child_count() == 0 {
        if let Ok(text) = node.utf8_text(src) {
            let cap = format!("__lit{lit_counter}");
            *lit_counter += 1;
            buf.push_str(&format!(" ({}) @{}", node.kind(), cap));
            predicates.push(format!("(#eq? @{cap} \"{}\")", escape_query_string(text)));
            return;
        }
    }
    buf.push_str(" (");
    buf.push_str(node.kind());
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        let field = node.field_name_for_child(child.id() as u32);
        if let Some(name) = field {
            buf.push(' ');
            buf.push_str(name);
            buf.push(':');
        }
        emit_node(buf, predicates, lit_counter, child, src);
    }
    buf.push(')');
}

fn escape_query_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            other => out.push(other),
        }
    }
    out
}

fn metavar_at(node: Node<'_>, src: &[u8]) -> Option<String> {
    if node.named_child_count() != 0 {
        return None;
    }
    let text = node.utf8_text(src).ok()?;
    let stripped = text.strip_prefix(METAVAR_PREFIX)?.strip_suffix(METAVAR_SUFFIX)?;
    if stripped.is_empty() {
        return None;
    }
    Some(stripped.to_owned())
}

/// Walk the subtree rooted at `node` and, if it contains exactly one
/// ellipsis identifier (`$$$NAME` → `__RECAST_ELLIPSIS_NAME__`) and no
/// other named leaves carrying meaningful content (no literals, no
/// single-node metavars), return the ellipsis name. Such a subtree
/// collapses to a single `(_) @NAME` wildcard in the generated query
/// so the parent field can match any shape.
fn subtree_ellipsis_capture(node: Node<'_>, src: &[u8]) -> Option<String> {
    let mut ellipsis: Option<String> = None;
    let mut other_leaves = 0usize;
    let mut stack = vec![node];
    while let Some(n) = stack.pop() {
        if !n.is_named() {
            continue;
        }
        if n.named_child_count() == 0 {
            let text = n.utf8_text(src).ok()?;
            if let Some(stripped) =
                text.strip_prefix(ELLIPSIS_PREFIX).and_then(|s| s.strip_suffix(METAVAR_SUFFIX))
                && !stripped.is_empty()
            {
                if ellipsis.is_some() {
                    return None;
                }
                ellipsis = Some(stripped.to_owned());
                continue;
            }
            other_leaves += 1;
            continue;
        }
        let mut c = n.walk();
        for child in n.named_children(&mut c) {
            stack.push(child);
        }
    }
    if other_leaves == 0 { ellipsis } else { None }
}

#[cfg(test)]
#[path = "structural_tests.rs"]
mod tests;

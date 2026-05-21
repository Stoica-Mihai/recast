//! Tree-sitter-backed structural rewrites (feature `structural`).
//!
//! Pattern syntax is tree-sitter's S-expression Query language.
//! Captures (`@name`) feed the rewrite template, which can reference
//! them as `$name`. The capture named `@root` (or, if absent, the
//! outermost match node) defines the byte range that gets replaced.

use std::path::Path;

use tree_sitter::{Language as TsLanguage, Node, Parser, Query, QueryCursor, StreamingIterator};

use crate::error::{Error, Result};
use crate::plan::{
    FileChange, Plan, PlanOptions, PlanOutcome, check_match_counts, read_text_or_skip_binary,
};
use crate::rewrite::{label_for_path, unified_diff};
use crate::walker::walk_paths;

const METAVAR_PREFIX: &str = "__RECAST_VAR_";
const ELLIPSIS_PREFIX: &str = "__RECAST_ELLIPSIS_";
const METAVAR_SUFFIX: &str = "__";

/// Language registry for structural rewrites. Variants are gated by
/// the matching `lang-*` cargo feature; build with `--features
/// lang-all` to enable every grammar shipped today.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Language {
    #[cfg(feature = "lang-rust")]
    Rust,
    #[cfg(feature = "lang-ts")]
    TypeScript,
    #[cfg(feature = "lang-ts")]
    Tsx,
    #[cfg(feature = "lang-js")]
    JavaScript,
    #[cfg(feature = "lang-python")]
    Python,
    #[cfg(feature = "lang-bash")]
    Bash,
    #[cfg(feature = "lang-go")]
    Go,
    #[cfg(feature = "lang-json")]
    Json,
    #[cfg(feature = "lang-md")]
    Markdown,
}

impl Language {
    /// Resolve a CLI-friendly name (case-insensitive) to a language.
    /// Returns `None` for languages whose `lang-*` feature wasn't
    /// compiled in.
    pub fn from_name(name: &str) -> Option<Self> {
        match name.to_ascii_lowercase().as_str() {
            #[cfg(feature = "lang-rust")]
            "rust" | "rs" => Some(Language::Rust),
            #[cfg(feature = "lang-ts")]
            "typescript" | "ts" => Some(Language::TypeScript),
            #[cfg(feature = "lang-ts")]
            "tsx" => Some(Language::Tsx),
            #[cfg(feature = "lang-js")]
            "javascript" | "js" | "jsx" => Some(Language::JavaScript),
            #[cfg(feature = "lang-python")]
            "python" | "py" => Some(Language::Python),
            #[cfg(feature = "lang-bash")]
            "bash" | "sh" | "shell" => Some(Language::Bash),
            #[cfg(feature = "lang-go")]
            "go" | "golang" => Some(Language::Go),
            #[cfg(feature = "lang-json")]
            "json" => Some(Language::Json),
            #[cfg(feature = "lang-md")]
            "markdown" | "md" => Some(Language::Markdown),
            _ => None,
        }
    }

    fn ts_language(self) -> TsLanguage {
        match self {
            #[cfg(feature = "lang-rust")]
            Language::Rust => tree_sitter_rust::LANGUAGE.into(),
            #[cfg(feature = "lang-ts")]
            Language::TypeScript => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            #[cfg(feature = "lang-ts")]
            Language::Tsx => tree_sitter_typescript::LANGUAGE_TSX.into(),
            #[cfg(feature = "lang-js")]
            Language::JavaScript => tree_sitter_javascript::LANGUAGE.into(),
            #[cfg(feature = "lang-python")]
            Language::Python => tree_sitter_python::LANGUAGE.into(),
            #[cfg(feature = "lang-bash")]
            Language::Bash => tree_sitter_bash::LANGUAGE.into(),
            #[cfg(feature = "lang-go")]
            Language::Go => tree_sitter_go::LANGUAGE.into(),
            #[cfg(feature = "lang-json")]
            Language::Json => tree_sitter_json::LANGUAGE.into(),
            #[cfg(feature = "lang-md")]
            Language::Markdown => tree_sitter_md::LANGUAGE.into(),
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
    let query = Query::new(&ts_lang, query_src)
        .map_err(|e| Error::StructuralQuery(format_query_error(query_src, &e)))?;
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

/// Multi-file structural pipeline. Walks `roots`, applies
/// [`structural_rewrite`] per file, and folds the results into a
/// [`Plan`] that callers can pipe into [`crate::apply_changes`]. Honors
/// `walk_options`, `max_files`, `max_bytes`, and the `at_least` /
/// `at_most` match-count guard from `opts`. The convergence check and
/// scripted-callback variants don't apply here — structural rewrites
/// aren't re-probed against their own output.
pub fn plan_structural_rewrite<P: AsRef<Path>>(
    lang: Language,
    query: &str,
    template: &str,
    roots: &[P],
    opts: &PlanOptions,
) -> Result<Plan> {
    let files = walk_paths(roots, &opts.walk_options)?;
    if files.len() > opts.max_files {
        return Err(Error::TooManyFiles { count: files.len(), limit: opts.max_files });
    }

    let mut changes: Vec<FileChange> = Vec::new();
    for path in &files {
        let before = match read_text_or_skip_binary(path, opts.max_bytes)? {
            Some(s) => s,
            None => continue,
        };
        let outcome = structural_rewrite(lang, &before, query, template)?;
        if outcome.text == before {
            continue;
        }
        let label = label_for_path(path);
        let diff = unified_diff(&label, &before, &outcome.text);
        changes.push(FileChange {
            path: path.clone(),
            matches: outcome.matches,
            before,
            after: outcome.text,
            diff,
        });
    }
    let total_matches: usize = changes.iter().map(|c| c.matches).sum();
    let files_scanned = files.len();
    if total_matches == 0 {
        return Ok(Plan {
            changes: Vec::new(),
            total_matches: 0,
            files_scanned,
            outcome: PlanOutcome::AlreadyApplied,
        });
    }
    check_match_counts(total_matches, opts.at_least, opts.at_most)?;
    Ok(Plan { changes, total_matches, files_scanned, outcome: PlanOutcome::Changes })
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
    let tree = parser.parse(&substituted, None).ok_or_else(|| {
        Error::StructuralQuery(format!(
            "could not parse `--ast` pattern with the requested grammar; check that the pattern is valid {} syntax with `$NAME` / `$$$NAME` metavars in node positions",
            ts_lang.name().unwrap_or("source")
        ))
    })?;
    let root = tree.root_node();
    if root.has_error() {
        let snippet = pattern.lines().next().unwrap_or(pattern);
        return Err(Error::StructuralQuery(format!(
            "`--ast` pattern is not valid {} source after metavar substitution: `{snippet}`. \
             Metavars (`$NAME`, `$$$NAME`) can only appear where an identifier-like token is \
             legal in the target language.",
            ts_lang.name().unwrap_or("source")
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
    if node.named_child_count() == 0
        && let Ok(text) = node.utf8_text(src)
    {
        let cap = format!("__lit{lit_counter}");
        *lit_counter += 1;
        buf.push_str(&format!(" ({}) @{}", node.kind(), cap));
        predicates.push(format!("(#eq? @{cap} \"{}\")", escape_query_string(text)));
        return;
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

/// Render a tree-sitter `QueryError` with the offending fragment and a
/// caret pointing at the byte offset, so callers see something useful
/// instead of the raw `QueryError { row, column, offset, kind, message }`
/// Debug output.
fn format_query_error(query_src: &str, err: &tree_sitter::QueryError) -> String {
    let kind = match err.kind {
        tree_sitter::QueryErrorKind::Syntax => "syntax",
        tree_sitter::QueryErrorKind::NodeType => "unknown node type",
        tree_sitter::QueryErrorKind::Field => "unknown field",
        tree_sitter::QueryErrorKind::Capture => "unknown capture",
        tree_sitter::QueryErrorKind::Predicate => "bad predicate",
        tree_sitter::QueryErrorKind::Structure => "structural mismatch",
        tree_sitter::QueryErrorKind::Language => "language mismatch",
    };
    let line = query_src.lines().nth(err.row).unwrap_or("");
    let caret_col = err.column.min(line.len());
    let caret = format!("{}^", " ".repeat(caret_col));
    let msg = err.message.trim();
    format!(
        "tree-sitter query {kind} error at line {row}, column {col}: {msg}\n  | {line}\n  | {caret}",
        row = err.row + 1,
        col = err.column + 1,
    )
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

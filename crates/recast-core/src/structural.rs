//! Tree-sitter-backed structural rewrites (feature `structural`).
//!
//! Pattern syntax is tree-sitter's S-expression Query language.
//! Captures (`@name`) feed the rewrite template, which can reference
//! them as `$name`. The capture named `@root` (or, if absent, the
//! outermost match node) defines the byte range that gets replaced.

use std::path::Path;

use rayon::prelude::*;
use tree_sitter::{Language as TsLanguage, Node, Parser, Query, QueryCursor, StreamingIterator};

use crate::error::{Error, Result};
use crate::plan::{
    FileChange, Plan, PlanOptions, PlanOutcome, check_match_counts, read_text_or_skip_binary,
};
use crate::rewrite::{label_for_path, unified_diff};
use crate::search::{
    SearchFile, SearchMatch, SearchOptions, SearchPlan, collect, scan, truncate_snippet,
};
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
    /// Returns [`Error::UnknownLanguage`] for names that aren't
    /// recognized or whose `lang-*` feature wasn't compiled in.
    pub fn from_name(name: &str) -> Result<Self> {
        match name.to_ascii_lowercase().as_str() {
            #[cfg(feature = "lang-rust")]
            "rust" | "rs" => Ok(Language::Rust),
            #[cfg(feature = "lang-ts")]
            "typescript" | "ts" => Ok(Language::TypeScript),
            #[cfg(feature = "lang-ts")]
            "tsx" => Ok(Language::Tsx),
            #[cfg(feature = "lang-js")]
            "javascript" | "js" | "jsx" => Ok(Language::JavaScript),
            #[cfg(feature = "lang-python")]
            "python" | "py" => Ok(Language::Python),
            #[cfg(feature = "lang-bash")]
            "bash" | "sh" | "shell" => Ok(Language::Bash),
            #[cfg(feature = "lang-go")]
            "go" | "golang" => Ok(Language::Go),
            #[cfg(feature = "lang-json")]
            "json" => Ok(Language::Json),
            #[cfg(feature = "lang-md")]
            "markdown" | "md" => Ok(Language::Markdown),
            _ => Err(Error::UnknownLanguage(name.to_owned())),
        }
    }

    /// Infer the grammar from a file extension. Returns `None` for
    /// extensions without a compiled grammar — the syntax-regression
    /// guard is skipped for those files, so a `--no-default-features`
    /// build that drops every `lang-*` feature keeps working unchecked.
    pub fn from_path(path: &Path) -> Option<Self> {
        match path.extension()?.to_str()? {
            #[cfg(feature = "lang-rust")]
            "rs" => Some(Language::Rust),
            #[cfg(feature = "lang-ts")]
            "ts" => Some(Language::TypeScript),
            #[cfg(feature = "lang-ts")]
            "tsx" => Some(Language::Tsx),
            #[cfg(feature = "lang-js")]
            "js" | "mjs" | "cjs" | "jsx" => Some(Language::JavaScript),
            #[cfg(feature = "lang-python")]
            "py" | "pyi" => Some(Language::Python),
            #[cfg(feature = "lang-bash")]
            "sh" | "bash" => Some(Language::Bash),
            #[cfg(feature = "lang-go")]
            "go" => Some(Language::Go),
            #[cfg(feature = "lang-json")]
            "json" => Some(Language::Json),
            #[cfg(feature = "lang-md")]
            "md" | "markdown" => Some(Language::Markdown),
            _ => None,
        }
    }

    /// Stable lowercase name for diagnostics (matches the canonical
    /// `from_name` alias).
    pub(crate) fn name(self) -> &'static str {
        match self {
            #[cfg(feature = "lang-rust")]
            Language::Rust => "rust",
            #[cfg(feature = "lang-ts")]
            Language::TypeScript => "typescript",
            #[cfg(feature = "lang-ts")]
            Language::Tsx => "tsx",
            #[cfg(feature = "lang-js")]
            Language::JavaScript => "javascript",
            #[cfg(feature = "lang-python")]
            Language::Python => "python",
            #[cfg(feature = "lang-bash")]
            Language::Bash => "bash",
            #[cfg(feature = "lang-go")]
            Language::Go => "go",
            #[cfg(feature = "lang-json")]
            Language::Json => "json",
            #[cfg(feature = "lang-md")]
            Language::Markdown => "markdown",
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

/// Parse `src` with `lang`'s grammar and count `ERROR` + `MISSING`
/// nodes. A parse that yields no tree (grammar load failure) counts
/// as zero — the guard compares pre/post deltas, so an unparsable
/// language degrades to "no regression detected" rather than a false
/// positive.
pub(crate) fn count_error_nodes(lang: Language, src: &str) -> usize {
    let mut parser = Parser::new();
    if parser.set_language(&lang.ts_language()).is_err() {
        return 0;
    }
    let Some(tree) = parser.parse(src, None) else {
        return 0;
    };
    let mut count = 0usize;
    let mut stack = vec![tree.root_node()];
    while let Some(node) = stack.pop() {
        if node.is_error() || node.is_missing() {
            count += 1;
        }
        let mut c = node.walk();
        for child in node.children(&mut c) {
            stack.push(child);
        }
    }
    count
}

/// Reject a rewrite whose post-image introduces *new* syntax errors.
/// Files whose extension maps to no compiled grammar pass through
/// unchecked. The comparison is a count delta — a file that was
/// already unparsable stays acceptable as long as the rewrite doesn't
/// make it worse. Shared by the regex/script planner ([`crate::plan`])
/// and the structural planner.
pub(crate) fn guard_syntax(path: &Path, before: &str, after: &str) -> Result<()> {
    let Some(lang) = Language::from_path(path) else {
        return Ok(());
    };
    let new_errors = count_error_nodes(lang, after).saturating_sub(count_error_nodes(lang, before));
    if new_errors > 0 {
        return Err(Error::SyntaxRegression {
            path: path.to_path_buf(),
            lang: lang.name(),
            new_errors,
        });
    }
    Ok(())
}

/// Result of [`structural_rewrite`]: the new source text plus the
/// number of disjoint matches that were rewritten.
#[derive(Debug, Clone)]
pub struct StructuralOutcome {
    pub text: String,
    pub matches: usize,
}

/// One slice of the parsed rewrite template. Literals are pre-joined
/// strings between captures; captures resolve to a known capture index
/// in the compiled query.
enum TemplatePart {
    Literal(String),
    Capture { index: usize, name: String },
}

/// One match in [`CompiledStructural::apply`]: byte range the rewrite
/// occupies in the source plus the rendered replacement text. Sorted by
/// `start` before splicing; overlapping later hits are skipped.
struct Hit {
    start: usize,
    end: usize,
    replacement: String,
}

/// A compiled structural-rewrite job: language, query, capture index
/// table, and the rewrite template pre-resolved to a sequence of
/// literal/capture parts. Built once per invocation and applied to every
/// candidate file — that's the whole point of pulling parsing out of
/// the per-file loop.
struct CompiledStructural {
    ts_lang: TsLanguage,
    query: Query,
    root_capture_idx: Option<usize>,
    template_parts: Vec<TemplatePart>,
    include_leading_attrs: bool,
}

impl CompiledStructural {
    fn compile(
        lang: Language,
        query_src: &str,
        template: &str,
        include_leading_attrs: bool,
    ) -> Result<Self> {
        let ts_lang = lang.ts_language();
        // Probe the language by configuring a throwaway parser. Catches
        // ABI mismatch up front so the per-thread workers can rely on
        // `set_language` succeeding without surfacing late errors.
        let mut probe = Parser::new();
        probe.set_language(&ts_lang).map_err(|e| Error::StructuralQuery(e.to_string()))?;

        let query = Query::new(&ts_lang, query_src)
            .map_err(|e| Error::StructuralQuery(format_query_error(query_src, &e)))?;
        let capture_names: Vec<&str> = query.capture_names().to_vec();
        let root_capture_idx = capture_names.iter().position(|n| *n == "root");
        let template_parts = parse_template(template, &capture_names)?;

        Ok(Self { ts_lang, query, root_capture_idx, template_parts, include_leading_attrs })
    }

    fn new_parser(&self) -> Parser {
        let mut parser = Parser::new();
        // Language ABI was validated in `compile`, so this call is
        // infallible in practice. If it somehow does fail, the parser
        // stays in its unset state and the next `parse()` returns None,
        // surfacing as Error::StructuralParse — no panic, defined
        // behavior.
        let _ = parser.set_language(&self.ts_lang);
        parser
    }

    fn apply(
        &self,
        parser: &mut Parser,
        cursor: &mut QueryCursor,
        source: &str,
    ) -> Result<StructuralOutcome> {
        let tree = parser.parse(source, None).ok_or(Error::StructuralParse)?;
        let bytes = source.as_bytes();

        let mut hits: Vec<Hit> = Vec::new();
        let mut iter = cursor.matches(&self.query, tree.root_node(), bytes);
        while let Some(m) = iter.next() {
            let primary = match self.root_capture_idx {
                Some(idx) => {
                    m.captures.iter().find(|c| c.index as usize == idx).ok_or_else(|| {
                        Error::StructuralQuery(format!(
                            "match did not bind primary capture index {idx}"
                        ))
                    })?
                }
                None => outermost_capture(m.captures)
                    .ok_or_else(|| Error::StructuralQuery("match bound no captures".into()))?,
            };
            let replacement = self.render(source, m.captures)?;
            let start = if self.include_leading_attrs {
                extend_start_over_attrs(primary.node, source)
            } else {
                primary.node.start_byte()
            };
            hits.push(Hit { start, end: primary.node.end_byte(), replacement });
        }
        hits.sort_by_key(|h| h.start);

        // Reserve source.len() plus the per-hit (replacement - range) delta
        // so the splice loop doesn't realloc when replacements grow the text.
        let extra: usize =
            hits.iter().map(|h| h.replacement.len().saturating_sub(h.end - h.start)).sum();
        let mut out = String::with_capacity(source.len() + extra);
        let mut cursor_byte = 0usize;
        let mut applied = 0usize;
        for hit in &hits {
            if hit.start < cursor_byte {
                continue;
            }
            out.push_str(&source[cursor_byte..hit.start]);
            out.push_str(&hit.replacement);
            cursor_byte = hit.end;
            applied += 1;
        }
        out.push_str(&source[cursor_byte..]);
        Ok(StructuralOutcome { text: out, matches: applied })
    }

    pub(crate) fn search(
        &self,
        parser: &mut Parser,
        cursor: &mut QueryCursor,
        source: &str,
    ) -> Result<Vec<SearchMatch>> {
        let tree = parser.parse(source, None).ok_or(Error::StructuralParse)?;
        let bytes = source.as_bytes();
        let capture_names = self.query.capture_names();

        let mut hits: Vec<SearchMatch> = Vec::new();
        let mut iter = cursor.matches(&self.query, tree.root_node(), bytes);
        while let Some(m) = iter.next() {
            let primary = match self.root_capture_idx {
                Some(idx) => {
                    m.captures.iter().find(|c| c.index as usize == idx).ok_or_else(|| {
                        Error::StructuralQuery(format!(
                            "match did not bind primary capture index {idx}"
                        ))
                    })?
                }
                None => outermost_capture(m.captures)
                    .ok_or_else(|| Error::StructuralQuery("match bound no captures".into()))?,
            };
            let pos = primary.node.start_position();
            let capture_name =
                capture_names.get(primary.index as usize).copied().map(ToOwned::to_owned);
            let snippet =
                truncate_snippet(&source[primary.node.start_byte()..primary.node.end_byte()]);
            hits.push(SearchMatch {
                line: pos.row + 1,
                column: pos.column + 1,
                snippet,
                capture: capture_name,
            });
        }
        hits.sort_by_key(|h| (h.line, h.column));
        Ok(hits)
    }

    fn render(&self, source: &str, captures: &[tree_sitter::QueryCapture<'_>]) -> Result<String> {
        let mut out = String::with_capacity(self.template_size_hint());
        for part in &self.template_parts {
            match part {
                TemplatePart::Literal(s) => out.push_str(s),
                TemplatePart::Capture { index, name } => {
                    let cap =
                        captures.iter().find(|c| c.index as usize == *index).ok_or_else(|| {
                            Error::StructuralTemplate(format!(
                                "capture `${name}` did not bind in this match"
                            ))
                        })?;
                    out.push_str(&source[cap.node.start_byte()..cap.node.end_byte()]);
                }
            }
        }
        Ok(out)
    }

    fn template_size_hint(&self) -> usize {
        self.template_parts
            .iter()
            .map(|p| match p {
                TemplatePart::Literal(s) => s.len(),
                TemplatePart::Capture { .. } => 16,
            })
            .sum()
    }
}

fn parse_template(template: &str, capture_names: &[&str]) -> Result<Vec<TemplatePart>> {
    use crate::template_scan::{scan_braced_name, scan_meta_name, utf8_char_len};

    let mut parts: Vec<TemplatePart> = Vec::new();
    let mut literal = String::new();
    let bytes = template.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'$' && i + 1 < bytes.len() {
            let next = bytes[i + 1];
            if next == b'$' {
                literal.push('$');
                i += 2;
                continue;
            }
            if next == b'{' {
                let (name_start, name_end, after) =
                    scan_braced_name(template, i).ok_or_else(|| {
                        Error::StructuralTemplate("unterminated `${...}` in template".into())
                    })?;
                let name = &template[name_start..name_end];
                push_capture(&mut parts, &mut literal, capture_names, name, true)?;
                i = after;
                continue;
            }
            if let Some((name_start, name_end, after)) = scan_meta_name(bytes, i) {
                let name = &template[name_start..name_end];
                push_capture(&mut parts, &mut literal, capture_names, name, false)?;
                i = after;
                continue;
            }
        }
        let ch_len = utf8_char_len(b);
        literal.push_str(&template[i..i + ch_len]);
        i += ch_len;
    }
    flush_literal(&mut literal, &mut parts);
    Ok(parts)
}

fn push_capture(
    parts: &mut Vec<TemplatePart>,
    literal: &mut String,
    capture_names: &[&str],
    name: &str,
    braced: bool,
) -> Result<()> {
    let cap_idx = capture_names.iter().position(|n| *n == name).ok_or_else(|| {
        if braced {
            Error::StructuralTemplate(format!("no capture named `${{{name}}}` in query"))
        } else {
            Error::StructuralTemplate(format!("no capture named `${name}` in query"))
        }
    })?;
    flush_literal(literal, parts);
    parts.push(TemplatePart::Capture { index: cap_idx, name: name.to_owned() });
    Ok(())
}

fn flush_literal(literal: &mut String, parts: &mut Vec<TemplatePart>) {
    if !literal.is_empty() {
        parts.push(TemplatePart::Literal(std::mem::take(literal)));
    }
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
    structural_rewrite_attrs(lang, source, query_src, template, false)
}

/// Variant of [`structural_rewrite`] that, when `include_leading_attrs`
/// is set, extends each replacement range backward over the contiguous
/// run of preceding `attribute_item` / doc-comment siblings — so
/// deleting a function also removes its `#[test]` / `///` lines instead
/// of orphaning them. A blank-line gap or a non-attr/non-doc sibling
/// ends the run. Node kinds are tree-sitter-rust's; languages without
/// those kinds simply never extend (no-op).
pub(crate) fn structural_rewrite_attrs(
    lang: Language,
    source: &str,
    query_src: &str,
    template: &str,
    include_leading_attrs: bool,
) -> Result<StructuralOutcome> {
    let compiled = CompiledStructural::compile(lang, query_src, template, include_leading_attrs)?;
    let mut parser = compiled.new_parser();
    let mut cursor = QueryCursor::new();
    compiled.apply(&mut parser, &mut cursor, source)
}

/// Walk backward from `node` over the contiguous run of preceding
/// `attribute_item` / doc-comment siblings, returning the start byte of
/// the earliest one in the run (or `node.start_byte()` if none). A
/// blank line (two or more newlines in the inter-sibling gap) breaks the
/// run, preserving intentional separation.
fn extend_start_over_attrs(node: Node, source: &str) -> usize {
    let bytes = source.as_bytes();
    let mut start = node.start_byte();
    let mut anchor = node;
    while let Some(prev) = anchor.prev_sibling() {
        if !is_swallowable_sibling(&prev, bytes) {
            break;
        }
        let gap = &source[prev.end_byte()..anchor.start_byte()];
        if gap.matches('\n').count() >= 2 {
            break;
        }
        start = prev.start_byte();
        anchor = prev;
    }
    start
}

/// True for an `attribute_item` or a doc-style comment (`///`, `//!`,
/// `/**`, `/*!`). Plain `//` / `/* */` comments are left in place.
fn is_swallowable_sibling(node: &Node, source: &[u8]) -> bool {
    match node.kind() {
        "attribute_item" => true,
        "line_comment" | "block_comment" => {
            let text = &source[node.start_byte()..node.end_byte()];
            text.starts_with(b"///")
                || text.starts_with(b"//!")
                || text.starts_with(b"/**")
                || text.starts_with(b"/*!")
        }
        _ => false,
    }
}

/// Multi-file structural pipeline. Walks `roots`, applies
/// [`structural_rewrite`] per file, and folds the results into a
/// [`Plan`] that callers can pipe into [`crate::apply_changes`]. Honors
/// `walk_options`, `max_files`, `max_bytes`, and the `at_least` /
/// `at_most` match-count guard from `opts`. The convergence check and
/// scripted-callback variants don't apply here — structural rewrites
/// aren't re-probed against their own output.
///
/// The compiled query, capture-index table, and parsed rewrite template
/// are built once and shared read-only across the per-file workers; only
/// the tree-sitter `Parser` and `QueryCursor` are per-thread.
pub fn plan_structural_rewrite<P: AsRef<Path>>(
    lang: Language,
    query: &str,
    template: &str,
    roots: &[P],
    opts: &PlanOptions,
    include_leading_attrs: bool,
) -> Result<Plan> {
    let files = walk_paths(roots, &opts.walk_options)?;
    if files.len() > opts.max_files {
        return Err(Error::TooManyFiles { count: files.len(), limit: opts.max_files });
    }
    let files_scanned = files.len();

    let compiled = CompiledStructural::compile(lang, query, template, include_leading_attrs)?;

    let results: Vec<Result<Option<FileChange>>> = files
        .par_iter()
        .map_init(
            || (compiled.new_parser(), QueryCursor::new()),
            |(parser, cursor), path| plan_one(&compiled, parser, cursor, path, opts),
        )
        .collect();

    let mut changes: Vec<FileChange> = Vec::with_capacity(files_scanned);
    for r in results {
        if let Some(change) = r? {
            changes.push(change);
        }
    }

    let total_matches: usize = changes.iter().map(|c| c.matches).sum();
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

fn plan_one(
    compiled: &CompiledStructural,
    parser: &mut Parser,
    cursor: &mut QueryCursor,
    path: &Path,
    opts: &PlanOptions,
) -> Result<Option<FileChange>> {
    let (before, permissions) = match read_text_or_skip_binary(path, opts.max_bytes)? {
        Some(pair) => pair,
        None => return Ok(None),
    };
    let outcome = compiled.apply(parser, cursor, &before)?;
    if outcome.text == before {
        return Ok(None);
    }
    if !opts.allow_syntax_errors {
        guard_syntax(path, &before, &outcome.text)?;
    }
    let label = label_for_path(path);
    let diff = unified_diff(&label, &before, &outcome.text);
    Ok(Some(FileChange {
        path: path.to_path_buf(),
        matches: outcome.matches,
        after: outcome.text,
        diff,
        permissions: Some(permissions),
    }))
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

/// Run a tree-sitter query against `source` and return match locations without rewriting.
pub fn structural_search(
    lang: Language,
    source: &str,
    query_src: &str,
) -> Result<Vec<SearchMatch>> {
    let compiled = CompiledStructural::compile(lang, query_src, "", false)?;
    let mut parser = compiled.new_parser();
    let mut cursor = QueryCursor::new();
    compiled.search(&mut parser, &mut cursor, source)
}

/// Multi-file structural search. Walk `roots`, run the query per file, fold into `SearchPlan`.
pub fn plan_structural_search<P: AsRef<std::path::Path>>(
    lang: Language,
    query_src: &str,
    roots: &[P],
    opts: &SearchOptions,
) -> Result<SearchPlan> {
    let files = scan(roots, opts)?;
    let files_scanned = files.len();
    let compiled = CompiledStructural::compile(lang, query_src, "", false)?;

    let results: Vec<Result<Option<SearchFile>>> = files
        .par_iter()
        .map_init(
            || (compiled.new_parser(), QueryCursor::new()),
            |(parser, cursor), path| {
                let (source, _) = match read_text_or_skip_binary(path, opts.max_bytes)? {
                    Some(pair) => pair,
                    None => return Ok(None),
                };
                let matches = compiled.search(parser, cursor, &source)?;
                if matches.is_empty() {
                    return Ok(None);
                }
                Ok(Some(SearchFile { path: path.to_path_buf(), matches }))
            },
        )
        .collect();

    let found = collect(results)?;
    let total_matches: usize = found.iter().map(|f| f.matches.len()).sum();
    check_match_counts(total_matches, opts.at_least, opts.at_most)?;
    Ok(SearchPlan { files: found, total_matches, files_scanned })
}

/// Compile a friendly pattern (target-language source with `$NAME`
/// placeholders) into a tree-sitter Query string. Exposed for callers
/// that want to inspect or further manipulate the query.
pub fn compile_friendly_query(lang: Language, pattern: &str) -> Result<String> {
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
    use crate::template_scan::{scan_ellipsis_name, scan_meta_name, utf8_char_len};

    let mut out = String::with_capacity(pattern.len());
    let bytes = pattern.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'$' {
            // $$$NAME — ellipsis metavar (variable-shape subtree)
            if let Some((name_start, name_end, after)) = scan_ellipsis_name(bytes, i) {
                out.push_str(ELLIPSIS_PREFIX);
                out.push_str(&pattern[name_start..name_end]);
                out.push_str(METAVAR_SUFFIX);
                i = after;
                continue;
            }
            if let Some((name_start, name_end, after)) = scan_meta_name(bytes, i) {
                out.push_str(METAVAR_PREFIX);
                out.push_str(&pattern[name_start..name_end]);
                out.push_str(METAVAR_SUFFIX);
                i = after;
                continue;
            }
        }
        let ch_len = utf8_char_len(b);
        out.push_str(&pattern[i..i + ch_len]);
        i += ch_len;
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
    use std::fmt::Write as _;

    // Iterative: user `--ast` pattern depth is unbounded — recursion
    // would give it a stack-overflow vector.
    enum Frame<'tree> {
        Open { node: Node<'tree>, field: Option<&'static str> },
        Close,
    }

    let mut stack: Vec<Frame<'_>> = vec![Frame::Open { node, field: None }];
    while let Some(frame) = stack.pop() {
        match frame {
            Frame::Close => buf.push(')'),
            Frame::Open { node, field } => {
                if !node.is_named() {
                    continue;
                }
                if let Some(name) = field {
                    buf.push(' ');
                    buf.push_str(name);
                    buf.push(':');
                }
                if let Some(ellipsis) = subtree_ellipsis_capture(node, src) {
                    buf.push_str(" (_) @");
                    buf.push_str(&ellipsis);
                    continue;
                }
                if let Some(meta) = metavar_at(node, src) {
                    buf.push_str(" (_) @");
                    buf.push_str(&meta);
                    continue;
                }
                // Terminal named leaves (identifier, integer_literal, etc.)
                // are constrained to exact text via `#eq?` predicates so a
                // literal in the pattern doesn't match every same-kind
                // sibling in the source.
                if node.named_child_count() == 0
                    && let Ok(text) = node.utf8_text(src)
                {
                    let n = *lit_counter;
                    *lit_counter += 1;
                    let _ = write!(buf, " ({}) @__lit{n}", node.kind());
                    let mut pred = String::new();
                    let _ = write!(pred, "(#eq? @__lit{n} \"{}\")", escape_query_string(text));
                    predicates.push(pred);
                    continue;
                }
                buf.push_str(" (");
                buf.push_str(node.kind());
                stack.push(Frame::Close);
                // Push children in reverse so the LIFO stack visits them
                // in source order. `named_child(i)` indexes the same set
                // `named_children()` iterates, so the named-child index
                // doubles as the argument to `field_name_for_named_child`.
                let count = node.named_child_count();
                for i in (0..count).rev() {
                    if let Some(child) = node.named_child(i) {
                        let field = node.field_name_for_named_child(i as u32);
                        stack.push(Frame::Open { node: child, field });
                    }
                }
            }
        }
    }
}

/// Pick the outermost-by-byte-range capture in a match: smallest start
/// byte wins; ties break to the largest end byte; final tiebreak is the
/// lowest capture index for stability across queries that differ only
/// in capture declaration order. Used when a query lacks an explicit
/// `@root` so the apply phase still picks a deterministic primary.
fn outermost_capture<'a, 'tree>(
    captures: &'a [tree_sitter::QueryCapture<'tree>],
) -> Option<&'a tree_sitter::QueryCapture<'tree>> {
    captures.iter().min_by(|a, b| {
        a.node
            .start_byte()
            .cmp(&b.node.start_byte())
            .then_with(|| b.node.end_byte().cmp(&a.node.end_byte()))
            .then_with(|| a.index.cmp(&b.index))
    })
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

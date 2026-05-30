# recast --search Mode Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add `--search` flag to recast CLI and `recast_search` MCP tool that find matches and return structured locations (file, line, column, snippet, capture name) without performing any rewrite.

**Architecture:** New `search.rs` module in `recast-core` holds the types and regex plan_search function; `structural.rs` gains `structural_search` (per-string) and `plan_structural_search` (multi-file); `json.rs` gains a `Search` report variant; the CLI binary gains `--search` dispatch; the MCP server gains `recast_search` tool. All share existing walk, guard, and pattern infrastructure.

**Tech Stack:** Rust 2024, `regex`, `rayon`, `tree-sitter`, `clap` v4, `serde`/`serde_json`, `insta` (snapshots), `tempfile` (test fixtures), `assert_cmd` (CLI integration tests), `rmcp` (MCP server).

---

## File Map

| File | Role |
|------|------|
| `crates/recast-core/src/search.rs` | **New.** `SearchMatch`, `SearchFile`, `SearchPlan`, `SearchOptions`; `plan_search`; `line_col`; `truncate_snippet`; internal helpers. |
| `crates/recast-core/src/search_tests.rs` | **New.** Unit tests for search.rs. |
| `crates/recast-core/src/structural.rs` | **Modify.** Add `compile_structural_query` helper, `structural_search` public fn, `plan_structural_search` public fn, `CompiledStructural::search` method. |
| `crates/recast-core/src/structural_tests.rs` | **Modify.** Tests for the new structural search functions. |
| `crates/recast-core/src/json.rs` | **Modify.** Add `JsonSearchMatch`, `JsonSearchFile`, `JsonReport::Search` variant, `from_search`. |
| `crates/recast-core/src/json_tests.rs` | **Modify.** Snapshot tests for search JSON output. |
| `crates/recast-core/src/lib.rs` | **Modify.** Export new types/functions from search.rs and structural.rs. |
| `crates/recast/src/main.rs` | **Modify.** `--search` flag; refactor `resolve_structural`; add `search_options()`; `run_search`, `run_structural_search`, `emit_search_results`. |
| `crates/recast/tests/cli.rs` | **Modify.** Integration tests for `--search`. |
| `crates/recast-mcp/src/server.rs` | **Modify.** `SearchArgs` struct; `recast_search` tool; update server instructions. |
| `crates/recast-mcp/src/server_tests.rs` | **Modify.** Tests for `recast_search`. |

---

## Task 1: Core types, `line_col`, `truncate_snippet`

**Files:**
- Create: `crates/recast-core/src/search.rs`
- Create: `crates/recast-core/src/search_tests.rs`
- Modify: `crates/recast-core/src/lib.rs` (add `mod search;`)

- [ ] **Step 1: Write the failing tests**

Create `crates/recast-core/src/search_tests.rs`:

```rust
#![allow(clippy::unwrap_used)]

use super::*;

#[test]
fn truncate_snippet_single_line() {
    assert_eq!(truncate_snippet("hello world"), "hello world");
}

#[test]
fn truncate_snippet_stops_at_newline() {
    assert_eq!(truncate_snippet("first line\nsecond line"), "first line");
}

#[test]
fn truncate_snippet_caps_at_200_chars() {
    let long = "a".repeat(250);
    let result = truncate_snippet(&long);
    assert_eq!(result.len(), 200);
}

#[test]
fn truncate_snippet_strips_whitespace() {
    assert_eq!(truncate_snippet("  hello  \n"), "hello");
}

#[test]
fn line_col_first_line() {
    assert_eq!(line_col("hello world", 0), (1, 1));
    assert_eq!(line_col("hello world", 6), (1, 7));
}

#[test]
fn line_col_second_line() {
    // "hello\nworld" — 'w' is at byte 6, line 2 col 1
    assert_eq!(line_col("hello\nworld", 6), (2, 1));
    // 'o' is at byte 8, line 2 col 3
    assert_eq!(line_col("hello\nworld", 8), (2, 3));
}

#[test]
fn line_col_third_line() {
    assert_eq!(line_col("a\nb\nc", 4), (3, 1));
}
```

- [ ] **Step 2: Create `search.rs` with types and helpers**

Create `crates/recast-core/src/search.rs`:

```rust
use std::path::PathBuf;

use rayon::prelude::*;

use crate::error::{Error, Result};
use crate::pattern::{CompiledPattern, PatternOptions};
use crate::plan::{check_match_counts, read_text_or_skip_binary};
use crate::walker::{WalkOptions, walk_paths};

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct SearchMatch {
    pub line: usize,
    pub column: usize,
    pub snippet: String,
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub capture: Option<String>,
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct SearchFile {
    pub path: PathBuf,
    pub matches: Vec<SearchMatch>,
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct SearchPlan {
    pub files: Vec<SearchFile>,
    pub total_matches: usize,
    pub files_scanned: usize,
}

#[derive(Debug, Clone)]
pub struct SearchOptions {
    pub pattern_options: PatternOptions,
    pub walk_options: WalkOptions,
    pub at_least: Option<usize>,
    pub at_most: Option<usize>,
    pub max_bytes: u64,
    pub max_files: usize,
}

impl Default for SearchOptions {
    fn default() -> Self {
        Self {
            pattern_options: PatternOptions::default(),
            walk_options: WalkOptions::default(),
            at_least: Some(1),
            at_most: None,
            max_bytes: 10 * 1024 * 1024,
            max_files: 1000,
        }
    }
}

/// Return (line, column) for a byte offset in `source`, both 1-indexed.
/// Column is byte-based (consistent with tree-sitter's `start_position`).
pub(crate) fn line_col(source: &str, byte_offset: usize) -> (usize, usize) {
    let prefix = &source[..byte_offset];
    let line = prefix.bytes().filter(|&b| b == b'\n').count() + 1;
    let col = match prefix.rfind('\n') {
        Some(nl) => byte_offset - nl,
        None => byte_offset + 1,
    };
    (line, col)
}

/// First line of `s`, trimmed, capped at 200 chars.
pub(crate) fn truncate_snippet(s: &str) -> String {
    let first_line = s.lines().next().unwrap_or("").trim();
    first_line.chars().take(200).collect()
}

pub fn plan_search<P: AsRef<std::path::Path>>(
    pattern: &str,
    roots: &[P],
    opts: &SearchOptions,
) -> Result<SearchPlan> {
    let compiled = CompiledPattern::compile(pattern, "", &opts.pattern_options)?;
    let files = scan(roots, opts)?;
    let files_scanned = files.len();

    let results: Vec<Result<Option<SearchFile>>> = files
        .par_iter()
        .map(|path| search_one(&compiled, path, opts))
        .collect();

    let found = collect(results)?;
    let total_matches: usize = found.iter().map(|f| f.matches.len()).sum();
    check_match_counts(total_matches, opts.at_least, opts.at_most)?;

    Ok(SearchPlan { files: found, total_matches, files_scanned })
}

pub(crate) fn scan<P: AsRef<std::path::Path>>(
    roots: &[P],
    opts: &SearchOptions,
) -> Result<Vec<PathBuf>> {
    let files = walk_paths(roots, &opts.walk_options)?;
    if files.len() > opts.max_files {
        return Err(Error::TooManyFiles { count: files.len(), limit: opts.max_files });
    }
    Ok(files)
}

pub(crate) fn collect(results: Vec<Result<Option<SearchFile>>>) -> Result<Vec<SearchFile>> {
    let mut out = Vec::new();
    for r in results {
        if let Some(f) = r? {
            out.push(f);
        }
    }
    Ok(out)
}

fn search_one(
    compiled: &CompiledPattern,
    path: &std::path::Path,
    opts: &SearchOptions,
) -> Result<Option<SearchFile>> {
    let (source, _) = match read_text_or_skip_binary(path, opts.max_bytes)? {
        Some(pair) => pair,
        None => return Ok(None),
    };

    let matches: Vec<SearchMatch> = compiled
        .regex()
        .find_iter(&source)
        .map(|m| {
            let (line, column) = line_col(&source, m.start());
            SearchMatch { line, column, snippet: truncate_snippet(m.as_str()), capture: None }
        })
        .collect();

    if matches.is_empty() {
        return Ok(None);
    }
    Ok(Some(SearchFile { path: path.to_path_buf(), matches }))
}

#[cfg(test)]
#[path = "search_tests.rs"]
mod tests;
```

- [ ] **Step 3: Register the module in `lib.rs`**

In `crates/recast-core/src/lib.rs`, after the `mod plan;` line, add:

```rust
mod search;
```

And add to the `pub use` exports (after `pub use plan::{...}`):

```rust
pub use search::{SearchFile, SearchMatch, SearchOptions, SearchPlan, plan_search};
```

- [ ] **Step 4: Run tests to verify they pass**

```bash
cargo test -p recast-core search --all-features 2>&1 | tail -20
```

Expected: all tests in `search_tests.rs` pass.

- [ ] **Step 5: Commit**

```bash
git add crates/recast-core/src/search.rs crates/recast-core/src/search_tests.rs crates/recast-core/src/lib.rs
git commit -m "feat: add SearchPlan types, plan_search, line_col, truncate_snippet"
```

---

## Task 2: Structural search — `CompiledStructural::search` + `plan_structural_search`

**Files:**
- Modify: `crates/recast-core/src/structural.rs`
- Modify: `crates/recast-core/src/structural_tests.rs`
- Modify: `crates/recast-core/src/lib.rs` (add `plan_structural_search` to exports)

- [ ] **Step 1: Write failing tests in `structural_tests.rs`**

Add to `crates/recast-core/src/structural_tests.rs`:

```rust
#[cfg(feature = "lang-rust")]
mod search_tests {
    use tempfile::TempDir;
    use std::fs;

    use crate::search::{SearchOptions, plan_structural_search};

    use super::*;

    #[test]
    fn structural_search_finds_function_names() {
        let source = "fn foo() {}\nfn bar() {}";
        let results = structural_search(
            Language::Rust,
            source,
            r#"(function_item name: (identifier) @name) @root"#,
        )
        .unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].snippet, "fn foo() {}");
        assert_eq!(results[0].line, 1);
        assert_eq!(results[1].snippet, "fn bar() {}");
        assert_eq!(results[1].line, 2);
    }

    #[test]
    fn structural_search_capture_name_is_primary() {
        let source = "fn foo() {}";
        let results = structural_search(
            Language::Rust,
            source,
            r#"(function_item name: (identifier) @name) @root"#,
        )
        .unwrap();
        // @root is primary; capture name should be "root"
        assert_eq!(results[0].capture.as_deref(), Some("root"));
    }

    #[test]
    fn structural_search_no_matches_returns_empty() {
        let source = "struct Foo {}";
        let results = structural_search(
            Language::Rust,
            source,
            r#"(function_item) @root"#,
        )
        .unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn plan_structural_search_multi_file() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("a.rs"), "fn foo() {}\nfn bar() {}").unwrap();
        fs::write(dir.path().join("b.rs"), "struct Baz {}").unwrap();

        let mut opts = SearchOptions::default();
        opts.at_least = Some(0);
        let plan = plan_structural_search(
            Language::Rust,
            r#"(function_item) @root"#,
            &[dir.path()],
            &opts,
        )
        .unwrap();

        assert_eq!(plan.total_matches, 2);
        assert_eq!(plan.files.len(), 1);
        assert_eq!(plan.files[0].matches.len(), 2);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cargo test -p recast-core structural_search --all-features 2>&1 | tail -10
```

Expected: compile error — `structural_search` and `plan_structural_search` not yet defined.

- [ ] **Step 3: Add `structural_search` and `plan_structural_search` to `structural.rs`**

In `crates/recast-core/src/structural.rs`, add these imports at the top of the file (alongside existing imports):

```rust
use crate::search::{SearchFile, SearchMatch, SearchOptions, SearchPlan, collect, scan, line_col, truncate_snippet};
```

Add the `search()` method to `CompiledStructural` (inside the existing `impl CompiledStructural` block, after the `apply()` method):

```rust
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
                m.captures
                    .iter()
                    .find(|c| c.index as usize == idx)
                    .ok_or_else(|| {
                        Error::StructuralQuery(format!(
                            "match did not bind primary capture index {idx}"
                        ))
                    })?
            }
            None => outermost_capture(m.captures).ok_or_else(|| {
                Error::StructuralQuery("match bound no captures".into())
            })?,
        };
        let pos = primary.node.start_position();
        let capture_name = capture_names
            .get(primary.index as usize)
            .copied()
            .map(ToOwned::to_owned);
        let snippet = truncate_snippet(
            &source[primary.node.start_byte()..primary.node.end_byte()],
        );
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
```

Add the two public functions after `structural_rewrite_friendly` (around line 610):

```rust
/// Run a tree-sitter Query against `source` and return all match locations
/// without applying any rewrite.
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

/// Multi-file structural search. Walks `roots`, runs `structural_search`
/// per file, and folds results into a `SearchPlan`.
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
```

Also add `use crate::plan::read_text_or_skip_binary;` to the existing imports in `structural.rs`.

- [ ] **Step 4: Export `plan_structural_search` and `structural_search` from `lib.rs`**

Find the existing structural export block in `crates/recast-core/src/lib.rs`:

```rust
pub use structural::{
    Language, StructuralOutcome, compile_friendly_query, plan_structural_rewrite,
    structural_rewrite, structural_rewrite_friendly,
};
```

Replace with:

```rust
pub use structural::{
    Language, StructuralOutcome, compile_friendly_query, plan_structural_rewrite,
    plan_structural_search, structural_rewrite, structural_rewrite_friendly, structural_search,
};
```

- [ ] **Step 5: Run tests to verify they pass**

```bash
cargo test -p recast-core --all-features 2>&1 | tail -20
```

Expected: all tests pass including the new structural search tests.

- [ ] **Step 6: Commit**

```bash
git add crates/recast-core/src/structural.rs crates/recast-core/src/structural_tests.rs crates/recast-core/src/lib.rs
git commit -m "feat: add structural_search and plan_structural_search"
```

---

## Task 3: JSON `Search` report variant

**Files:**
- Modify: `crates/recast-core/src/json.rs`
- Modify: `crates/recast-core/src/json_tests.rs`

- [ ] **Step 1: Write failing snapshot test**

Add to `crates/recast-core/src/json_tests.rs`:

```rust
#[cfg(feature = "serde")]
mod search_json_tests {
    use std::path::PathBuf;
    use insta::assert_snapshot;
    use crate::search::{SearchFile, SearchMatch, SearchPlan};
    use super::*;

    fn sample_search_plan() -> SearchPlan {
        SearchPlan {
            files: vec![
                SearchFile {
                    path: PathBuf::from("src/auth.rs"),
                    matches: vec![
                        SearchMatch {
                            line: 84,
                            column: 5,
                            snippet: "struct TokenExpiry".to_owned(),
                            capture: None,
                        },
                        SearchMatch {
                            line: 102,
                            column: 9,
                            snippet: "impl TokenExpiry {".to_owned(),
                            capture: Some("definition".to_owned()),
                        },
                    ],
                },
            ],
            total_matches: 2,
            files_scanned: 10,
        }
    }

    #[test]
    fn search_json_shape() {
        assert_snapshot!(from_search(&sample_search_plan()).to_line().unwrap());
    }

    #[test]
    fn search_json_empty() {
        let plan = SearchPlan { files: vec![], total_matches: 0, files_scanned: 5 };
        assert_snapshot!(from_search(&plan).to_line().unwrap());
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cargo test -p recast-core search_json --all-features 2>&1 | tail -10
```

Expected: compile error — `from_search` not yet defined.

- [ ] **Step 3: Add `JsonSearchMatch`, `JsonSearchFile`, `JsonReport::Search`, `from_search` to `json.rs`**

Add these structs after `JsonFile` (around line 62 of `json.rs`):

```rust
#[derive(Debug, Serialize)]
pub struct JsonSearchMatch {
    pub line: usize,
    pub column: usize,
    pub snippet: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capture: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct JsonSearchFile {
    pub path: String,
    pub matches: Vec<JsonSearchMatch>,
}
```

Add `Search` variant to `JsonReport`:

```rust
Search {
    files_scanned: usize,
    total_matches: usize,
    files: Vec<JsonSearchFile>,
},
```

Add import at the top of `json.rs`:

```rust
use crate::search::SearchPlan;
```

Add `from_search` after `from_error`:

```rust
pub fn from_search(plan: &SearchPlan) -> JsonReport<'static> {
    JsonReport::Search {
        files_scanned: plan.files_scanned,
        total_matches: plan.total_matches,
        files: plan
            .files
            .iter()
            .map(|f| JsonSearchFile {
                path: f.path.display().to_string(),
                matches: f
                    .matches
                    .iter()
                    .map(|m| JsonSearchMatch {
                        line: m.line,
                        column: m.column,
                        snippet: m.snippet.clone(),
                        capture: m.capture.clone(),
                    })
                    .collect(),
            })
            .collect(),
    }
}
```

- [ ] **Step 4: Run tests and accept snapshots**

```bash
cargo test -p recast-core search_json --all-features 2>&1 | tail -20
cargo insta review
```

Accept the generated snapshots for `search_json_shape` and `search_json_empty`.

- [ ] **Step 5: Run full test suite**

```bash
cargo test -p recast-core --all-features 2>&1 | tail -10
```

Expected: all pass.

- [ ] **Step 6: Commit**

```bash
git add crates/recast-core/src/json.rs crates/recast-core/src/json_tests.rs crates/recast-core/src/snapshots/
git commit -m "feat: add JsonReport::Search variant and from_search"
```

---

## Task 4: CLI `--search` flag

**Files:**
- Modify: `crates/recast/src/main.rs`
- Modify: `crates/recast/tests/cli.rs`

- [ ] **Step 1: Write failing integration tests**

Add to `crates/recast/tests/cli.rs`:

```rust
mod search_tests {
    use super::*;

    #[test]
    fn search_exits_zero_and_shows_matches() {
        let dir = fixture(&[("a.txt", "foo bar foo\n")]);
        recast()
            .arg("foo")
            .arg("--search")
            .arg(dir.path())
            .assert()
            .success()
            .stdout(predicate::str::contains("a.txt:1:1: foo"))
            .stdout(predicate::str::contains("2 matches in 1 file"));
    }

    #[test]
    fn search_quiet_shows_only_summary() {
        let dir = fixture(&[("a.txt", "foo\n")]);
        recast()
            .arg("foo")
            .arg("--search")
            .arg("--quiet")
            .arg(dir.path())
            .assert()
            .success()
            .stdout(predicate::str::contains("1 match in 1 file"))
            .stdout(predicate::str::is_match("^1 match").unwrap());
    }

    #[test]
    fn search_json_emits_kind_search() {
        let dir = fixture(&[("a.txt", "foo\n")]);
        let out = recast()
            .arg("foo")
            .arg("--search")
            .arg("--json")
            .arg(dir.path())
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();
        let s = String::from_utf8(out).unwrap();
        let v: serde_json::Value = serde_json::from_str(s.trim()).unwrap();
        assert_eq!(v["kind"], "search");
        assert_eq!(v["total_matches"], 1);
        assert_eq!(v["files"][0]["matches"][0]["line"], 1);
        assert_eq!(v["files"][0]["matches"][0]["column"], 1);
        assert_eq!(v["files"][0]["matches"][0]["snippet"], "foo");
    }

    #[test]
    fn search_no_match_guard_violation_exits_two() {
        let dir = fixture(&[("a.txt", "bar\n")]);
        recast()
            .arg("foo")
            .arg("--search")
            .arg(dir.path())
            .assert()
            .code(2);
    }

    #[test]
    fn search_at_least_zero_allows_no_matches() {
        let dir = fixture(&[("a.txt", "bar\n")]);
        recast()
            .arg("foo")
            .arg("--search")
            .arg("--at-least")
            .arg("0")
            .arg(dir.path())
            .assert()
            .success();
    }

    #[test]
    fn search_does_not_modify_files() {
        let dir = fixture(&[("a.txt", "foo\n")]);
        recast()
            .arg("foo")
            .arg("--search")
            .arg(dir.path())
            .assert()
            .success();
        assert_eq!(std::fs::read_to_string(dir.path().join("a.txt")).unwrap(), "foo\n");
    }

    #[test]
    fn search_conflicts_with_apply() {
        let dir = fixture(&[("a.txt", "foo\n")]);
        recast()
            .arg("foo")
            .arg("--search")
            .arg("--apply")
            .arg(dir.path())
            .assert()
            .failure();
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cargo test -p recast search_tests --all-features 2>&1 | tail -10
```

Expected: multiple failures — `--search` flag not yet known to clap.

- [ ] **Step 3: Add `--search` flag to `Cli` struct**

In the `Cli` struct in `crates/recast/src/main.rs`, find the `apply` field and add `search` nearby (logically grouped with mode flags, after `check`):

```rust
/// Find matches and report locations without rewriting. Reports
/// file:line:column:snippet. Compatible with --json, --quiet, --type,
/// all filter flags, and structural --lang/--query/--ast.
#[arg(long, action = ArgAction::SetTrue, conflicts_with_all = ["apply", "check", "script", "stdin", "recover"])]
search: bool,
```

Also update `PATTERN` and `REPLACEMENT` `required_unless_present_any` to include `"search"`:

```rust
#[arg(required_unless_present_any = ["completions", "recover", "search"])]
pattern: Option<String>,

#[arg(required_unless_present_any = ["completions", "recover", "search"])]
replacement: Option<String>,
```

- [ ] **Step 4: Add `search_options()` and refactor `resolve_structural` in `Cli` impl**

Add to `impl Cli`:

```rust
fn search_options(&self) -> SearchOptions {
    SearchOptions {
        pattern_options: self.pattern_options(),
        walk_options: WalkOptions {
            hidden: self.hidden,
            no_ignore: self.no_ignore,
            follow_symlinks: false,
            types: self.type_.clone(),
            types_not: self.type_not.clone(),
            globs: self.glob.clone(),
        },
        at_least: self.min_matches(),
        at_most: self.guard.at_most,
        max_bytes: self.guard.max_bytes,
        max_files: self.guard.max_files,
    }
}
```

Rename the existing `resolve_structural` to `resolve_structural_rewrite` (for the rewrite path). Add `resolve_structural_query` for the shared query-compilation logic:

```rust
fn compile_structural_query(cli: &Cli, lang: Language) -> Result<String> {
    if let Some(q) = cli.structural.query.as_deref() {
        Ok(q.to_owned())
    } else if let Some(pat) = cli.structural.ast_pattern.as_deref() {
        recast_core::compile_friendly_query(lang, pat).map_err(anyhow::Error::from)
    } else {
        Err(anyhow!("--query or --ast required with --lang"))
    }
}

fn resolve_structural_rewrite(cli: &Cli) -> Result<Option<(Language, String, String)>> {
    let Some(lang_name) = cli.structural.lang.as_deref() else {
        return Ok(None);
    };
    let lang = resolve_lang(lang_name)?;
    let query = compile_structural_query(cli, lang)?;
    let template = cli
        .replacement
        .clone()
        .ok_or_else(|| anyhow!("REPLACEMENT positional is the template in structural mode"))?;
    Ok(Some((lang, query, template)))
}

fn resolve_structural_for_search(cli: &Cli) -> Result<Option<(Language, String)>> {
    let Some(lang_name) = cli.structural.lang.as_deref() else {
        return Ok(None);
    };
    let lang = resolve_lang(lang_name)?;
    let query = compile_structural_query(cli, lang)?;
    Ok(Some((lang, query)))
}
```

Update `run_structural` signature to accept `template: &str` (not `&str` from `resolve_structural`'s return since the return type changed to `String`). The call site changes from:
```rust
if let Some((lang, query, template)) = structural.as_ref() {
    return run_structural(&cli, *lang, query, template);
}
```
to:
```rust
if let Some((lang, query, template)) = structural.as_ref() {
    return run_structural(&cli, *lang, query, template);
}
```
Same — just `template` is now `&String` deref'd to `&str`. No change needed there.

Also remove the old `resolve_structural` function (replaced by `resolve_structural_rewrite`). Update the call in `run()`:
```rust
let structural = resolve_structural_rewrite(&cli)?;
```

- [ ] **Step 5: Add search dispatch to `run()`**

In `run()`, add before the existing `resolve_structural_rewrite` call:

```rust
if cli.search {
    return run_search_mode(cli);
}
```

Add `run_search_mode` function after `run()`:

```rust
fn run_search_mode(cli: Cli) -> Result<u8> {
    let structural = resolve_structural_for_search(&cli)?;
    let paths = cli.paths_as_pathbufs();
    let opts = cli.search_options();
    let pool = build_pool(cli.threads).context("configure worker thread pool")?;
    pool.install(|| {
        if let Some((lang, query)) = structural {
            let plan = match plan_structural_search(lang, &query, &paths, &opts) {
                Ok(p) => p,
                Err(e) => return handle_search_error(e, cli.output.json),
            };
            emit_search_results(&cli.output, &plan)?;
            return Ok(EXIT_OK);
        }
        let pattern = cli
            .pattern
            .as_deref()
            .ok_or_else(|| anyhow!("PATTERN is required for regex search mode"))?;
        let plan = match plan_search(pattern, &paths, &opts) {
            Ok(p) => p,
            Err(e) => return handle_search_error(e, cli.output.json),
        };
        emit_search_results(&cli.output, &plan)?;
        Ok(EXIT_OK)
    })
}

fn handle_search_error(err: CoreError, as_json: bool) -> Result<u8> {
    handle_plan_error(err, as_json)
}

fn emit_search_results(out: &OutputOptions, plan: &SearchPlan) -> Result<()> {
    if out.json {
        println!("{}", json::from_search(plan).to_line()?);
        return Ok(());
    }

    let mut stdout = io::stdout().lock();

    if !out.quiet {
        if out.verbose {
            for file in &plan.files {
                writeln!(stdout, "--- {} ({} match(es)) ---", file.path.display(), file.matches.len())?;
                for m in &file.matches {
                    writeln!(stdout, "{}:{}:{}: {}", file.path.display(), m.line, m.column, m.snippet)?;
                }
            }
        } else {
            for file in &plan.files {
                for m in &file.matches {
                    writeln!(stdout, "{}:{}:{}: {}", file.path.display(), m.line, m.column, m.snippet)?;
                }
            }
        }
        writeln!(stdout)?;
    }

    let nfiles = plan.files.len();
    writeln!(
        stdout,
        "{} {} in {} {}, {} files scanned",
        plan.total_matches,
        if plan.total_matches == 1 { "match" } else { "matches" },
        nfiles,
        if nfiles == 1 { "file" } else { "files" },
        plan.files_scanned,
    )?;
    Ok(())
}
```

- [ ] **Step 6: Update imports in `main.rs`**

Add to the `use recast_core::{...}` import block:

```rust
SearchOptions, SearchPlan, plan_search,
```

And (feature-gated, inside the existing `#[cfg(any(...))]` block if present, or unconditionally since `plan_structural_search` is only called behind the `resolve_structural_for_search` path which already checks for `--lang`):

```rust
plan_structural_search,
```

Also add `json::from_search` — `json` is already imported as a module, so `json::from_search(plan)` works once the function is added to `json.rs` (done in Task 3).

- [ ] **Step 7: Run integration tests**

```bash
cargo test -p recast search_tests --all-features 2>&1 | tail -20
```

Expected: all search tests pass.

- [ ] **Step 8: Run full CLI test suite**

```bash
cargo test -p recast --all-features 2>&1 | tail -10
```

Expected: all pass.

- [ ] **Step 9: Commit**

```bash
git add crates/recast/src/main.rs crates/recast/tests/cli.rs
git commit -m "feat: add --search flag to CLI with human and JSON output"
```

---

## Task 5: MCP `recast_search` tool

**Files:**
- Modify: `crates/recast-mcp/src/server.rs`
- Modify: `crates/recast-mcp/src/server_tests.rs`

- [ ] **Step 1: Write failing tests in `server_tests.rs`**

Tests call the server method directly with constructed `Parameters<SearchArgs>`. Add a `search_args` helper and three tests:

```rust
fn search_args(pattern: &str, path: &std::path::Path) -> SearchArgs {
    SearchArgs {
        pattern: Some(pattern.to_owned()),
        lang: None,
        query: None,
        ast_pattern: None,
        paths: vec![path.to_string_lossy().into_owned()],
        literal: false,
        ignore_case: false,
        single_line: false,
        hidden: false,
        no_ignore: false,
        follow_symlinks: false,
        types: vec![],
        types_not: vec![],
        globs: vec![],
        at_least: Some(1),
        at_most: None,
        max_bytes: DEFAULT_MAX_BYTES,
        max_files: DEFAULT_MAX_FILES,
    }
}

#[tokio::test]
async fn search_tool_finds_matches() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("a.txt"), "foo bar foo\n").unwrap();

    let out = server().recast_search(Parameters(search_args("foo", dir.path()))).await.unwrap();
    let body = extract_text(out);
    assert!(body.contains("\"kind\":\"search\""), "missing kind=search: {body}");
    assert!(body.contains("\"total_matches\":2"), "expected 2 matches: {body}");
    assert!(body.contains("\"line\":1"), "expected line 1: {body}");
    assert!(body.contains("\"column\":1"), "expected column 1: {body}");
}

#[tokio::test]
async fn search_tool_guard_error_on_no_match() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("a.txt"), "bar\n").unwrap();

    let out = server().recast_search(Parameters(search_args("foo", dir.path()))).await.unwrap();
    assert!(out.is_error.unwrap_or(false), "expected isError=true: {out:?}");
}

#[tokio::test]
async fn search_tool_structural_surfaces_capture_name() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("a.rs"), "fn foo() {}\n").unwrap();

    let args = SearchArgs {
        pattern: None,
        lang: Some("rust".to_owned()),
        query: None,
        ast_pattern: Some("fn $NAME() {}".to_owned()),
        paths: vec![dir.path().to_string_lossy().into_owned()],
        at_least: Some(1),
        at_most: None,
        literal: false,
        ignore_case: false,
        single_line: false,
        hidden: false,
        no_ignore: false,
        follow_symlinks: false,
        types: vec![],
        types_not: vec![],
        globs: vec![],
        max_bytes: DEFAULT_MAX_BYTES,
        max_files: DEFAULT_MAX_FILES,
    };
    let out = server().recast_search(Parameters(args)).await.unwrap();
    let body = extract_text(out);
    assert!(body.contains("\"kind\":\"search\""), "missing kind=search: {body}");
    assert!(body.contains("\"total_matches\":1"), "expected 1 match: {body}");
    assert!(body.contains("\"capture\":"), "expected capture field: {body}");
}
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cargo test -p recast-mcp search_tool --all-features 2>&1 | tail -10
```

Expected: compile error — `recast_search` tool not defined.

- [ ] **Step 3: Add `SearchArgs` struct to `server.rs`**

Add after `StructuralArgs`:

```rust
/// Arguments for `recast_search`.
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct SearchArgs {
    /// Regex pattern to match. Required unless `lang` is set (structural search).
    #[serde(default)]
    pub pattern: Option<String>,
    /// Target language for structural search (rust, ts, tsx, js, python, bash, go, json, markdown).
    /// When set, use `query` or `ast_pattern` instead of `pattern`.
    #[serde(default)]
    pub lang: Option<String>,
    /// Tree-sitter S-expression query (structural search; mutually exclusive with `ast_pattern`).
    #[serde(default)]
    pub query: Option<String>,
    /// Friendly structural pattern with `$NAME` / `$$$NAME` placeholders (mutually exclusive with `query`).
    #[serde(default)]
    pub ast_pattern: Option<String>,
    /// Paths or globs to scan. Defaults to `["."]` if omitted.
    #[serde(default = "default_paths")]
    pub paths: Vec<String>,
    /// Treat `pattern` as a literal string (no regex metas).
    #[serde(default)]
    pub literal: bool,
    /// Case-insensitive matching.
    #[serde(default)]
    pub ignore_case: bool,
    /// Disable implicit `(?s)` so `.` no longer matches `\n`.
    #[serde(default)]
    pub single_line: bool,
    /// Include hidden files in the walk.
    #[serde(default)]
    pub hidden: bool,
    /// Disable `.gitignore` filtering.
    #[serde(default)]
    pub no_ignore: bool,
    /// Follow symlinks.
    #[serde(default)]
    pub follow_symlinks: bool,
    /// Ripgrep `--type` filter (e.g. `["rust", "ts"]`).
    #[serde(default)]
    pub types: Vec<String>,
    /// Ripgrep `--type-not` filter.
    #[serde(default)]
    pub types_not: Vec<String>,
    /// Ripgrep glob include/exclude (e.g. `["!vendor/**"]`).
    #[serde(default)]
    pub globs: Vec<String>,
    /// Require at least N matches. Defaults to 1.
    #[serde(default = "default_at_least")]
    pub at_least: Option<usize>,
    /// Require at most N matches.
    #[serde(default)]
    pub at_most: Option<usize>,
    /// Refuse files larger than this many bytes. Defaults to 10 MiB.
    #[serde(default = "default_max_bytes")]
    pub max_bytes: u64,
    /// Refuse runs touching more than this many files. Defaults to 1000.
    #[serde(default = "default_max_files")]
    pub max_files: usize,
}

impl SearchArgs {
    fn search_options(&self) -> Result<SearchOptions, McpError> {
        let pattern_options = PatternOptions {
            literal: self.literal,
            ignore_case: self.ignore_case,
            single_line: self.single_line,
        };
        let walk_options = WalkOptions {
            hidden: self.hidden,
            no_ignore: self.no_ignore,
            follow_symlinks: self.follow_symlinks,
            types: self.types.clone(),
            types_not: self.types_not.clone(),
            globs: self.globs.clone(),
        };
        Ok(SearchOptions {
            pattern_options,
            walk_options,
            at_least: self.at_least,
            at_most: self.at_most,
            max_bytes: self.max_bytes,
            max_files: self.max_files,
        })
    }

    fn paths_as_pathbufs(&self) -> Vec<PathBuf> {
        self.paths.iter().map(PathBuf::from).collect()
    }
}
```

- [ ] **Step 4: Add `recast_search` tool to `#[tool_router] impl RecastServer`**

Add after `recast_recover`:

```rust
#[tool(description = "Search for patterns across files and return structured match locations \
                   (file, line, column, snippet, capture name). Use for code navigation: \
                   finding definitions, usages, callsites — not for rewriting.\n\
                   \n\
                   Two modes:\n\
                   - **Regex search:** supply `pattern`. All regex features from \
                   `recast_preview` apply (literal, ignore_case, single_line, globs, types).\n\
                   - **Structural search:** supply `lang` + `ast_pattern` (or `query`). \
                   Capture names (e.g. `@definition`, `@name`) surface in the `capture` \
                   field so callers can distinguish definitions from usages.\n\
                   \n\
                   WHEN TO USE OVER GREP:\n\
                   - You want structured JSON output (file, line, col, snippet).\n\
                   - You want AST-aware search (exact `fn foo()` shape, not text substring).\n\
                   - You want the match-count guard — zero matches exits with an error so \
                   silent misses are impossible.\n\
                   \n\
                   OUTPUT: `{kind: \"search\", files_scanned, total_matches, files: \
                   [{path, matches: [{line, column, snippet, capture?}]}]}`\n\
                   \n\
                   EXAMPLES:\n\
                   Find all uses of `TokenExpiry` across src/:\n\
                   `{\"pattern\": \"TokenExpiry\", \"paths\": [\"src/\"]}`\n\
                   \n\
                   Find all Rust function definitions:\n\
                   `{\"lang\": \"rust\", \"ast_pattern\": \"fn $NAME() {}\", \"paths\": [\"src/\"]}`")]
async fn recast_search(
    &self,
    Parameters(args): Parameters<SearchArgs>,
) -> Result<CallToolResult, McpError> {
    let opts = args.search_options()?;
    let paths = args.paths_as_pathbufs();

    let plan = match (&args.lang, &args.query, &args.ast_pattern, &args.pattern) {
        (Some(lang_name), query, ast_pattern, _) => {
            // Structural search
            let lang = Language::from_name(lang_name).map_err(to_mcp_err)?;
            let query_str = match (query, ast_pattern) {
                (Some(_), Some(_)) => {
                    return Err(invalid_args("supply either `query` or `ast_pattern`, not both"))
                }
                (Some(q), None) => q.clone(),
                (None, Some(pat)) => compile_friendly_query(lang, pat).map_err(to_mcp_err)?,
                (None, None) => {
                    return Err(invalid_args("one of `query` or `ast_pattern` is required with `lang`"))
                }
            };
            plan_structural_search(lang, &query_str, &paths, &opts).map_err(to_mcp_err)?
        }
        (None, _, _, Some(pattern)) => {
            // Regex search
            plan_search(pattern, &paths, &opts).map_err(to_mcp_err)?
        }
        (None, _, _, None) => {
            return Err(invalid_args("either `pattern` (regex) or `lang` + `ast_pattern`/`query` (structural) is required"));
        }
    };

    Ok(CallToolResult::success(vec![Content::json(json::from_search(&plan))?]))
}
```

- [ ] **Step 5: Update imports in `server.rs`**

Add to the `use recast_core::{...}` block:

```rust
SearchOptions, plan_search, plan_structural_search,
```

- [ ] **Step 6: Update MCP server instructions to mention `recast_search`**

In `get_info()`, update the `TOOL PICK:` section to include:

```
- `recast_search` for finding definitions/usages/callsites without rewriting. \
Returns structured locations with capture names. \
Prefer over grep for structured output and AST-aware queries.\n\
```

- [ ] **Step 7: Run MCP tests**

```bash
cargo test -p recast-mcp --all-features 2>&1 | tail -20
```

Expected: all tests pass.

- [ ] **Step 8: Commit**

```bash
git add crates/recast-mcp/src/server.rs crates/recast-mcp/src/server_tests.rs
git commit -m "feat: add recast_search MCP tool"
```

---

## Task 6: Clippy, fmt, full suite, documentation updates

**Files:**
- Modify: `README.md`
- Modify: `CHANGELOG.md`

- [ ] **Step 1: Run fmt**

```bash
cargo fmt --all
```

Expected: no changes (or reformats some lines).

- [ ] **Step 2: Run clippy**

```bash
cargo clippy --workspace --all-targets --all-features -- -D warnings 2>&1 | grep -E "^error|warning\[" | head -30
```

Fix any warnings. Common issues: unused imports, missing `use` statements, `#[allow(dead_code)]` needed temporarily.

- [ ] **Step 3: Run full test suite**

```bash
cargo test --workspace --all-features 2>&1 | tail -20
```

Expected: all tests pass.

- [ ] **Step 4: Update `CHANGELOG.md`**

Add a new entry under `## [Unreleased]` (or the current WIP section):

```markdown
### Added
- `--search` flag: find match locations (file:line:column:snippet) without rewriting. Outputs grep-like human text or JSON (`kind: "search"` with per-match line/column/snippet/capture). Supports all existing filter flags, `--at-least`/`--at-most`, `--json`, `--quiet`, `--verbose`, and structural mode (`--lang` + `--ast`/`--query`).
- `recast_search` MCP tool: structured code navigation for AI agents. Regex and structural (AST-aware) search; capture names surface definition vs usage distinctions.
```

- [ ] **Step 5: Update `README.md`**

Add a `## Search mode` section (or extend the existing Usage section) showing:

```bash
# Find all occurrences of TokenExpiry
recast TokenExpiry --search src/

# Structural: find all Rust function definitions
recast --lang rust --ast 'fn $NAME() {}' --search src/

# Machine-readable output
recast TokenExpiry --search --json src/
```

- [ ] **Step 6: Commit docs**

```bash
git add CHANGELOG.md README.md
git commit -m "docs: add --search and recast_search to changelog and README"
```

---

## Self-Review Checklist

After writing this plan, verify against the spec:

- [x] `--search` flag conflicts with `--apply`, `--check`, `--script`, `--stdin`, `--recover`
- [x] `REPLACEMENT` optional when `--search` present
- [x] Structural mode search: no template required
- [x] `SearchMatch` has `line`, `column`, `snippet`, `capture: Option<String>`
- [x] `SearchOptions` omits `allow_non_convergent`, `allow_syntax_errors`
- [x] `plan_search` (regex) + `plan_structural_search` (structural) public functions
- [x] `truncate_snippet`: first line, trim, cap 200 chars
- [x] `line_col`: 1-indexed, byte-based
- [x] Structural primary capture: `@root` if present, else outermost (implemented in `CompiledStructural::search`)
- [x] `JsonReport::Search`: no `outcome` field, has `files_scanned`, `total_matches`, `files` array
- [x] Human output: `file:line:col: snippet` lines + summary
- [x] `--quiet`: summary only
- [x] `--verbose`: per-file headers
- [x] Exit codes: 0 on success, 2 on guard violated — handled via `handle_plan_error` reuse
- [x] MCP `recast_search`: supports regex and structural, no replacement/script/convergence args
- [x] MCP server instructions updated
- [x] Integration tests cover: basic search, `--json`, `--quiet`, guard violation, no file mutation, conflict with `--apply`

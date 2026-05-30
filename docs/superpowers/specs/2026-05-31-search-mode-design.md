# Search Mode Design

**Date:** 2026-05-31  
**Status:** Approved

## Summary

Add `--search` flag to recast that finds matches without computing or applying any replacement. Outputs structured match locations (file, line, column, snippet, capture name) in both human-readable and JSON forms. Complements the existing preview → apply workflow: search first to verify scope, then promote to a full rewrite by adding a replacement.

## Motivation

recast's `--diff` preview mode is framed around "what would change." Agents and humans often need a prior step: "what exists and where?" — finding all call sites of a function, all usages of a type, all files containing a pattern — without the cognitive overhead of a diff. The `--search` flag provides this as a first-class operation sharing all of recast's existing filtering, guard, and JSON machinery.

## CLI

```
recast PATTERN --search [PATHS]
recast --lang rust --ast 'fn $NAME() {}' --search [PATHS]
```

- `REPLACEMENT` positional becomes optional when `--search` is present.
- `--search` conflicts with `--apply`, `--check`, `--script`, `--stdin`, `--recover`.
- In structural mode (`--lang` + `--query`/`--ast`): `REPLACEMENT` is not required; `resolve_structural` gains a search path that omits template parsing.
- All existing filter flags apply unchanged: `--type`, `--type-not`, `--glob`, `--hidden`, `--no-ignore`, `--at-least`, `--at-most`, `--max-bytes`, `--max-files`, `--literal`, `--ignore-case`, `--single-line`.

## Core Library

### New module: `recast-core/src/search.rs`

```rust
pub struct SearchMatch {
    pub line: usize,              // 1-indexed
    pub column: usize,            // 1-indexed
    pub snippet: String,          // matched text; truncated at first \n or 200 chars
    pub capture: Option<String>,  // structural only: primary capture name
}

pub struct SearchFile {
    pub path: PathBuf,
    pub matches: Vec<SearchMatch>,
}

pub struct SearchPlan {
    pub files: Vec<SearchFile>,
    pub total_matches: usize,
    pub files_scanned: usize,
}

pub struct SearchOptions {
    pub pattern_options: PatternOptions,
    pub walk_options: WalkOptions,
    pub at_least: Option<usize>,   // default Some(1)
    pub at_most: Option<usize>,
    pub max_bytes: u64,
    pub max_files: usize,
}
```

`SearchOptions` omits `allow_non_convergent` and `allow_syntax_errors` — both are rewrite-only concepts.

### Functions

```rust
pub fn plan_search<P: AsRef<Path>>(pattern: &str, roots: &[P], opts: &SearchOptions) -> Result<SearchPlan>
pub fn plan_structural_search<P: AsRef<Path>>(lang: Language, query: &str, roots: &[P], opts: &SearchOptions) -> Result<SearchPlan>
```

`plan_structural_search` reuses query compilation but adds a `search()` method to `CompiledStructural` (no template required, returns positions + primary capture name only).

### Snippet truncation

`truncate_snippet(s: &str) -> String`: take up to the first `\n`, then cap at 200 chars. Strips leading/trailing whitespace.

### Line/column computation (regex)

Walk `source[..match_start]`, count `\n` for line (1-indexed), subtract last `\n` offset for column (1-indexed). O(n) per file, computed once for all matches via a single pass.

### Structural search

Add `CompiledStructural::search(parser, cursor, source) -> Result<Vec<(usize, usize, String)>>` returning `(start_byte, end_byte, capture_name)` tuples. Derive line/col from tree-sitter's `node.start_position()` (already `(row, column)` — zero-indexed, add 1).

**Primary capture selection:** `@root` capture if present, otherwise the outermost (lowest start byte, longest span) capture in the match — same selection logic as the rewrite path.

`plan_structural_search` accepts an already-compiled S-expression query string. Friendly `--ast` patterns are resolved to S-expression queries via `compile_friendly_query` in the CLI dispatch layer before calling into the library, matching the existing rewrite path.

## JSON Output

New `JsonReport::Search` variant in `json.rs`:

```json
{
  "kind": "search",
  "files_scanned": 312,
  "total_matches": 47,
  "files": [
    {
      "path": "src/auth.rs",
      "matches": [
        {"line": 84, "column": 5, "snippet": "struct TokenExpiry", "capture": null}
      ]
    }
  ]
}
```

No `outcome` field (no `AlreadyApplied` concept for search). Guard violations still emit `{"kind":"error",...}` as usual.

## Human Output

```
src/auth.rs:84:5: struct TokenExpiry
src/auth.rs:102:9: impl TokenExpiry {

2 matches in 1 file, 312 files scanned
```

`--quiet`: summary line only (`2 matches in 1 file, 312 files scanned`).  
`--verbose`: add per-file timing.

## MCP Tool

New `recast_search` tool in `server.rs`. `SearchArgs` struct: all fields from `RewriteArgs` except `replacement`, `script_source`, `script_path`, `allow_non_convergent`, `allow_syntax_errors`.

Tool description frames it as **code navigation**: find definitions, usages, callsites — not as a rewrite preview. Structural search surfaces capture names so agents can distinguish `@definition` from `@usage`.

## Exit Codes

Same as existing: 0 = success (matches found, or `at_least=0` and none found); 2 = guard violated (`TooFewMatches` when `at_least > 0` and zero matches).

## Files Changed

| File | Change |
|------|--------|
| `crates/recast-core/src/search.rs` | New module |
| `crates/recast-core/src/search_tests.rs` | Unit tests |
| `crates/recast-core/src/structural.rs` | Add `search()` method to `CompiledStructural` |
| `crates/recast-core/src/json.rs` | Add `JsonReport::Search` variant + `from_search()` |
| `crates/recast-core/src/json_tests.rs` | JSON schema tests for search |
| `crates/recast-core/src/lib.rs` | Export new types + functions |
| `crates/recast/src/main.rs` | `--search` flag, dispatch, human output, `resolve_structural` update |
| `crates/recast-mcp/src/server.rs` | `recast_search` tool + `SearchArgs` |
| `crates/recast-mcp/src/server_tests.rs` | MCP search tool tests |
| `crates/recast/tests/cli.rs` | Integration tests |

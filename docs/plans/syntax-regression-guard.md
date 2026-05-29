# Plan: post-apply syntax-regression guard + structural attr-aware delete

Status: **Both phases landed.** Phase A (Feature 1,
syntax-regression guard) in 0.1.13; Phase B (Feature 2, structural
attr-aware delete, `--include-leading-attrs`) in 0.1.14. Target:
`recast-core` + `recast-mcp`.
Author context: prompted by a real warp-upstream session where a
regex `recast_apply` deleting test fns left orphaned `#[test]`
attributes (triple-`#[test]`) and the breakage only surfaced 4
`cargo check`s later. Root cause was a user regex (`(?:#\[..\]\n)?` —
one attr, should've been `*` for the stack) + wrong-tool choice
(regex on item boundaries instead of `recast_structural`). recast
behaved correctly but had no way to catch that the *output* was
structurally degenerate.

This plan adds two defenses. **They cover DIFFERENT failure sets — not
redundant, and neither subsumes the other** (verified empirically,
see next section).

---

## ⚠ EMPIRICAL FINDING (verified 2026-05-22, do not re-litigate)

Ran tree-sitter-rust `has_error` against the exact bug shapes inside
recast-core. Result:

```
clean       (#[test] fn foo)            : has_error = false
stacked     (#[test] #[test] fn foo)    : has_error = false   ← THE BUG
dangling    (#[test] then })            : has_error = false
orphan_doc  (/// at end of file)        : has_error = false
```

**tree-sitter parses the orphan-`#[test]` bug as VALID syntax.** Two
attributes on one fn is syntactically legal — it's *rustc* that
rejects it ("duplicate test attribute"), a SEMANTIC check one layer
above parsing. recast has tree-sitter, not rustc.

Consequence, blunt:
- **Feature 1 (parse-error guard) does NOT catch the observed bug.**
  The broken output parses clean; `has_error` stays false; guard is
  silent. An earlier draft of this plan claimed otherwise — that claim
  was wrong and is removed.
- **Feature 2 (structural attr-aware delete) is the real fix** for the
  observed bug — it never creates the orphan.
- Feature 1 still has value, but for a NARROWER class: edits that make
  a file genuinely *unparseable* (unbalanced braces, truncated
  expressions, stray tokens from greedy regex). Not orphan-attr/doc.

Do not "fix" Feature 1 to catch orphan attrs via tree-sitter — it
structurally cannot. The only thing that catches the rustc-level class
is rustc (`cargo check`), which is out of scope (recast is not a
linter).

---

## Problem statement

`recast_apply` / `plan_rewrite` (regex + script modes) treat files as
opaque text. A regex correct on the matched span can still produce
output that is one of two broken kinds:

- **Unparseable** — unbalanced delimiters, truncated expressions,
  stray tokens (e.g. greedy `.*?\n}\n` strands a brace). tree-sitter
  flags these (`has_error` flips true). **Feature 1 catches this.**
- **Parses-but-wrong** — orphaned `#[test]` / `///` doc, attribute
  stacked on the wrong item. Valid syntax, wrong meaning; rustc
  rejects, tree-sitter does not. **Feature 1 is BLIND to this;
  Feature 2 prevents it by construction.**

The engine already runs two guards (convergence, match-count).
Feature 1 adds a third — "did the edit make the file unparseable?" —
using the already-linked tree-sitter. It is a real but partial net,
NOT a catch-all for "broken output."

---

## Feature 1 — post-apply structural re-parse guard (the NARROW net)

Catches unparseable output only. Does NOT catch orphan-attr/doc (see
empirical finding). Ship it for the unbalanced-delimiter class, not
as a fix for the observed bug.

### Behavior

For every `FileChange` whose path maps to a compiled tree-sitter
grammar:

1. Parse the **pre-image** (original on-disk text), count error nodes
   `E_before` (`Node::has_error` walk, or `Tree::root_node().descendant_count`
   over `is_error()` / `is_missing()` nodes).
2. Parse the **post-image** (`change.after`), count `E_after`.
3. If `E_after > E_before` → the rewrite *introduced* new syntax
   errors. Abort the whole apply (or refuse the plan) with a new
   typed error.

Count-delta, NOT absolute. Files with pre-existing parse errors
(in-progress code, macro-heavy spans tree-sitter chokes on,
conditional `cfg`) must not trip the guard — only *new* breakage
matters.

### Where it hooks

- **Plan-time (preferred):** run the guard inside the planner, after
  `process_one` produces `change.after`, before the plan is returned.
  Means `recast_preview` surfaces the regression too, not just
  `recast_apply`. Add to `plan.rs::finalize_plan` or as a step in
  `process_one` (`crates/recast-core/src/plan.rs:248` region).
- Structural mode (`plan_structural_rewrite`) is lower-risk (it's
  already AST-driven) but should still run the guard — a bad template
  can still emit broken text. Wire in `structural.rs::plan_one`
  (`crates/recast-core/src/structural.rs:384` region).

Plan-time is better than commit-time: catches it in dry-run, no
files touched, agent iterates before any write.

### The missing piece — extension → Language mapping

**Gap:** `Language::from_name` (`structural.rs:54`) maps CLI names
(`"rust"`, `"rs"`) → `Language`. There is **no** path/extension →
`Language` inference. The guard needs it: given
`change.path = "foo.rs"`, pick `Language::Rust`.

Add:

```rust
impl Language {
    /// Infer grammar from a file extension. Returns None for
    /// extensions without a compiled grammar (guard is skipped for
    /// those files — text rewrite passes through unchecked).
    pub fn from_path(path: &Path) -> Option<Self> {
        let ext = path.extension()?.to_str()?;
        match ext {
            #[cfg(feature = "lang-rust")]   "rs" => Some(Language::Rust),
            #[cfg(feature = "lang-ts")]     "ts" => Some(Language::TypeScript),
            #[cfg(feature = "lang-ts")]     "tsx" => Some(Language::Tsx),
            #[cfg(feature = "lang-js")]     "js" | "mjs" | "cjs" | "jsx" => Some(Language::JavaScript),
            #[cfg(feature = "lang-python")] "py" | "pyi" => Some(Language::Python),
            #[cfg(feature = "lang-bash")]   "sh" | "bash" => Some(Language::Bash),
            #[cfg(feature = "lang-go")]     "go" => Some(Language::Go),
            #[cfg(feature = "lang-json")]   "json" => Some(Language::Json),
            #[cfg(feature = "lang-md")]     "md" | "markdown" => Some(Language::Markdown),
            _ => None,
        }
    }
}
```

Keep the ext list in sync with `from_name` aliases. Note `tsx`/`jsx`
disambiguation: `.tsx` → Tsx grammar, `.jsx` → JavaScript (tree-sitter
JS grammar handles jsx).

Guard skips (returns Ok) when `from_path` is None OR when the crate
was built without the relevant `lang-*` feature — so a
`--no-default-features` regex-only build keeps working, just without
the guard.

### Error variant

`crates/recast-core/src/error.rs`:

```rust
#[error("rewrite introduced {new_errors} new syntax error(s) in {path} \
         ({lang}); pass allow_syntax_errors to override")]
SyntaxRegression { path: PathBuf, lang: &'static str, new_errors: usize },
```

Add matching `ErrorKind::SyntaxRegression` + the arm in
`Error::kind()` (`error.rs:101`-region match — it's exhaustive, so the
compiler forces the addition). Snapshot tests in `json_tests.rs`
(`error_kind_covers_every_error_variant`, the `error_json_*` set) need
a new case.

### Escape hatch

New `PlanOptions` field `allow_syntax_errors: bool` (default `false`).
Mirrors `allow_non_convergent`. Lets the user override for the
genuine case where the rewrite is mid-refactor and intentionally
leaves the file un-parseable (rare, but the override must exist or
the guard becomes a footgun of its own).

CLI: `--allow-syntax-errors` flag in `main.rs` GuardOptions.
MCP: `allow_syntax_errors` arg on `RewriteArgs` + `StructuralArgs`.

### Cost

One extra parse per *changed* file (not per scanned file — only files
that actually differ hit the guard). Parsing is ~MB/ms with
tree-sitter; negligible vs the IO the apply already does. Pre-image
parse can reuse the text the planner already read (it has `before` in
`process_one` scope — thread it through rather than re-reading).

### Edge cases / decisions

- **Pre-image already broken, post-image equally broken** →
  `E_after == E_before` → pass. Correct: we didn't make it worse.
- **Pre-image broken, post-image fixed** → `E_after < E_before` →
  pass. (Rewrite improved parseability.)
- **Markdown / JSON** — these grammars rarely "error" in the Rust
  sense; guard is mostly inert for them but harmless.
- **Macro-heavy Rust** — tree-sitter-rust does parse `macro_rules!`
  and most macro invocations; some exotic ones produce error nodes
  even when rustc is happy. Count-delta handles this: pre and post
  both carry the same macro-induced errors, delta is 0.
- **Non-UTF8 / binary** — already skipped upstream by
  `read_text_or_skip_binary`; never reaches the guard.
- **Partial-file grammar mismatch** (e.g. a `.rs` file that's actually
  a template) — worst case a false positive; the `allow_syntax_errors`
  override is the release valve.

---

## Feature 2 — structural "include leading attrs/docs" (the REAL fix for the observed bug)

### Behavior

`recast_structural` deleting/replacing an item should optionally
extend the match range to swallow contiguous leading
`attribute_item` / doc-comment siblings, so deleting a fn also deletes
its `#[test]` + `///` lines. Tree-sitter knows these are sibling nodes
preceding the item — no attr-line hand-counting.

New `StructuralArgs` field `include_leading_attrs: bool`. Default
**true** when the rewrite is a deletion (template empties the node)?
— No, keep default explicit/false to avoid surprising existing
callers; document that delete-shaped rewrites usually want it true.
(Decide during impl; lean false-default for least surprise.)

### Where

`structural.rs` Hit construction (`apply` / `emit_node` region,
~`structural.rs:177`-`213`). When the flag is set and the primary
capture node has preceding siblings of kind `attribute_item` /
`line_comment` (doc) / `block_comment` (doc), walk backward over the
contiguous run and move `Hit.start` to the first such sibling's
`start_byte`. Stop at the first non-attr/non-doc sibling or a blank
line gap (preserve intentional separation — decision: contiguous
only, blank line breaks the run).

### Relation to Feature 1

These are DISJOINT, not symptom-vs-cause of the same thing:
- Feature 1 catches *unparseable* output (any mode). Blind to orphan
  attrs (they parse clean — verified).
- Feature 2 prevents orphan attrs/docs (structural mode only). The
  only one of the two that addresses the observed bug.

An agent that misuses *regex* to delete code-with-attributes is helped
by NEITHER for the orphan case (F1 blind, F2 is structural-only) — the
mitigation there is the doc/threshold nudge already shipped in 0.1.12
("shape-sensitive change → use recast_structural"). F2 only pays off
once the agent is actually in structural mode.

---

## Out of scope (explicitly NOT doing)

- **Orphaned-helper / dead-code detection.** Deleting a test whose
  private helper goes unused is call-graph fallout, not a rewrite
  concern. Right tool is `cargo check` / `cargo fix`. Pushing recast
  here is scope creep.
- **Regex-mode advisory hint** ("matched span borders an attribute —
  consider structural mode"). Soft nudge; redundant once guard 1
  catches the hard case at apply-time. Revisit only if guard-1
  false-negative data shows a gap.
- **Name-set + signature-wildcard structural matching** (match "any
  fn whose name ∈ {A,B,C}, any signature" in one call). Real
  ergonomics win but bigger scope (friendly-form parser changes).
  Separate plan if survey signal shows the multi-fn-delete pattern is
  common.

---

## Implementation order + versioning

NOTE on ordering: B is the real fix for the observed bug; A is a
narrower net for a different class. A-first is still defensible —
it's simpler, language-agnostic, needs no new structural semantics,
and the 0.1.12 doc-nudge already steers the agent toward structural
mode for the shape-sensitive case in the meantime. Starting with A
per decision; B follows. If priorities flip, B can go first
independently — they don't depend on each other.

**Phase A — Feature 1 (guard), ship as `0.1.13`:**

1. `Language::from_path` + unit tests (`structural_tests.rs`).
2. Error-node counter helper `fn count_error_nodes(lang, src) -> usize`
   in `structural.rs` (parse, walk tree, count `is_error || is_missing`).
3. `PlanOptions::allow_syntax_errors` field + `Default`.
4. Guard call in `plan.rs::process_one` (regex/script) — thread
   `before` through, parse both, compare, emit `SyntaxRegression`.
5. Same guard in `structural.rs::plan_one`.
6. `Error::SyntaxRegression` + `ErrorKind` + `kind()` arm.
7. CLI `--allow-syntax-errors` (`main.rs` GuardOptions + `plan_options`).
8. MCP `allow_syntax_errors` on `RewriteArgs` + `StructuralArgs` +
   thread into `plan_options()`.
9. MCP tool-description note: "rewrites that introduce new syntax
   errors are rejected; pass allow_syntax_errors to override."
10. Tests: regex that STRANDS A BRACE (unbalanced) → `SyntaxRegression`;
    same with `allow_syntax_errors:true` → succeeds; pre-broken file
    stays passable; non-grammar extension skips guard; snapshot
    updates for the new error kind. NOTE: an orphan-`#[test]` test
    would FAIL (parses clean) — do not write that as a guard-1 test.
11. CHANGELOG `[0.1.13]`, AGENTS.md status bump, docs/safety.md +
    structural-mode.md note.

New `Error` variant + new `PlanOptions` field + new MCP args = additive
public API. Patch bump defensible pre-1.0 (AGENTS.md §11 "break freely
until 1.0"), but it's really minor-shaped — could argue `0.2.0`.
Recommendation: `0.1.13`, additive, opt-out guard on by default.

**Phase B — Feature 2 (attr-aware structural), ship as `0.1.14`:**

12. `include_leading_attrs` on `StructuralArgs` + `plan_structural_rewrite`
    signature (or fold into a structural-opts struct to avoid arg
    sprawl — see DRY note).
13. Backward sibling-walk in Hit construction + tests (delete fn with
    `#[test]` + `///` → both removed; blank-line gap stops the run;
    multiple stacked attrs all removed).
14. CLI `--include-leading-attrs` (structural mode only).
15. CHANGELOG `[0.1.14]`, docs.

---

## Test matrix (Feature 1)

| Case | Input | Expect |
|---|---|---|
| Stranded brace | regex deletes `fn f() {` line, leaves body + `}` | `SyntaxRegression`, new_errors ≥ 1 |
| Override | same + `allow_syntax_errors:true` | success |
| Truncated expr | regex chops a `let x = ` mid-statement | `SyntaxRegression` |
| Clean delete | regex deletes a whole balanced fn | success, 0 new errors |
| Orphan `#[test]` (NEGATIVE) | regex deletes fn body, leaves `#[test]` | **success** — parses clean, guard CANNOT see it. Documents the limit. |
| Pre-broken | file already has parse error, rewrite touches elsewhere | success (delta 0) |
| Pre-broken worsened | file pre-broken, rewrite adds a 2nd break | `SyntaxRegression` (delta +1) |
| Non-grammar ext | rewrite on `.txt` / `.toml` | guard skipped, success |
| Feature-off build | `--no-default-features` (no lang-*) | guard inert, success |
| Balanced edit | normal rename, no structural change | success |
| Structural template breakage | `recast_structural` template emits unbalanced text | `SyntaxRegression` |

The orphan-`#[test]` NEGATIVE row is deliberate — it locks in the
known limitation so nobody later "fixes" the guard expecting it to
fire. It won't. Verified.

Property test: arbitrary pattern/replacement on a valid `.rs` source
never panics in the guard (parse failure ≠ panic).

---

## Files touched (grounded refs)

- `crates/recast-core/src/structural.rs` — `Language::from_path`
  (new, near `from_name:54`); `count_error_nodes` (new);
  `ts_language:78` reused; guard in `plan_one:384`.
- `crates/recast-core/src/plan.rs` — `process_one:248` guard hook;
  `PlanOptions` struct + `Default`; `finalize_plan`.
- `crates/recast-core/src/error.rs` — `Error` enum:12, `ErrorKind`:77,
  `kind()` match:101.
- `crates/recast-core/src/json.rs` + `json_tests.rs` — new error-kind
  snapshot.
- `crates/recast/src/main.rs` — `GuardOptions`, `plan_options()`,
  `--allow-syntax-errors`.
- `crates/recast-mcp/src/server.rs` — `RewriteArgs` + `StructuralArgs`
  new field, `plan_options()` builders, tool descriptions.
- `crates/recast-core/src/lib.rs` — re-export `Language::from_path` if
  needed by binary/mcp (it's already `pub`).

## DRY / quality watch

- `RewriteArgs` and `StructuralArgs` in the MCP server already
  duplicate the walk/guard option fields. Adding `allow_syntax_errors`
  to both repeats the pattern — consider a shared `GuardOpts` substruct
  flattened via `#[serde(flatten)]` before this grows further. Flag,
  don't necessarily fix in this pass.
- `plan_one` (structural) and `process_one` (regex) will both grow a
  guard call — factor the parse-and-compare into one
  `guard_syntax(path, before, after, opts) -> Result<()>` helper in
  `structural.rs`, called from both. No copy-paste.

## Open questions

1. Guard default on or off? Recommend **on** (the whole point is
   catch-by-default). Override via `allow_syntax_errors`.
2. Abort-whole-apply vs skip-bad-file? Recommend **abort whole plan**
   at plan-time (atomic mental model: the plan is rejected, nothing
   half-applies). Skipping individual files silently is worse.
3. Patch (`0.1.13`) vs minor (`0.2.0`)? Additive API, pre-1.0 — patch
   is fine, but it's a behavior change (previously-accepted rewrites
   now rejected). Lean `0.1.13` + loud CHANGELOG; the new guard can be
   disabled per-call.
4. Error-node count method: `has_error()` is a cheap bool but doesn't
   count; need an explicit cursor walk counting `is_error()` +
   `is_missing()` nodes. Confirm tree-sitter `TreeCursor` walk is the
   right primitive (it is — `node.is_error()` per visited node).

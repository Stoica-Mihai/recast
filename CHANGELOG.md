# Changelog

All notable changes to `recast` land here. Format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/); the project
follows [Semantic Versioning](https://semver.org/spec/v2.0.0.html) once a
1.0.0 release exists.

## [Unreleased]

### Added

- `--search` flag: find match locations (file:line:col:snippet) without rewriting. Outputs grep-like human text or structured JSON (`kind: "search"`). Supports all filter flags, `--at-least`/`--at-most` guard, `--json`, `--quiet`, `--verbose`, and structural mode (`--lang` + `--ast`/`--query`).
- `recast_search` MCP tool: structured code navigation returning per-match line/column/snippet/capture. Supports regex and tree-sitter AST-aware queries; capture names distinguish definition from usage captures.

## [0.1.14] — 2026-05-29

### Added

- **Structural attr-aware delete (`--include-leading-attrs`).** In
  structural mode, deleting or replacing an item now optionally extends
  each match backward over the contiguous run of preceding
  `attribute_item` / doc-comment siblings — so deleting a function also
  removes its `#[test]` / `#[cfg(...)]` / `///` lines instead of
  orphaning them. This is the real fix for the orphaned-attribute class
  that the (syntactic-only) syntax-regression guard cannot catch: an
  orphaned `#[test]` parses clean.
  - A blank line ends the run (an attribute separated by an empty line
    is treated as detached and left in place). Plain `//` / `/* */`
    comments are never swallowed — only doc comments (`///`, `//!`,
    `/**`, `/*!`).
  - Node kinds are tree-sitter-rust's (`attribute_item`); languages
    without those kinds never extend (no-op), so the flag is safe to set
    in any language.
  - CLI `--include-leading-attrs` (structural mode only); MCP
    `include_leading_attrs: true` on `recast_structural`. Default off.

## [0.1.13] — 2026-05-29

### Added

- **Syntax-regression guard.** A third planner guard (alongside the
  match-count and convergence checks): for every changed file whose
  extension maps to a compiled tree-sitter grammar (rust, ts, tsx, js,
  py, sh, go, json, md), recast re-parses the post-image and rejects the
  rewrite if it introduces *new* parse errors relative to the pre-image.
  Catches greedy regex that strands a brace or truncates an expression
  before anything is written to disk. The check is a count delta — a
  file that was already unparsable stays acceptable as long as the
  rewrite doesn't make it worse. Runs at plan time, so `recast_preview`
  / `--diff` surface the regression too.
  - **Limitation (by design):** the guard is *syntactic*, not semantic.
    An orphaned `#[test]` left on the wrong item parses clean and is NOT
    caught — that is a rustc-level error one layer above tree-sitter. Use
    structural mode for shape-sensitive deletes.
  - Opt out per run with `--allow-syntax-errors` (CLI) /
    `allow_syntax_errors: true` (MCP `recast_preview` / `recast_apply` /
    `recast_structural`). On by default.
  - New `Error::SyntaxRegression` variant + `ErrorKind::syntax_regression`
    for JSON output.
  - New `Language::from_path` maps a file extension to its grammar; the
    guard is skipped (rewrite passes through unchecked) for extensions
    with no compiled grammar and for `--no-default-features` builds.

## [0.1.12] — 2026-05-22

Real-session feedback from a Claude Code user surfaced two
documentation gaps in v0.1.11's tool descriptions. Doc-only patch
release.

### Changed

- **MCP tool descriptions document the `\\n` footgun.**
  `recast_apply` / `recast_preview` `replacement` is a regex
  template — backreferences (`$1`, `${name}`) are interpolated but
  C-style escape sequences (`\\n`, `\\t`) are NOT decoded. An agent
  passing `"foo\\nbar"` ends up writing literal backslash-n on disk,
  not a newline. Descriptions now carry a `FOOTGUN —` block telling
  callers to use real LFs in the JSON string value, not the `\\n`
  escape.
- **Refined "when to use" threshold.** The flat "3+ files" rule
  under-served simple 4-site edits where `Edit` is genuinely faster.
  Descriptions now distinguish: **5+ sites** for simple text changes;
  **any count** for shape-sensitive changes (use
  `recast_structural`); **any count** when atomicity is required.
  Added an explicit `WHEN NOT TO USE` block so the agent can opt
  out without abandoning recast entirely for the harder cases.

## [0.1.11] — 2026-05-22

Agent-adoption pass for `recast-mcp`: real-world Claude Code survey
showed the agent defaulting to `Edit` / `write_file` loops even with
the MCP server installed. Three documentation-shaped fixes encode the
decision rule at every surface the agent reads.

### Changed

- **MCP tool descriptions rewritten with worked examples + decision
  rule.** `recast_preview`, `recast_apply`, `recast_structural`, and
  `recast_recover` now embed 2-3 concrete JSON invocations each plus
  the "if 3+ files, call recast first" trigger. The LLM tool-ranker
  reads these on every selection step; example-rich descriptions
  reduce uncertainty enough to flip the default away from `Edit`.
- **`ServerInfo.instructions` expanded** from one sentence into a
  decision-rule + two-step-workflow block. The MCP client injects
  this into its system prompt during handshake, so the heuristic
  reaches the agent before any tool call is considered.

### Added

- **`docs/src/agent-rules.md`** — copy-pasteable rules snippet for
  every common agent runtime (Claude Code, Cursor, Continue, Cline,
  Aider). Users drop the block into their project's `AGENTS.md` /
  `.cursorrules` / equivalent so the in-project system prompt
  doubles up with the MCP server's instructions field. Linked from
  both the root README and the `recast-mcp` crates.io README.

## [0.1.10] — 2026-05-22

Hotfix: replace the placeholder root README on the `recast-mcp`
crates.io page with a dedicated MCP-focused README. Otherwise
identical to 0.1.9.

### Fixed

- **`recast-mcp` crates.io listing showed CLI install instructions.**
  All three crates pinned `readme = "../../README.md"`, so the
  crates.io page for the MCP server told you to `cargo install
  recast-cli`. Added `crates/recast-mcp/README.md` with the MCP
  install + client-config + tool-list content and pointed the crate
  manifest at it. `recast-cli` and `recast-core` still share the
  root README (close enough to their actual landing-page audience).

## [0.1.9] — 2026-05-22

Post-`0.1.8` follow-through: a deep audit pass over the previously
untouched modules (walker, lockfile, json, rewrite, script, parallel),
the `recast-mcp` server crate for MCP-aware AI agents, a nightly
fuzz workflow, a criterion regression gate on PRs, and the symlink +
concurrent-apply regression tests that pin down behavior the audit
exposed.

### Added

- **`recast-mcp` crate.** Model Context Protocol server that exposes
  the recast engine to MCP-aware AI agents (Claude Desktop, Cursor,
  Continue, Cline, custom MCP clients). Library-linked against
  `recast-core` — no subprocess, no CLI string assembly, no JSON
  parse round-trip. Speaks JSON-RPC over stdio per MCP convention.
  Four tools, 1:1 with the planner API: `recast_preview`,
  `recast_apply`, `recast_structural`, `recast_recover`. Typed
  argument schemas (via `rmcp` 1.7 + `schemars` 1.x) so agents can't
  malform calls; engine errors propagate as `McpError` with the
  typed `kind` discriminator preserved in the payload.
- **Walker symlink regression tests.** Cover cycle detection,
  dangling links, escape-root behavior with/without
  `follow_symlinks`, and the gitignore interaction when following
  links. Symlink semantics now documented in `walker.rs` module
  doc-comment.
- **Concurrent-apply integration tests** in
  `crates/recast/tests/concurrency.rs`. External fs2 lock on
  `.recast.lock` forces `recast --apply` to exit non-zero with the
  `Locked` error; `--force` bypasses the guard.
- **Nightly fuzz workflow** (`.github/workflows/fuzz.yml`) runs three
  cargo-fuzz targets (compile_friendly_query,
  structural_rewrite_friendly, pattern_compile_convergence) for 60
  minutes each per day. Corpus cached across runs so coverage
  compounds.
- **Criterion regression gate** (`.github/workflows/bench.yml`) runs
  benches on every PR, diffs against the `main` baseline stored on
  `gh-pages`, comments + fails the check at >50% regression.

### Changed

- **Walker uses `WalkParallel`** instead of the single-threaded
  iterator. Honors the surrounding rayon pool's thread count so
  `--threads N` is now respected for the walk phase as well.
- **`label_for_path` fast path** for absolute / plain-relative paths
  — skips the PathBuf rebuild when no leading `./` needs stripping.
  Saves one allocation per labeled file in the planner's hot loop.
- **`from_apply` routes through `header(plan)`** to dedupe the
  JsonHeader construction; the two header builders no longer drift.

### Fixed

- **Lockfile error misclassification.** `acquire_workspace_lock`
  used to fold every `io::Error` from `try_lock_exclusive` into
  `Error::Locked`, hiding permission-denied / ENOSPC / EIO behind
  "another recast is already applying". Now matches on
  `ErrorKind::WouldBlock` and only that variant maps to `Locked`;
  every other variant propagates as `Error::Io` with the underlying
  source preserved.
- **Workspace lock derivation canonicalizes input paths** and locks
  at the deepest common ancestor. Two `--apply` invocations against
  the same tree from different CWDs (or one against `src/`, one
  against `src/sub/`) now share one `.recast.lock` instead of
  proceeding in parallel.
- **EXDEV fallback** in `commit_one`, `rollback_committed`, and
  `recover_sweep`. A new `rename_with_exdev_fallback` helper catches
  `ErrorKind::CrossesDevices` from `fs::rename` and degrades to
  `copy + sync_all + remove_file`. Same-directory renames inside a
  normal filesystem never hit this path; overlayfs, unionfs, FUSE
  backends, and certain container layouts can return EXDEV even for
  lexically-sibling renames, so the apply now degrades cleanly
  rather than aborting.

## [0.1.8] — 2026-05-22

Post-`0.1.7` follow-through: residual cleanup-pass items + a CI
deprecation fix flagged by the 0.1.7 release run, plus a deep
correctness/durability audit pass over `commit`, `structural`, and
the CLI dispatch layer.

### Changed

- **`commit::apply_inner` hidden behind `#[cfg(test)]`.** Was
  `pub(crate)` so the rollback test could inject a mid-commit
  failure; that leaked the test-only seam into the production API.
  Production `apply_changes` and `commit_all` carry no hook
  references; `apply_inner` + `commit_all_with` exist only under
  `cfg(test)`. The two paths share a `finalize_apply` helper, and
  `commit_all` now delegates to a single `commit_all_with` impl with
  a no-op hook closure instead of duplicating the loop body.
- **Per-apply `NonceGen`** replaces the static `AtomicU64` counter
  inside `fn nonce()`. The struct is constructed once per
  `apply_changes` call and threaded by reference through stage and
  commit phases; sibling-filename uniqueness across rayon workers
  is unchanged, but the state is scoped to the invocation instead
  of living in static mutable memory.
- **`NonceGen::new` samples `SystemTime::now()` once** and stores
  it as a precomputed mix seed. The previous shape sampled the
  clock inside every `next()` call, so a 1k-file apply did 1k extra
  syscalls just to disambiguate sibling filenames.
- **`emit_diff` / `emit_apply` narrowed** from `&Cli` to
  `&OutputOptions, &Plan`. Dropped the transitive dependency on
  GuardOptions and StructuralCli substructs that those helpers
  never touched.
- **`--threads N` honored across the whole pipeline.** Previously
  only the regex planner ran inside the user-scoped rayon pool;
  scripted-plan, structural-plan, and the commit/stage phase all
  fell back to the global pool. The CLI now installs the pool once
  and wraps every parallel phase, regardless of mode.
- **Primary capture fallback in `structural::apply` is deterministic.**
  When a query lacks an explicit `@root`, the apply phase now picks
  the outermost-by-byte-range capture (smallest start, then largest
  end, then lowest capture index) instead of "the capture with the
  largest index", which was declaration-order dependent.

### Fixed

- **`recover_sweep` skips parentless sibling entries.** Previously
  fell back to `PathBuf::new()` as the HashMap target key when
  `path.parent()` returned None, silently bucketing unrelated
  parentless siblings together. Skip with a `trace!` line instead
  so recovery never makes cross-target decisions.
- **`recover_sweep` aggregates per-group errors** instead of
  bailing on the first failure. The user invokes `--recover`
  precisely when the tree is in a partial state; aborting at the
  first bad group left the rest unreconciled. All groups are
  attempted; the first error is propagated after the sweep so the
  exit code still reflects failure.
- **`structural::compile_friendly_query` passes child index, not
  node id, to `field_name_for_named_child`.** Was calling
  `field_name_for_child(child.id() as u32)`, conflating the
  pointer-derived opaque identifier with the positional child
  index. Generated `--ast` queries could silently drop / mis-name
  field annotations as a result.
- **UTF-8 corruption in three byte walkers.**
  `structural::parse_template`, `structural::substitute_metavars`,
  and `pattern::CompiledPattern::replacement_probe` advanced one
  byte at a time and pushed each raw byte as `char`, mojibaking
  every multibyte codepoint. All three now advance by a full UTF-8
  scalar via a shared `template_scan::utf8_char_len` helper.
- **`rollback_committed` no longer unlinks the target before
  restoring the backup.** The explicit `remove_file` was redundant
  (`rename` replaces atomically on Unix) and opened a window where
  neither the old nor the new content existed on disk.
- **`finalize_apply` fsyncs parent dirs before deleting backups.**
  The previous order unlinked the safety net before the rename
  batch's directory entries were durable; a crash in the window
  could leave the target absent with the backup already gone.
- **`structural::emit_node` is iterative.** The recursive
  implementation grew the stack proportionally to the depth of the
  user's `--ast` pattern AST, giving pathological patterns a
  stack-overflow vector. Replaced with an explicit `Open` / `Close`
  frame stack — identical output, bounded by heap.

### CI

- **`actions/checkout` / `actions/upload-artifact` /
  `actions/download-artifact` bumped from `@v4` to `@v5`** across
  audit, ci, docs, and release workflows. Closes the Node 20
  deprecation annotation surfaced by the v0.1.7 release run before
  the 2026-06-02 forced-Node-24 cutover.

### Removed

- **Windows as a supported target.** No longer running
  `cargo test` on `windows-latest`; no longer cross-compiling
  `x86_64-pc-windows-msvc` artifacts; no longer carrying the
  `#[cfg(windows)]` / `#[cfg(unix)]` carve-outs in source. The
  release matrix shrinks from seven targets to six (Linux gnu/musl
  × x86_64/aarch64 plus macOS x86_64/aarch64). The parent-directory
  fsync, the symlink walker tests, and the permission-preservation
  test all assume unix unconditionally now. `label_for_path` drops
  the backslash-to-forward-slash translation it carried for Windows
  diff headers.

## [0.1.7] — 2026-05-21

Post-`0.1.6` cleanup pass: targeted perf wins across the structural,
scripted, and commit pipelines plus a round of error-schema and DRY
hardening surfaced by an internal review.

### Performance

- **Structural mode is no longer per-file recompiled.** The tree-sitter
  `Query`, capture-index table, and rewrite template are built once
  per invocation as a shared `CompiledStructural`; only the
  `Parser` + `QueryCursor` are per-thread. `plan_structural_rewrite`
  now drives per-file work via `rayon par_iter().map_init(...)`. The
  rewrite template is pre-parsed into `Vec<TemplatePart::{Literal,
  Capture { index, name }}>` so capture-name lookup at match time is
  an index hit, not an O(N) byte scan.
- **`commit::stage_all` parallel.** Per-file write + `sync_all` runs
  on rayon workers so kernel fsync overlaps; commit phase stays serial
  for deterministic rollback. Local measurement: 500-file apply ~9ms,
  1000-file ~15ms (NVMe, ext4).
- **`plan_rewrite_scripted` parallel.** Each rayon worker gets a fresh
  sandboxed Rhai `Engine` via `ScriptRewriter::fresh()`; the compiled
  AST is shared. Enables `sync` feature on the rhai dep (Rc → Arc
  internally).
- **Single-pass match count in `rewrite_text`.** Drops the redundant
  `find_iter().count()` scan that ran before `replace_all` and the
  `before.to_owned()` clone on every changed file.
- **Cheaper regex convergence probe.** The per-file idempotency check
  uses `Regex::find_iter().count()` on the post-image instead of a
  full second `replace_all` round.
- **HashSet parent-dir dedup in fsync.** `best_effort_fsync_parents`
  was O(N²) via `Vec<PathBuf>::iter().any(...)`; now `HashSet<&Path>`.
- **Planner peak memory ~halved.** `FileChange::before` (full
  pre-image per changed file) is dropped — it was set during diff
  rendering and never read afterward. Worst-case (max_bytes 10 MiB ×
  max_files 1000) drops ~10 GiB from peak resident memory.
- **Hot-path micro-allocations trimmed.** `label_for_path` skips the
  `\` → `/` scan on Unix where it can't matter; structural splice
  pre-reserves `source.len() + (replacement - range) delta` so the
  output `String` doesn't realloc when matches grow text;
  `emit_node` writes literal-terminal predicates with `write!`
  instead of three intermediate `format!` Strings per terminal.

### Changed

- **Wire JSON field order shifted.** Plan / Apply / Check reports now
  emit `outcome`, `files_scanned`, `total_matches` as the shared
  prefix (extracted into `JsonHeader` with `#[serde(flatten)]`)
  followed by the mode-specific count. JSON members are unordered
  per spec so this is semantically identical, but the wire bytes do
  reorder. Snapshot fixtures updated.
- **`Error::kind()` returns `ErrorKind` directly.** The
  `Error → ErrorKind` mapping lives on `impl Error` instead of in
  `json::error_kind()`; the JSON module re-exports `ErrorKind` from
  `error` for back-compat.
- **`Language::from_name` returns `Result<Self, Error>`** instead of
  `Option<Self>`. Unknown names surface as `Error::UnknownLanguage`
  from the library boundary.
- **`RewriteOutcome` shape.** Dropped the `before` field — the caller
  already owns the pre-image. `changed()` method removed; compare
  `outcome.after != before` at the call site if needed.
- **`FileChange.before` field removed.** Was `#[serde(skip)]` and
  only used to render the diff during planning; `apply_changes` has
  always read from `after`. Pre-1.0 break for anyone reading the
  field directly; `FileChange.diff` carries the rendered diff.
- **`handle_plan_error` returns `Result<u8>`** instead of `u8` so a
  failed JSON serialize during error reporting is now observable
  rather than silently dropped.
- **CLI options grouped into substructs.** Output (`--diff` /
  `--json` / `--quiet` / `--verbose`), guards (`--at-least` /
  `--at-most` / `--allow-non-convergent` / `--max-bytes` /
  `--max-files`), and structural (`--lang` / `--query` / `--ast`)
  are now `#[command(flatten)]`-ed substructs. CLI surface
  byte-identical; only the in-code access path changes
  (`cli.output.json`, `cli.guard.max_files`, etc.).
- **`SiblingKind` enum drives recast-sibling filenames.** The
  `.{target}.recast.{token}.{nonce}` token (`bak` / `tmp`) is now
  emitted and parsed through one `SiblingKind::as_str` /
  `SiblingKind::from_token` pair so the two sides cannot drift.

### Added

- **`Error::InvalidThreads` + `Error::ThreadPool` variants** replace
  the synthetic `Error::Io { path: empty }` wrappers that
  `parallel::build_pool` used to emit for non-IO failures. JSON
  surfaces them as `invalid_threads` / `thread_pool` `error` kinds.
- **`ScriptRewriter::fresh()`** — sibling rewriter, new sandboxed
  Engine, shared AST. Designed for per-rayon-worker construction.
- **`template_scan` module** — shared `$NAME` / `${NAME}` / `$$$NAME`
  placeholder scanners. The regex convergence probe and the
  structural pattern preprocess + template parser now share one
  identifier grammar so the three byte walkers can't drift.
- **`plan::read_text_or_skip_binary`** — single helper that wraps the
  metadata + max-bytes check + `read_to_string` + UTF-8 skip. Used by
  both `plan_rewrite` and `plan_structural_rewrite`.
- **Multi-file structural criterion bench**
  (`bench_plan_structural_rewrite`) covering 10 / 100 / 500 file
  fixtures.

### Internal

- `commit::recover_sweep` reports `ignore::Error` via the existing
  `Error::Walk` variant instead of building a synthetic
  `Error::Io { path: empty }`.
- `commit` extracted `parent_dir` / `remove_nonced` helpers; rollback
  loops collapsed.
- Binary's `Language::from_name` / `dispatch(plan)` helpers consolidate
  the apply / check / diff trailer duplicated between `run` and
  `run_structural`.
- Binary `Cli` adds `min_matches()`, `paths_as_pathbufs()`,
  `recover_paths()`, and `acquire_workspace_lock_for(&cli)` helpers
  so the recurring `Some(at_least.unwrap_or(1))`, path conversions,
  the `--recover` clap-workaround fold, and the lock-root probe
  each live in one place.
- `CompiledStructural::apply` collects matches into a named
  `struct Hit { start, end, replacement }` instead of the positional
  `(usize, usize, String)` tuple it had before — readability only.

### Documentation

- `CHANGELOG.md` + `PLAN.md` §7.1 + `docs/src/json-schema.md`
  brought back in sync with the wire format (shared-header field
  order; full `ErrorKind` vocabulary including
  `invalid_threads` / `thread_pool`).
- `AGENTS.md` §11 records the harness-classifier workaround for
  `git commit`: split `git add` and `git commit` into separate
  shell calls; do not chain them with `&&` + heredoc.

## [0.1.6] — 2026-05-21

First successful crates.io release. v0.1.5's publish step failed
because the crates.io account hadn't verified its email yet; this
is the same content with the verified email now in place.

## [0.1.5] — 2026-05-21 (crates.io publish blocked on email verification)

Hotfix: v0.1.4's release matrix referenced the old `recast` package
name (renamed to `recast-cli` for the crates.io publish), so every
target leg failed to build and nothing shipped. The Cargo.toml
fix is the only code-level change; everything below is what 0.1.4
*was* meant to ship.

## [0.1.4] — 2026-05-21 (not shipped)

First crates.io release.

### Added

- Published to crates.io as `recast-core` (library) and `recast-cli`
  (binary). The bare `recast` name on crates.io is already taken by
  an unrelated serialization-format crate; the installed binary is
  still called `recast`. Install via `cargo install recast-cli`.
- Tag-pushed releases now publish both crates automatically via a
  new `publish-crates` job in `.github/workflows/release.yml`,
  gated on the `CARGO_REGISTRY_TOKEN` repository secret.
- Concurrent-apply lockfile (`.recast.lock` per workspace), `--force`
  escape hatch. Two `recast --apply` against the same tree no longer
  interleave; second invocation errors out cleanly.
- Friendlier tree-sitter query errors with line/column + caret;
  `--ast` parse failures name the grammar that choked.
- Criterion benchmark suite (`cargo bench --features lang-rust,script
  -p recast-core`) covering regex compile, plan over 10/100/500
  files, and a structural rewrite over a 200-fn source.
- mdBook documentation under `docs/`, deployed to
  https://stoica-mihai.github.io/recast/ on every push to `main`.

### Changed

- Workspace MSRV bumped from 1.85 to 1.89 to unlock
  `std::fs::File::sync_all` + `unlock` + `OpenOptions::truncate(false)`.
- Error chains print one cause per line instead of the previous
  `{err:#}` double-printed style.

## [0.1.3] — 2026-05-21

Big release bundling every prod-readiness item landed since v0.1.2:
structural-mode atomic apply, crash recovery (`--recover`), friendly
`$NAME` / `$$$ELLIPSIS` patterns, proptest harness, eight tree-sitter
grammars (Rust, TS, TSX, JS, Python, Bash, Go, JSON, Markdown), and a
release matrix that now ships aarch64-linux and musl static binaries
alongside the existing x86_64-linux / macOS / Windows targets.

### Added

- **Tree-sitter grammars** wired up for structural mode:
  - Tier 1: TypeScript / TSX / JavaScript (with JSX) / Python
  - Tier 2: Bash / Go
  - Tier 3: JSON / Markdown
  `Language` enum variants: `Rust | TypeScript | Tsx | JavaScript |
  Python | Bash | Go | Json | Markdown`. CLI names accept the obvious
  aliases (`ts`, `tsx`, `jsx`, `py`, `sh`, `golang`, `md`, …).
- **Per-language cargo features.** `structural` feature is gone;
  replaced by `lang-rust`, `lang-ts`, `lang-js`, `lang-python`,
  `lang-bash`, `lang-go`, `lang-json`, `lang-md`, and the
  convenience `lang-all`. At least one `lang-*` must be enabled
  for structural mode to compile.
- **Friendly `$NAME` / `$$$ELLIPSIS` patterns.** `--ast 'fn $NAME() {}'`
  compiles target-language source into a tree-sitter query, with
  `$NAME` for single-node capture and `$$$NAME` for variable-shape
  subtree capture.
- **Crash recovery sweep.** `recast --recover PATHS` scans for
  leftover `.recast.bak.*` / `.recast.tmp.*` siblings from a
  previously interrupted `--apply` and restores or cleans up.
- **Structural `--apply` is atomic** — now routed through the same
  two-phase commit + rollback used by regex/script modes.
- **Cross-compiled Linux release binaries.** The release matrix
  builds aarch64-unknown-linux-gnu plus x86_64 / aarch64
  unknown-linux-musl via cross-rs, so Alpine, distroless, and
  AWS Graviton consumers get pre-built artifacts. The `recast` binary defaults to
  `["script", "lang-all"]` so `cargo install --path crates/recast`
  still gets the full surface; users can opt out with
  `--no-default-features --features lang-rust` for a slim binary.
- **Proptest harness** covering compile / rewrite / template /
  friendly-pattern paths so adversarial input never panics.

### Breaking

- `--features structural` no longer exists — pick `lang-*` features.
- `recast-core` workspace dep now has `default-features = false` at
  the workspace level; downstream `recast` re-opts in.

## [0.1.2] — 2026-05-21

Windows build hotfix for the release workflow. The `rustix::fs::fsync`
call was Unix-only; the x86_64-pc-windows-msvc leg of the v0.1.1
release matrix failed to compile and the publish step was skipped, so
v0.1.1 shipped without binaries.

### Fixed

- Replace `rustix::fs::fsync` with `std::fs::File::sync_all()`
  (cross-platform). Parent-directory fsync is now `#[cfg(unix)]`-gated;
  Windows relies on per-file `sync_all` for durability.
- Drop the `rustix` workspace dependency entirely (no longer needed).

## [0.1.1] — 2026-05-21

Re-tag of 0.1.0 to ship pre-built binaries via the new release
workflow. No source-level feature changes vs 0.1.0.

### Fixed

- `cargo deny` no longer flags `recast-core` as a wildcard
  dependency (explicit `version = "=0.1.x"` pin on the workspace
  path dep).

### CI

- New `.github/workflows/release.yml` builds `recast` for
  `x86_64-unknown-linux-gnu`, `x86_64/aarch64-apple-darwin`, and
  `x86_64-pc-windows-msvc`, packages binary + README + LICENSE +
  CHANGELOG into a `.tar.gz` (or `.zip` on Windows) per target with
  a `.sha256` sidecar, and attaches everything to the matching
  GitHub Release.

## [0.1.0] — 2026-05-21

First tagged alpha. Charter from `PLAN.md` (phases 0–6) delivered;
shipped as an unpublished workspace (no crates.io release yet,
`publish = false`). Hardening tasks listed in §"Path to ready" remain
before a 1.0 ship: crash-time recovery sweep, structural `--apply`
through the 2-phase commit, cross-platform CI matrix, fuzz tests,
more tree-sitter grammars.

### Added

- **Phase 0 scaffold** — workspace layout (`crates/recast` +
  `crates/recast-core`), CI (`ci.yml`, `audit.yml`), `rustfmt`, `clippy`,
  `cargo-deny`, AGENTS.md / CLAUDE.md operating manual, MIT license.
- **Phase 1 MVP** — regex find/replace via the `regex` crate; unified
  diff preview via `similar`; `--apply` writes; match-count guard
  (`--at-least`, `--at-most`); convergence (idempotency) check; parallel
  per-file work via `rayon`.
- **Phase 2 atomicity** — two-phase commit: sibling temp + fsync, then
  per-file `original → backup` and `temp → original` rename pair. On any
  commit-phase failure the rename log walks back in reverse and restores
  every committed file from its backup; remaining staged temps are
  deleted. Test injects a mid-commit failure and verifies the tree is
  bit-identical to the pre-image.
- **Phase 3 filters** — `ignore`-crate integration with
  `-t/--type`, `-T/--type-not`, `-g/--glob`, `--no-ignore`, `--hidden`.
- **Phase 4 JSON schema** — `--json` emits one schema-locked line per
  invocation: `kind` ∈ `plan` | `apply` | `check` | `error`, plus
  machine-readable `error` kind and `exit_code` on the error variant.
  `insta` snapshots in `crates/recast-core/src/snapshots/` lock the wire
  format. Documented in `PLAN.md §7.1`.
- `--threads N` — explicit rayon worker count via a dedicated thread
  pool installed for the scan.
- `--completions <shell>` — bash, zsh, fish, elvish, powershell.
- `--stdin` — read input from stdin, rewrite once, write to stdout.
  Skips the walker and commit phases for one-shot pipelines.
- **Phase 5 scripted replacements** — feature-gated `script` flag
  pulls in `rhai`. `recast --script foo.rhai 'pattern' '' paths/`
  runs the script per regex match; the return value becomes the
  replacement. Script sees `captures` (array; index 0 is the full
  match) and `whole` (full-match alias, since `match` is a Rhai
  reserved keyword). Sandbox caps: 1M operations, 1 MiB strings,
  1024 array entries, expression depth 64. Available in `--stdin`
  too. Scripted scans run sequentially (rhai engine isn't `Sync`).
- **Phase 6 structural rewrites** — feature-gated `structural`
  flag pulls in `tree-sitter` + `tree-sitter-rust`. `recast
  --lang rust --query '<s-expr>' '' '<template>' paths/` parses
  each file with the named grammar, runs the tree-sitter Query
  against it, and substitutes the captures into the template.
  `$name` / `${name}` references the capture; `@root` (or the
  outermost capture when absent) defines the replace range.
  Supports `--stdin`, `--check`, `--apply`, default diff. Only
  Rust shipped initially; the `Language` enum is the extension
  point for more grammars.
- Tracing spans via `tracing` at `DEBUG` (phase markers) and `TRACE`
  (per-file events). `RUST_LOG=debug` surfaces them.
- Public-API rustdoc on every exported item; a `docs` CI job runs
  `cargo doc --no-deps -- -D warnings`.

### Fixed

- Diff path labels drop leading `./` so unified-diff headers read
  `a/src/a.rs` instead of `a/./src/a.rs` when the user passes `.` as
  the root.

### Tests

- 77 unit + integration tests at the time of writing. Snapshots cover
  JSON output (11 cases) and unified-diff output (4 cases). Integration
  tests under `crates/recast/tests/cli.rs` spawn the binary via
  `assert_cmd` and verify exit codes for every mode.

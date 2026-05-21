# Changelog

All notable changes to `recast` land here. Format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/); the project
follows [Semantic Versioning](https://semver.org/spec/v2.0.0.html) once a
1.0.0 release exists.

## [Unreleased]

### Added

- Published to crates.io as `recast-core` (library) and `recast-cli`
  (binary). The bare `recast` name on crates.io is already taken by
  an unrelated serialization-format crate; the installed binary is
  still called `recast`. Install via `cargo install recast-cli`.
- Tag-pushed releases now publish both crates automatically via a
  new `publish-crates` job in `.github/workflows/release.yml`,
  gated on the `CARGO_REGISTRY_TOKEN` repository secret.

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

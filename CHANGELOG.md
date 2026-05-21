# Changelog

All notable changes to `recast` land here. Format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/); the project
follows [Semantic Versioning](https://semver.org/spec/v2.0.0.html) once a
1.0.0 release exists.

## [Unreleased]

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

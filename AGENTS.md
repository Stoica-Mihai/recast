# AGENTS.md

Operating manual for AI agents and human contributors working in this repo.

## 1. Project

**recast** — a CLI for safe, atomic, transparent multi-file text rewrites. Pure Rust. Tuned for LLM coding agents driving mechanical edits; equally usable by humans.

**Not** a `sed` clone. **Not** a formatter. **Not** a typed-language refactor tool — though `--lang rust` + a tree-sitter query gets close. See `PLAN.md` for the full thesis and non-goals.

## 2. Status

Alpha (v0.1.13). Every phase of `PLAN.md` (0–6) has landed; pre-built binaries ship for Linux (x86_64/aarch64, gnu + musl) and macOS (x86_64/aarch64) via the `release.yml` workflow. Windows is not a supported target — `#[cfg(unix)]` is assumed throughout. Update `PLAN.md` and `CHANGELOG.md` in lockstep with any further feature work.

## 3. Thesis

Multi-file regex rewrites that defeat the two silent failure modes that bite LLM agents using `python` heredocs:

- **Silent no-match.** Default `--at-least 1` guard fails non-zero if zero matches.
- **Non-idempotent re-runs.** Built-in convergence check refuses non-convergent rewrites; reports "already applied" on second run.

Plus first-class atomicity (two-phase commit with rollback + a crash-recovery sweep), unified-diff preview by default, agent-friendly JSON output, and ergonomics close to `sd`. Beyond regex there are two opt-in modes: a Rhai script callback (`--script`) and tree-sitter structural matching (`--lang` + `--query`/`--ast`).

If a feature decision could be answered "`sd` already does this," reconsider scope.

## 4. Non-goals

- Not a code formatter (`rustfmt`, `prettier`).
- Not a linter (`clippy`, `eslint`).
- Not a full IDE-grade refactor tool — `rust-analyzer` / `ast-grep` are richer. Structural mode covers the common rename + reshape cases, not type-aware refactors.
- Not interactive. No TUI for picking matches one-by-one. Batch-by-default for agents and CI.
- Not a VCS. Atomicity is per-invocation, not cross-invocation.

## 5. Architecture

```
crates/
  recast/                         # binary: clap entry, top-level orchestration
    src/main.rs                    # CLI parse + dispatch
    src/completion.rs              # shell completion generator
    tests/cli.rs                   # assert_cmd integration tests
  recast-core/                    # library
    src/lib.rs
    src/walker.rs                  # ignore/glob/type-aware path enumeration
    src/pattern.rs                 # regex compile + convergence probe
    src/rewrite.rs                 # per-file rewrite + unified diff + label_for_path
    src/plan.rs                    # walk → compile → rewrite → guard pipeline
    src/commit.rs                  # 2-phase atomic commit + recovery sweep
    src/parallel.rs                # rayon thread-pool builder (--threads)
    src/json.rs                    # schema-locked JSON output (feature `serde`)
    src/script.rs                  # Rhai scripted replacements (feature `script`)
    src/structural.rs              # tree-sitter rewrites + friendly `$NAME` patterns
    src/proptests.rs               # property tests covering the public surface
```

Data flow per regex/script invocation:

1. `walker` enumerates candidate files honoring globs + ignore rules + `--type` / `-g` filters.
2. `pattern` compiles the regex once and stores the replacement template (literal or interpolated).
3. Per file (parallel via `rayon`):
   a. Read into memory (skip if size > `--max-bytes` or non-UTF8).
   b. `regex::replace_all` (or `rewrite_text_scripted` for `--script`); collect new text + match count.
   c. If new == old, mark "no change".
4. **Convergence check.** Re-apply pattern to the post-image. If any file would change again, abort — pattern is non-convergent.
5. **Match-count guard.** If total matches < `--at-least`, exit 2.
6. Mode dispatch: `--diff` (default) prints unified diffs + summary; `--apply` runs two-phase commit; `--check` exits 1 if any file would change.

Structural mode (`--lang` + `--query`/`--ast`) skips the regex pipeline; it parses each file with tree-sitter, runs a tree-sitter Query, substitutes captures into the rewrite template, and routes through the same `apply_changes` for the actual write.

Cross-crate types live in `recast-core`. The binary depends on the library, never the reverse.

## 6. Stack (locked)

- Rust 2024 edition.
- `clap` v4 with derive macros, plus `clap_complete` for shell completions.
- `regex` crate. Feature-gate `fancy-regex` only if lookaround proves necessary.
- `ignore` crate (powers ripgrep) for `.gitignore` semantics; `globset` for explicit `-g` glob arguments.
- `similar` for diff generation and unified hunks.
- `tempfile` for sibling temp files. fsync via `std::fs::File::sync_all()` (cross-platform).
- `rayon` for parallel per-file work, with a dedicated `ThreadPool` so `--threads N` is honored without touching the global pool.
- `rhai` (feature `script`) for the scripted-replacement callback — default features off, std/no_module/no_custom_syntax.
- `tree-sitter` + per-language grammars (`tree-sitter-rust`, `-typescript`, `-javascript`, `-python`, `-bash`, `-go`, `-json`, `-md`) gated by `lang-*` features. The umbrella `lang-all` enables all.
- `anyhow` in binaries only; `thiserror` for typed errors in `recast-core`. `#![forbid(unsafe_code)]` workspace-wide.
- `tracing` + `tracing-subscriber`. `RUST_LOG` controls level. `--json` switches to single-line JSON output suitable for agent parsing.
- `serde` + `serde_json` (feature `serde`) for the JSON output schema.
- Tests: separate `_tests.rs` files referenced via `#[cfg(test)] #[path = ...] mod tests;`. Integration tests under `crates/recast/tests/` using `assert_cmd` + `predicates`. Snapshot tests via `insta`. Property tests via `proptest`.

## 7. Workspace layout

```
crates/
  recast/
  recast-core/
```

Each crate keeps a tight surface. The binary depends on the library, never the reverse.

## 8. Build, test, run

```bash
cargo build --release --workspace --all-features
cargo test --workspace --all-features
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
cargo doc --workspace --no-deps --all-features          # RUSTDOCFLAGS=-D warnings in CI

cargo run -p recast -- --help
cargo run -p recast -- 'OldName' 'NewName' src/
cargo run -p recast -- --apply 'OldName' 'NewName' src/
cargo run -p recast -- --lang rust --ast 'fn $NAME() {}' --apply '' 'fn ${NAME}_v2() {}' src/
cargo run -p recast -- --script bump.rhai '(\d+)' '' src/version.txt
cargo run -p recast -- --recover src/
```

CI runs `fmt --check`, `clippy -D warnings`, `test --workspace --all-features` on Linux + macOS; `cargo doc` with `-D warnings`; and `cargo deny check bans licenses sources`. The release workflow cross-compiles six targets and uploads them to the matching GitHub Release with notes auto-extracted from the `CHANGELOG.md` section for that tag.

## 9. Coding standards

- `rustfmt` enforced. No exceptions.
- `clippy -D warnings`. Suppress with `#[allow(clippy::...)]` plus a one-line `// reason:` comment.
- `#![forbid(unsafe_code)]` workspace-wide via `Cargo.toml` lints table.
- No `unwrap()` / `expect()` in hot paths. Propagate `Result`. Tests may unwrap.
- Errors: `thiserror` enums per library crate; `anyhow::Result` only at binary boundaries.
- No global mutable state.
- No `lazy_static!` / `once_cell` for configuration. Configuration flows in via constructor or argument.
- Tests live in their own files, never in the implementation file. For unit tests of `src/foo.rs`, write `src/foo_tests.rs` and reference it from `foo.rs` via `#[cfg(test)] #[path = "foo_tests.rs"] mod tests;`. Integration tests live under `tests/`. Snapshot tests via `insta` for diff/JSON output. Property tests via `proptest` (see `crates/recast-core/src/proptests.rs`).

## 10. Comments and documentation

- Default: no comments. Identifiers carry the meaning.
- Add a comment only when the *why* is non-obvious: a hidden invariant, a workaround for a specific upstream bug, behavior that would surprise a future reader.
- Do not explain *what* the code does in a comment when the code itself shows it.
- Do not reference the current task, PR, or caller in a comment ("added for X", "used by Y") — those rot. Put that context in the PR description.
- Public API items get doc comments (`///`). Internal items get a comment only when the *why* rule triggers.

## 11. Agent workflow rules

- **LSP first.** For "find definition", "find references", "list symbols", "find implementations", use the language server before grep. Grep is fallback only after LSP returns no result on retry.
- **Evidence before assertion.** Cite `file:line` for any claim about the codebase. Don't guess. Verify what is on disk now.
- **No `Co-Authored-By` trailers.** Commits are by the human author only.
- **Conventional commit prefixes:** `feat:`, `fix:`, `refactor:`, `chore:`, `docs:`, `test:`, `perf:`, `ci:`. Subject line ≤ 72 chars. Body explains *why* when not obvious.
- **Commit per feature.** Each landed feature, bug fix, or scoped refactor is its own commit. A commit must compile, pass `fmt --check` / `clippy -D warnings` / `test --workspace`, and represent one logical unit.
- **One concern per PR.** Refactor + feature in one PR is a reject. Split.
- **Destructive git operations require explicit human approval.** `reset --hard`, `push --force`, `branch -D`.
- **No bypassing hooks.** Never `--no-verify` without explicit instruction.
- **No backwards-compat shims pre-1.0.** Break freely until a 1.0 release exists.
- **TDD by default.** Every feature, bug fix, or behavioral change goes through the `/tdd:tdd` skill: write the failing test first, watch it fail, then write the minimum implementation that makes it pass. No production code without a red test first. Scaffolding, doc tweaks, and pure refactors of already-tested code are exempt.
- **DRY enforced.** Every feature, bug fix, refactor, or test edit runs through the `/engineering-principles:dry-principle` skill. Before writing new code, search for an existing helper, type, or pattern that already solves the sub-problem; lift it into a shared spot if it's now used in two places. Mechanical copy-paste between modules is a reject.
- **Verify before claiming done.** Run the relevant tests. Re-read the diff. Report results, not intentions.
- **Commit immediately after each landed feature.** Once `fmt --check` / `clippy -D warnings` / `test --workspace` pass on a scoped change, create the commit without waiting for further approval. Do not batch multiple features into one approval gate. Pushing remains opt-in.
- **Split `git add` and `git commit` into separate shell calls.** Do not chain them in one Bash invocation (`git add … && git commit -m "$(cat <<EOF…EOF)"`). The compound shape — staging + heredoc body + pipe in one call — trips the harness's auto-classifier and the commit is denied; running each command in its own tool call clears it. Multi-line commit messages stay inline via `git commit -m "$(cat <<'EOF' … EOF\n)"`; no need for a temporary message file.
- **Keep `README.md`, `CHANGELOG.md`, `PLAN.md`, and `docs/` (mdBook) in sync with the code.** The README is the user-facing front door; the changelog is the release-notes source the release workflow reads; the plan tracks phase status; the mdBook under `docs/` is the hosted documentation site published to GitHub Pages by `.github/workflows/docs.yml`. Drift in any of them after a feature lands is a bug.

## 12. Release process

Tag pushes (`v*`) trigger `.github/workflows/release.yml`:

1. **build job** matrix — 6 targets, mixing native (linux gnu x86_64, macOS x86_64/aarch64) and `cross`-driven (linux gnu aarch64, linux musl x86_64/aarch64). Each leg builds with `--all-features`, packages binary + README + LICENSE + CHANGELOG into a `.tar.gz`, and emits a `.sha256` sidecar. Windows is not currently part of the release matrix.
2. **publish job** downloads every artifact, extracts the `## [<version>]` section from `CHANGELOG.md`, and uploads to the matching release via `gh release create --notes-file` (new) or `gh release edit --notes-file` (existing).

To cut a release:

```bash
# 1. update version in workspace Cargo.toml and Cargo.lock + recast-core path-dep pin
# 2. add a `## [X.Y.Z] — DATE` section to CHANGELOG.md
# 3. commit, tag, push
git commit -am "chore: bump to X.Y.Z"
git tag -a vX.Y.Z -m "vX.Y.Z — short tagline"
git push origin main
git push origin vX.Y.Z
```

The workflow does the rest.

## 13. Lineage and references

- **sd** (`https://github.com/chmln/sd`) — closest prior art. Read first. `recast` differs in atomicity, match-required guard, idempotency check, JSON output, scripted + structural modes.
- **ripgrep** (`https://github.com/BurntSushi/ripgrep`) — `ignore` and `regex` crate idioms; workspace and CI shape.
- **ast-grep** (`https://ast-grep.github.io/`) — closest prior art for structural mode. Our `--ast` patterns + `$NAME` / `$$$NAME` metavars borrow heavily.
- **comby** (`https://comby.dev/`) — alternative structural-match reference.
- **amux** (`~/Documents/git/amux`, sibling project of same author) — operating-manual style, CI workflow shape, cadence rules, security posture (`#![forbid(unsafe_code)]`, no `unwrap()`/`expect()` in hot paths). This file mirrors `amux/AGENTS.md`.

## 14. Open questions

- Lookaround in default regex flavor — stay on `regex` crate; feature-gate `fancy-regex` if agents reach for it often.
- Concurrent invocation safety (two `recast --apply` against the same tree) — current behavior is undefined. File locks? lease file in the workspace root? Pending design.
- crates.io: `recast-cli` + `recast-core` both shipped (the bare `recast` name was taken by an unrelated crate). Path-dep version pin in workspace `Cargo.toml` tracks the workspace version; bump in lockstep per release.
- YAML / TOML grammars — pending tree-sitter v0.25 ABI compatibility upstream.
- Structural pattern UX — surface friendlier errors when `--ast` parse fails (currently dumps tree-sitter Query error).

## 15. License

MIT. See `LICENSE`.

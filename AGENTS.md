# AGENTS.md

Operating manual for AI agents and human contributors working in this repo.

## 1. Project

**recast** — a CLI for safe, atomic, transparent multi-file text rewrites. Pure Rust. Tuned for LLM coding agents driving mechanical edits; equally usable by humans.

**Not** a `sed` clone. **Not** a formatter. **Not** a typed-language refactor tool. See `PLAN.md` for the full thesis and non-goals.

## 2. Status

Pre-alpha. `PLAN.md` is the project charter and supersedes any other doc until the first code lands. Update `PLAN.md` in lockstep with the implementation as features land.

## 3. Thesis

Multi-file regex rewrites that defeat the two silent failure modes that bite LLM agents using `python` heredocs:

- **Silent no-match.** Default `--at-least 1` guard fails non-zero if zero matches.
- **Non-idempotent re-runs.** Built-in convergence check refuses non-convergent rewrites; reports "already applied" on second run.

Plus first-class atomicity (two-phase commit across the match set), unified-diff preview by default, agent-friendly JSON output, and ergonomics close to `sd`.

If a feature decision could be answered "`sd` already does this," reconsider scope.

## 4. Non-goals

- Not a code formatter (`rustfmt`, `prettier`).
- Not a linter (`clippy`, `eslint`).
- Not a syntactic refactor tool for typed languages by default — `ast-grep`, `rust-analyzer`. Structural mode is a v2 bonus, not the main thrust.
- Not interactive. No TUI for picking matches one-by-one. Batch-by-default for agents and CI.
- Not a VCS. Atomicity is per-invocation, not cross-invocation.

## 5. Architecture (target)

```
crates/
  recast/             # binary: clap entry, top-level orchestration
  recast-core/        # library
    src/lib.rs
    src/walker.rs     # path enumeration + ignore filters
    src/pattern.rs    # regex compilation, literal mode, idempotency
    src/rewrite.rs    # per-file rewriting + diff generation
    src/commit.rs     # two-phase atomic commit
```

Data flow per invocation:

1. `walker` enumerates candidate files honoring globs + ignore rules.
2. `pattern` compiles the regex once.
3. Per file (parallel via `rayon`):
   a. Read into memory (skip if size > `--max-bytes`).
   b. `regex::replace_all`; collect new text + match count.
   c. If new == old, mark "no change".
4. **Idempotency check.** Re-apply pattern to the post-image. If any file would change again, abort — pattern is non-convergent.
5. **Match-count guard.** If total matches < `--at-least`, exit 2.
6. Mode dispatch: `--diff` (default) prints unified diffs + summary; `--apply` runs two-phase commit; `--check` exits 1 if any file would change.

Cross-crate types live in `recast-core`. The binary depends on the library, never the reverse.

## 6. Stack (locked)

- Rust 2024 edition.
- `clap` v4 with derive macros.
- `regex` crate. Feature-gate `fancy-regex` only if lookaround proves necessary.
- `ignore` crate (powers ripgrep) for `.gitignore` semantics.
- `globset` for explicit `-g` glob arguments.
- `similar` for diff generation and unified hunks.
- `tempfile` for sibling temp files.
- `rustix` (default-features-off, `fs` feature) for `fsync` on Linux. No `libc::` direct calls. `#![forbid(unsafe_code)]` workspace-wide.
- `rayon` for parallel per-file work.
- `anyhow` in binaries only; `thiserror` for typed errors in `recast-core`.
- `tracing` + `tracing-subscriber`. `RUST_LOG` controls level. `--json` switches to single-line JSON output suitable for agent parsing.
- `serde` + `serde_json` for the JSON output schema.
- Tests: colocated unit tests; integration tests under `tests/` building fixture trees via `tempfile`. Snapshot diffs via `insta`.

## 7. Workspace layout

```
crates/
  recast/
  recast-core/
```

Each crate keeps a tight surface. The binary depends on the library, never the reverse.

## 8. Build, test, run

```bash
cargo build --release --workspace
cargo test --workspace --all-features
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check

cargo run -p recast -- --help
cargo run -p recast -- 'OldName' 'NewName' src/
cargo run -p recast -- --apply 'OldName' 'NewName' src/
```

CI must run `fmt --check`, `clippy -D warnings`, and `test --workspace --all-features` on every PR.

## 9. Coding standards

- `rustfmt` enforced. No exceptions.
- `clippy -D warnings`. Suppress with `#[allow(clippy::...)]` plus a one-line `// reason:` comment.
- `#![forbid(unsafe_code)]` workspace-wide via `Cargo.toml` lints table.
- No `unwrap()` / `expect()` in hot paths. Propagate `Result`. Tests may unwrap.
- Errors: `thiserror` enums per library crate; `anyhow::Result` only at binary boundaries.
- No global mutable state.
- No `lazy_static!` / `once_cell` for configuration. Configuration flows in via constructor or argument.
- Tests: colocated unit tests (`#[cfg(test)] mod tests`), integration tests under `tests/`. Snapshot tests via `insta` for diff output.

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
- **Conventional commit prefixes:** `feat:`, `fix:`, `refactor:`, `chore:`, `docs:`, `test:`, `perf:`. Subject line ≤ 72 chars. Body explains *why* when not obvious.
- **Commit per feature.** Each landed feature, bug fix, or scoped refactor is its own commit. A commit must compile, pass `fmt --check` / `clippy -D warnings` / `test --workspace`, and represent one logical unit.
- **One concern per PR.** Refactor + feature in one PR is a reject. Split.
- **Destructive git operations require explicit human approval.** `reset --hard`, `push --force`, `branch -D`.
- **No bypassing hooks.** Never `--no-verify` without explicit instruction.
- **No backwards-compat shims pre-1.0.** Break freely until a 1.0 release exists.
- **Verify before claiming done.** Run the relevant tests. Re-read the diff. Report results, not intentions.
- **Commit immediately after each landed feature.** Once `fmt --check` / `clippy -D warnings` / `test --workspace` pass on a scoped change, create the commit without waiting for further approval. Do not batch multiple features into one approval gate. Pushing remains opt-in.
- **Update `PLAN.md` when deviating.** The plan is living. Drift without a doc update is a bug.

## 12. Lineage and references

- **sd** (`https://github.com/chmln/sd`) — closest prior art. Read first. `recast` differs in atomicity, match-required guard, idempotency check, JSON output.
- **ripgrep** (`https://github.com/BurntSushi/ripgrep`) — `ignore` and `regex` crate idioms; workspace and CI shape.
- **comby** (`https://comby.dev/`) — structural-match reference for v2.
- **ast-grep** (`https://ast-grep.github.io/`) — same.
- **amux** (`~/Documents/git/amux`, sibling project of same author) — operating-manual style, CI workflow shape, cadence rules, security posture (`#![forbid(unsafe_code)]`, no `unwrap()`/`expect()` in hot paths). This file mirrors `amux/AGENTS.md`.

## 13. Open questions

- Lookaround in default regex flavor — stay on `regex` crate; feature-gate `fancy-regex` if agents reach for it often.
- Script DSL choice — `rhai` vs purpose-built mini-language. Defer until v2.
- Concurrency model — `rayon` parallel iteration over files is the default. Revisit when a watch mode lands.
- Binary distribution — `cargo install` plus pre-built binaries from GitHub Releases. Homebrew tap once stable.

## 14. License

MIT. See `LICENSE`.

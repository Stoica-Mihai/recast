# recast

**Status:** Pre-implementation. This document is the only source of truth
until the first code lands. Future contributors (human or LLM agent) should
treat this as the project charter and update it in lockstep with the
implementation.

## 1. What `recast` is

`recast` is a CLI for **safe, atomic, transparent multi-file text rewrites**.
It takes a pattern, a replacement, and a set of paths; it produces either
a diff preview or an atomic apply across every file. It is tuned for the
working environment where LLM coding agents drive most of the edits, but
it is equally usable by humans for mechanical refactors.

Think of it as: `sed`'s ergonomics for one-shot single-file edits, plus
`sd`'s readable syntax, plus `ripgrep`'s preview, plus features designed
specifically to defeat the two silent failure modes that bite agents:
**no-match misfires** and **non-idempotent re-runs**.

## 2. Why it should exist

Today, LLM coding agents very frequently fall back to Python heredocs for
mechanical edits across multiple files. Example session-level shape:

```sh
python3 <<'PY'
import re, pathlib
for p in pathlib.Path("crates").rglob("*.rs"):
    text = p.read_text()
    text = re.sub(r"OldName\b", "NewName", text)
    p.write_text(text)
PY
```

This is the right level of expressiveness, but it carries five concrete
problems:

1. **No diff preview.** Whatever was written is the new state. Bugs land
   silently.
2. **Silent no-match.** If the pattern misses, the heredoc still exits
   `0`. The agent assumes the edit worked and moves on.
3. **Non-atomic.** If the script aborts mid-loop (panic, OOM, signal),
   half the tree is rewritten and half is not.
4. **Boilerplate cost.** ~6 lines of code, ~50 tokens, for every batch
   edit. Multiply by hundreds of agent calls per session.
5. **Not idempotent-aware.** Re-running the same heredoc after a successful
   apply produces no error, but also no signal that there was nothing to do.

Existing tools each cover *part* of the gap:

| Tool      | Multi-file | Diff preview | Multi-line regex | Atomic apply | Match-required guard | Conditional / script |
|-----------|-----------:|-------------:|-----------------:|-------------:|---------------------:|---------------------:|
| `sed`     | manual loop | no          | clumsy (`-z`)    | no           | no                   | limited              |
| `sd`      | yes        | preview only | yes              | per-file     | no                   | no                   |
| `ripgrep --replace` | yes | yes  | yes              | n/a (read-only by default) | no | no |
| `comby`   | yes        | preview      | structural       | yes          | no                   | yes (templates)      |
| `ast-grep`| yes        | preview      | AST              | yes          | no                   | yes (rules)          |
| python heredoc | yes   | no           | yes              | no           | no                   | yes (full lang)      |

`recast` aims to be the row where every column is **yes** *and* the
ergonomic surface stays close to `sd`'s. The killer differentiators are
the last two columns plus first-class atomicity.

## 3. Core features (v1)

1. **Regex find/replace** across globbed paths.
   - PCRE-flavored (via Rust `regex` + optional `fancy-regex` for
     lookaround if needed).
   - Multi-line by default (`(?s)` implicit) — surprising-by-default for
     experienced sed users but matches what LLMs typically expect.
   - Named captures supported.
2. **Diff preview** before any write.
   - Unified diff per file, plus a final summary (`N files changed, M
     matches`).
   - `--diff` is the default when no `--apply` is given.
3. **Atomic apply** across the entire match set.
   - Two-phase commit: each file is written to a sibling temp file, all
     temps are fsynced, then renamed. Any per-file failure rolls back all
     pending temps before exit.
   - `--apply` is the explicit flag that turns previews into writes.
4. **Match-required guard** — the headline LLM-safety feature.
   - `--require-matches N` (or `--at-least N`) fails with non-zero exit
     and a clear error if fewer than `N` matches are found.
   - Default: `--at-least 1`. Silent zero-match is impossible by default;
     an agent must explicitly pass `--at-least 0` to allow no-op runs.
5. **Idempotency check.**
   - Before applying, `recast` checks whether running the rewrite again
     would change anything. If the post-image already matches the
     pre-image *because the pattern would not match the replacement*, the
     run is treated as already-applied; `recast` reports
     "already applied" and exits 0 without writing.
   - This catches the "re-run produces silent duplicate edits" failure
     mode common to LLM agents that retry on partial output.
6. **Globbing + ignore rules.**
   - Path arguments are globbed via `globwalk` or `ignore` crate.
   - `.gitignore` / `.ignore` / `.rgignore` respected by default;
     `--no-ignore` to override.
   - Symlinks not followed by default.
7. **Capture-aware replacement.**
   - `$1`, `$2`, `${name}` syntax (Rust regex style).
   - `--literal` flag to disable interpolation when the replacement
     contains `$`.

## 4. v2 features (deferred but designed for)

8. **Script mode** for conditional / computed replacements.
   - `--script <file>` accepting a small expression DSL or a Lua/Rhai
     script. Each match invokes the script with the captures and the
     surrounding context; the return value is the replacement.
   - Goal: cover the 5% of cases where regex-only is insufficient,
     without needing python heredocs.
9. **Structural mode** via `tree-sitter` integration.
   - `--lang rust --match 'fn $name($args) { $body }'` for syntactic
     patterns. Comby/ast-grep territory.
10. **Watch mode** — re-apply on file change.
11. **Multi-pattern playbooks** — a TOML/JSON file describing a sequence
    of rewrites; useful for codemods and migrations.

## 5. Non-goals

- Not a general code formatter. (`rustfmt`, `prettier` handle that.)
- Not a linter. (`clippy`, `eslint` exist.)
- Not a syntactic refactor tool for typed languages by default — that's
  what `ast-grep` and `rust-analyzer` are for. Structural mode is a v2
  bonus, not the main thrust.
- Not interactive (no TUI for picking matches one-by-one). The audience
  is agents and CI — batch-by-default.
- Not a database / VCS. Atomicity is per-invocation, not per-repo
  transaction across time.

## 6. Technology stack

- **Language:** Rust 2024 edition.
- **CLI parsing:** `clap` v4 with derive macros.
- **Regex:** `regex` crate. If lookaround turns out to be needed for
  agent ergonomics, gate `fancy-regex` behind a feature flag.
- **Globbing / ignore rules:** `ignore` crate (powers ripgrep), with
  `globset` for explicit glob arguments.
- **Diff rendering:** `similar` crate for the diff engine and unified
  hunks.
- **File I/O atomicity:** `tempfile` for sibling temp files; `nix` or
  `rustix` (default-features-off, `fs` feature) for `fsync` on Linux.
  No `libc::` direct calls; keep `#![forbid(unsafe_code)]`.
- **Error handling:** `anyhow` at the binary boundary; `thiserror` for
  the internal library crate.
- **Logging:** `tracing` + `tracing-subscriber`. `RUST_LOG` controls
  level. CI mode uses single-line JSON if `--json` is passed (helps
  agents parse output).
- **Tests:** colocated unit tests; integration tests under `tests/`
  using `tempfile` to build fixture trees. Snapshot diffs via `insta`.
- **CI:** GitHub Actions — `rustfmt --check`, `clippy -D warnings`,
  `cargo test --all-features`. Mirror the structure used in the
  reference workspace `~/Documents/git/amux`.

Why Rust: fast startup (agents invoke the binary thousands of times per
session), single static binary, mature regex + tree-sitter ecosystem,
sits naturally alongside `ripgrep`, `sd`, `fd` in the same toolbelt.

## 7. CLI surface (v1)

```
recast [OPTIONS] <pattern> <replacement> [paths]...

  pattern        Regex pattern. Multi-line by default. Use --literal for
                 plain-string matching.
  replacement    Replacement template. $1, $2, ${name} interpolated unless
                 --literal is set.
  paths          Paths or globs to scan. Defaults to current directory if
                 omitted. .gitignore respected by default.

Modes:
  --diff         Show unified diff per file (default when --apply absent).
  --apply        Atomically write the changes.
  --check        Exit non-zero if any file would change. No output, no
                 writes. Useful in CI.

Safety knobs:
  --at-least <N>           Require at least N matches across all files
                           (default 1). 0 disables the guard.
  --at-most <N>            Require at most N matches (default unbounded).
  --allow-idempotent       Skip the idempotency check.
  --max-bytes <N>          Refuse files larger than N bytes (default 10MiB).
  --max-files <N>          Refuse runs touching more than N files (default
                           1000).

Filtering:
  -t, --type <lang>        Only files of this type (Rust, JS, etc.; mirrors
                           ripgrep's --type).
  --hidden                 Include hidden files.
  --no-ignore              Disable .gitignore filtering.
  -g, --glob <glob>        Add an include/exclude glob (-g '!**/vendor/**').

Output:
  --json                   Emit machine-readable summary on stdout.
  --quiet                  Suppress diff body; print only the summary.
  -v, --verbose            Per-file timing and counters.

Misc:
  -L, --literal            Treat pattern and replacement as literal strings.
  -i, --ignore-case        Case-insensitive matching.
  -s, --single-line        Disable implicit (?s) — make `.` not match \n.
  --threads <N>            Worker threads (default = num CPUs).
```

Exit codes:

- `0` — success (apply mode) or "no changes needed" (check mode).
- `1` — at least one file would change but `--check` was set.
- `2` — match-count guard violated (no matches when `--at-least 1`, etc).
- `3` — internal error (parse, I/O, atomic rollback).

### 7.1 JSON output schema

`--json` emits exactly one line of compact JSON on stdout per invocation
(errors go to stdout too, not stderr, so an agent has a single stream to
parse). Snapshot-locked in `crates/recast-core/src/snapshots/` — changing
field names or order is a breaking change.

Every report carries a `kind` discriminator: `plan` | `apply` | `check` |
`error`. Non-error reports share `outcome` (`"changes"` or
`"already_applied"`), `files_scanned`, and `total_matches`. Each mode adds
the count it owns.

```jsonc
// plan (default mode)
{
  "kind": "plan",
  "outcome": "changes" | "already_applied",
  "files_scanned": 5,
  "files_changed": 2,
  "total_matches": 3,
  "changes": [
    { "path": "src/a.rs", "matches": 2 },
    { "path": "src/b.rs", "matches": 1 }
  ]
}

// apply
{
  "kind": "apply",
  "outcome": "changes" | "already_applied",
  "files_scanned": 5,
  "files_written": 2,
  "total_matches": 3
}

// check
{
  "kind": "check",
  "outcome": "changes" | "already_applied",
  "files_scanned": 5,
  "files_would_change": 2,
  "total_matches": 3
}

// error
{
  "kind": "error",
  "error": "too_few_matches"
         | "too_many_matches"
         | "non_convergent"
         | "too_many_files"
         | "file_too_large"
         | "invalid_regex"
         | "invalid_glob"
         | "walk"
         | "io",
  "message": "human-readable description",
  "exit_code": 2 | 3
}
```

`error` carries the process exit code so the agent can act without
re-reading `$?`. `too_few_matches` and `too_many_matches` map to exit 2;
the rest map to exit 3.

## 8. Architecture sketch

```
crates/
  recast/         # binary
    src/main.rs   # clap entry, top-level orchestration
  recast-core/    # library, depends on regex / ignore / similar
    src/lib.rs
    src/walker.rs   # path enumeration + ignore filters
    src/pattern.rs  # regex compilation, literal mode, idempotency
    src/rewrite.rs  # per-file rewriting + diff generation
    src/commit.rs   # two-phase atomic commit
```

Data flow per invocation:

1. `walker` enumerates candidate files honoring globs + ignore rules.
2. `pattern` compiles the regex once; caches per-thread captures.
3. For each file (parallel via `rayon` or `tokio::task::spawn_blocking`):
   a. Read into memory (skip if size > `--max-bytes`).
   b. Run `regex::replace_all`; collect the new text + the match count.
   c. If the new text equals the old, mark as "no change".
4. After all files processed, run the **idempotency check**: would
   re-applying the same pattern to the new text produce another change?
   If yes for any file, abort with a clear error — the pattern is not
   convergent.
5. Run the **match-count guard**: if total matches < `--at-least`,
   abort with exit 2.
6. If `--diff` or default: print unified diffs, summary, exit 0.
   If `--apply`: write all temp files, fsync, rename, exit 0.
   If `--check`: exit 0 if no file would change, exit 1 otherwise.

## 9. Implementation phases

**Phase 0 — scaffold.** Workspace layout, CI workflow, `.github/workflows/ci.yml`
mirroring amux. License + README pointing at this PLAN.md.

**Phase 1 — minimal MVP.** Regex find/replace across paths, unified diff
preview, no atomic apply yet (just `--apply` writing each file in-place,
no rollback). Match-count guard. Idempotency check.

**Phase 2 — atomicity.** Two-phase commit. Crash-safe writes. Tests that
inject a panic mid-loop and verify the tree is unchanged.

**Phase 3 — ignore rules + globs.** `ignore` crate integration; `--type`,
`-g` flags.

**Phase 4 — JSON output, exit-code spec lockdown.** Agent-friendly mode.

**Phase 5 — script mode (v2).** Embed `rhai` or a small `expr` DSL for
conditional replacements.

**Phase 6 — structural mode (v2).** Tree-sitter for syntactic patterns
behind `--lang`. Optional dependency, feature-flagged.

Each phase ships as one or more commits with the **commit-per-feature**
cadence: every commit must pass `cargo fmt --check`, `cargo clippy -D
warnings`, `cargo test --all-features`. Conventional commit prefixes
(`feat:`, `fix:`, `refactor:`, `chore:`, `docs:`, `test:`, `perf:`).
Subject ≤ 72 chars. Body explains *why*, not *what*.

## 10. Open questions

- **Lookaround in default regex flavor?** Pure `regex` crate is fast but
  no lookaround. `fancy-regex` adds them at a perf cost. Decide based on
  whether agents reach for lookaround often. Default: stay on `regex`,
  feature-gate `fancy-regex`.
- **Script DSL choice.** `rhai` (Rust-native, embeddable, strict types)
  vs. tiny purpose-built mini-language. Defer until v2.
- **Idempotency definition.** A pattern is "convergent" when applying it
  to the post-image produces no further change. This is the operational
  test. Document edge cases (patterns that intentionally chain through
  multiple passes — `recast` will refuse them and the user must split
  the rewrite into two invocations).
- **Concurrency model.** `rayon` parallel iteration over files is the
  obvious default. Reconsider if a future watch mode wants async.
- **Binary distribution.** Plain `cargo install`, plus pre-built
  binaries for x86_64-linux/macos/windows-arm64 from GitHub Releases.
  Optional Homebrew tap once stable.

## 11. Naming context

`recast` was chosen out of `recast`, `morph`, `cinch`, `salvo`, `relit`.
Rationale: verb form, conveys structured transform, no PATH collisions
in common dev tools, googleable, reads naturally in CI logs
(`recast --apply 'OldName' 'NewName' src/**/*.rs`).

Naming-rejection summary recorded so future contributors don't relitigate:

- `morph`: memorable but slightly cute; some collision risk with FPGA /
  graphics tooling.
- `cinch`: leans on "atomic apply" metaphor; loses the find/replace
  meaning at a glance.
- `salvo`: batch-burst metaphor; same readability concern as `cinch`.
- `relit`: clashes with linker tooling and game-tool namespaces.
- `sed2`, `safesed`, `llmsed`: derivative; niche-trapping.

## 12. Lineage / reference projects

- **sd** (`https://github.com/chmln/sd`) — closest prior art. Read it
  first. `recast` differs in: atomicity, match-required guard,
  idempotency check, agent-output (JSON) mode.
- **ripgrep** (`https://github.com/BurntSushi/ripgrep`) — uses the
  `ignore` and `regex` crates; structure of the workspace and CI is a
  good template.
- **comby** (`https://comby.dev/`) — structural matching reference for
  v2.
- **ast-grep** (`https://ast-grep.github.io/`) — same.
- **amux** (`~/Documents/git/amux`, sibling project of same author) —
  reference for AGENTS.md operating-manual style, CI workflow shape,
  cadence rules, security posture (`#![forbid(unsafe_code)]`, no
  `unwrap()`/`expect()` in hot paths, etc.). Mirror those conventions
  here.

## 13. What an LLM agent should do first

1. Read this document in full.
2. Skim the `sd` source for the regex + walker patterns worth borrowing.
3. Create AGENTS.md mirroring amux's structure: project thesis, target
   architecture, locked stack, coding standards, agent workflow rules,
   commit cadence rule. CLAUDE.md → AGENTS.md symlink.
4. Write the workspace scaffold per Phase 0.
5. Land Phase 1 in one commit. Verify with `cargo fmt --check`,
   `cargo clippy --all-targets --all-features -- -D warnings`,
   `cargo test --workspace --all-features`.
6. Update this PLAN.md to reflect any deviations from the plan as they
   land.

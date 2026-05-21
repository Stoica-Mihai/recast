# recast

`recast` is a CLI for **safe, atomic, transparent multi-file text rewrites**.
Pure Rust. Tuned for LLM coding agents driving mechanical edits; equally
usable by humans for mechanical refactors.

## Why it exists

LLM agents and shell scripts that rewrite code typically fall back to
`sed`, `sd`, or a Python heredoc. All three share two silent failure
modes:

1. **Silent no-match.** The pattern misses, the tool exits `0`, the
   agent moves on assuming success.
2. **Non-idempotent re-runs.** Re-running a rewrite on already-rewritten
   text either no-ops (looks like the first run failed) or, worse,
   compounds the rewrite.

`recast` makes both impossible by default. A missing match is a non-zero
exit; a non-convergent rewrite is rejected outright.

## What it does

| Capability             | `sed` / `sd` | Python heredoc | `recast` |
|------------------------|:---:|:---:|:---:|
| Multi-file rewrite     | manual / yes | yes | yes |
| Match-required guard   | no  | no  | **yes** |
| Idempotency check      | no  | no  | **yes** |
| Atomic two-phase apply | no  | no  | **yes** |
| Diff preview by default| no  | no  | **yes** |
| Crash-recovery sweep   | no  | no  | **yes** |
| Agent-friendly JSON    | no  | no  | **yes** |
| Regex pattern          | yes | yes | yes |
| Script pattern (Rhai)  | no  | yes (Python) | **yes** |
| Structural (AST)       | no  | no  | **yes** (tree-sitter) |

## Status

Alpha. Tracked in [`PLAN.md`](https://github.com/Stoica-Mihai/recast/blob/main/PLAN.md);
release notes in [`CHANGELOG.md`](https://github.com/Stoica-Mihai/recast/blob/main/CHANGELOG.md).

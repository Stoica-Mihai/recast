# recast

CLI for safe, atomic, transparent multi-file text rewrites. Pure Rust.
Tuned for LLM coding agents driving mechanical edits; equally usable by
humans for mechanical refactors.

`recast` differs from `sed` / `sd` / a Python heredoc in four places:

1. **Match-required guard.** Default `--at-least 1` exits non-zero when
   nothing matches. Silent no-match is impossible by default.
2. **Idempotency check.** Refuses non-convergent rewrites; reports
   "already applied" on the second run.
3. **Atomic apply.** Two-phase commit: sibling temp + fsync + rename;
   rollback restores the tree if any rename mid-batch fails.
4. **Agent-friendly JSON.** `--json` emits a single-line, schema-locked
   report for plan / apply / check / error.

Status: pre-alpha. See [`PLAN.md`](./PLAN.md) for the charter and
[`AGENTS.md`](./AGENTS.md) for the operating manual.

## Install

```bash
cargo install --path crates/recast
```

Pre-built binaries and a Homebrew tap will follow a 1.0 release.

## Usage

```bash
recast [OPTIONS] <PATTERN> <REPLACEMENT> [PATHS]...
```

### Diff preview (default)

```bash
recast 'OldName' 'NewName' src/
```

Prints a unified diff per file plus a summary line. No writes.

### Atomic apply

```bash
recast --apply 'OldName' 'NewName' src/
```

Writes the changes through a two-phase commit. If any rename fails
mid-batch, the rollback restores the original tree bit-identical.

### CI gate

```bash
recast --check 'TODO' 'FIXME' .
# exit 0: nothing would change
# exit 1: at least one file would change
```

### Capture groups

```bash
recast 'fn (\w+)_old\b' 'fn ${1}_new' src/
```

`$1`, `${name}` interpolated; use `--literal` to disable interpolation.

### Filters

```bash
recast -t rust 'Old' 'New' .                # only Rust files
recast -T markdown 'Old' 'New' .            # everything except Markdown
recast -g '!vendor/**' 'Old' 'New' .        # exclude vendor dir
recast --no-ignore 'Old' 'New' .            # ignore .gitignore
```

### Agent-friendly JSON

```bash
recast --json --apply 'Old' 'New' src/
# {"kind":"apply","outcome":"changes","files_scanned":12,"files_written":3,"total_matches":7}
```

Schema documented in [`PLAN.md §7.1`](./PLAN.md#71-json-output-schema).

## Exit codes

| Code | Meaning |
|-----:|---------|
| `0`  | Success or "no changes needed" |
| `1`  | `--check` set and at least one file would change |
| `2`  | Match-count guard violated (`--at-least` / `--at-most`) |
| `3`  | Internal error (regex parse, I/O, non-convergent pattern) |

## Build from source

```bash
cargo build --release --workspace
cargo test --workspace --all-features
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
```

## License

MIT. See [`LICENSE`](./LICENSE).

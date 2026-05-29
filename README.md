# recast

[![crates.io](https://img.shields.io/crates/v/recast-cli.svg?label=recast-cli)](https://crates.io/crates/recast-cli)
[![recast-core](https://img.shields.io/crates/v/recast-core.svg?label=recast-core)](https://crates.io/crates/recast-core)
[![docs.rs](https://img.shields.io/docsrs/recast-core)](https://docs.rs/recast-core)
[![CI](https://github.com/Stoica-Mihai/recast/actions/workflows/ci.yml/badge.svg)](https://github.com/Stoica-Mihai/recast/actions/workflows/ci.yml)
[![license](https://img.shields.io/crates/l/recast-cli.svg)](./LICENSE)

CLI for safe, atomic, transparent multi-file text rewrites. Pure Rust.
Tuned for LLM coding agents driving mechanical edits; equally usable by
humans for mechanical refactors.

`recast` differs from `sed` / `sd` / a Python heredoc in six places:

1. **Match-required guard.** Default `--at-least 1` exits non-zero when
   nothing matches. Silent no-match is impossible by default.
2. **Idempotency check.** Refuses non-convergent rewrites; reports
   "already applied" on the second run.
3. **Syntax-regression guard.** For files with a tree-sitter grammar,
   refuses a rewrite whose output introduces new parse errors (a greedy
   regex stranding a brace). Syntactic only; override with
   `--allow-syntax-errors`.
4. **Atomic apply.** Two-phase commit (sibling temp + fsync + rename)
   with rollback if any per-file step fails. Crash-recovery sweep
   reconciles leftover `.recast.bak` / `.recast.tmp` siblings.
5. **Agent-friendly JSON.** `--json` emits a single-line,
   schema-locked report for plan / apply / check / error.
6. **Three pattern modes.** Regex (default), Rhai script callback
   (`--script`), or tree-sitter structural (`--lang` + `--query` /
   `--ast`).

Status: alpha (v0.1.6). All phases of [`PLAN.md`](./PLAN.md) landed.

- 📦 Install: `cargo install recast-cli` ([crates.io/recast-cli](https://crates.io/crates/recast-cli))
- 📚 Library: [crates.io/recast-core](https://crates.io/crates/recast-core) · [docs.rs/recast-core](https://docs.rs/recast-core)
- 📥 Pre-built binaries: [GitHub Releases](https://github.com/Stoica-Mihai/recast/releases)
- 📖 Hosted docs: <https://stoica-mihai.github.io/recast/>
- 🛠 Operating manual: [`AGENTS.md`](./AGENTS.md)

## Install

### Pre-built binary

Download the matching artifact for your platform from
[Releases](https://github.com/Stoica-Mihai/recast/releases/latest):

| OS                | Targets                                                  |
|-------------------|----------------------------------------------------------|
| Linux x86_64      | `x86_64-unknown-linux-gnu`, `x86_64-unknown-linux-musl`  |
| Linux aarch64     | `aarch64-unknown-linux-gnu`, `aarch64-unknown-linux-musl`|
| macOS x86_64      | `x86_64-apple-darwin`                                    |
| macOS Apple Silicon | `aarch64-apple-darwin`                                 |

The `musl` builds are statically linked — drop into an Alpine container
or distroless image without a glibc dependency.

### Cargo install (from crates.io)

```bash
cargo install recast-cli              # full feature set
cargo install recast-cli --no-default-features --features lang-rust  # slim
```

The crate on crates.io is named `recast-cli` (the bare `recast` name was
already taken by an unrelated serialization-format library). The
installed binary is still called `recast` — that's the command name
everything in this README uses.

### Cargo install (from source)

```bash
git clone https://github.com/Stoica-Mihai/recast
cd recast
cargo install --path crates/recast            # full feature set
```

Stock install ships every grammar, the Rhai script engine, and JSON
output. Opt out via `--no-default-features` and pick only the features
you want (see [§ Cargo features](#cargo-features)).

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

Two-phase commit: each file is written to a sibling temp + fsync, then
renamed into place. A failure mid-rename triggers reverse-rename of every
already-renamed file from its backup, leaving the tree bit-identical to
the pre-image.

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

`$1`, `${name}` interpolated; use `--literal` (`-L`) to disable
interpolation.

### Filters

```bash
recast -t rust 'Old' 'New' .                # only Rust files
recast -T markdown 'Old' 'New' .            # everything except Markdown
recast -g '!vendor/**' 'Old' 'New' .        # exclude vendor dir
recast --no-ignore 'Old' 'New' .            # ignore .gitignore
recast --hidden 'Old' 'New' .               # include dot-files
```

### Stdin mode

```bash
echo 'fn old_name() {}' | recast --stdin 'old_name' 'new_name'
# fn new_name() {}
```

Reads one buffer, rewrites once, writes to stdout. Skips the walker and
commit phases. The match-count guard still applies.

### Scripted replacement (Rhai callback)

`--script` takes a path to a Rhai script that runs per match. The
script's return value (coerced to string) becomes the replacement. The
positional `REPLACEMENT` is still required but ignored — pass `""`.

```bash
cat > bump.rhai <<'RHAI'
(parse_int(captures[1]) + 1).to_string()
RHAI

echo "version 3" | recast --stdin --script bump.rhai '(\d+)' ''
# version 4
```

The script sees `captures` (array; index 0 is the full match) and
`whole` (full-match alias — `match` is a Rhai reserved keyword).

### Structural rewrite (tree-sitter)

Two modes. Both require `--lang <LANG>`.

**Friendly `--ast`** — write the pattern in the target language with
`$NAME` (single-node) and `$$$NAME` (variable-shape subtree) placeholders:

```bash
recast --lang rust --apply \
  --ast 'fn $NAME($$$ARGS) { $$$BODY }' \
  '' \
  'fn ${NAME}_v2$ARGS $BODY' \
  src/
```

Matches any function regardless of param count or body shape; rewrites
the name and keeps the original args/body verbatim.

**Raw `--query`** — pass a tree-sitter S-expression query directly:

```bash
recast --lang rust --apply \
  --query '((identifier) @id (#eq? @id "old_name"))' \
  '' 'new_name' src/
```

Capture named `@root` (or, absent that, the outermost capture in each
match) defines the byte range to replace. The template uses
`$capture_name` / `${capture_name}` for substitution.

#### Supported languages

| Language    | CLI name                       | Feature       |
|-------------|--------------------------------|---------------|
| Rust        | `rust`, `rs`                   | `lang-rust`   |
| TypeScript  | `typescript`, `ts`             | `lang-ts`     |
| TSX         | `tsx`                          | `lang-ts`     |
| JavaScript  | `javascript`, `js`, `jsx`      | `lang-js`     |
| Python      | `python`, `py`                 | `lang-python` |
| Bash        | `bash`, `sh`, `shell`          | `lang-bash`   |
| Go          | `go`, `golang`                 | `lang-go`     |
| JSON        | `json`                         | `lang-json`   |
| Markdown    | `markdown`, `md`               | `lang-md`     |

### Crash recovery

If a `--apply` crashes mid-commit (panic, signal, power loss), the tree
may have leftover `.foo.recast.bak.N` / `.foo.recast.tmp.N` siblings.
Reconcile with:

```bash
recast --recover src/
```

Restores from backup when the target is missing; deletes stale temps and
backups when the target is present.

### Shell completions

```bash
recast --completions bash > /etc/bash_completion.d/recast
recast --completions zsh  > ~/.config/zsh/completions/_recast
recast --completions fish > ~/.config/fish/completions/recast.fish
```

Also supported: `elvish`, `powershell`.

### Agent-friendly JSON

```bash
recast --json --apply 'Old' 'New' src/
# {"kind":"apply","outcome":"changes","files_scanned":12,"files_written":3,"total_matches":7}
```

Schema documented in [`PLAN.md §7.1`](./PLAN.md#71-json-output-schema).
Snapshot-locked in `crates/recast-core/src/snapshots/` — every
field-name / shape change is a visible PR diff.

## Exit codes

| Code | Meaning |
|-----:|---------|
| `0`  | Success, or "no changes needed" |
| `1`  | `--check` set and at least one file would change |
| `2`  | Match-count guard violated (`--at-least` / `--at-most`) |
| `3`  | Internal error (regex parse, I/O, non-convergent pattern, script error, …) |

## Cargo features

| Feature       | Default | What it enables                                              |
|---------------|:-------:|--------------------------------------------------------------|
| `script`      | ✅      | Rhai script callback (`--script`)                            |
| `lang-rust`   | ✅      | Rust grammar for structural mode                             |
| `lang-ts`     | ✅      | TypeScript + TSX grammars                                    |
| `lang-js`     | ✅      | JavaScript + JSX grammar                                     |
| `lang-python` | ✅      | Python grammar                                               |
| `lang-bash`   | ✅      | Bash grammar                                                 |
| `lang-go`     | ✅      | Go grammar                                                   |
| `lang-json`   | ✅      | JSON grammar                                                 |
| `lang-md`     | ✅      | Markdown grammar                                             |
| `lang-all`    | ✅      | Meta — enables every `lang-*` above                          |

Structural mode requires at least one `lang-*` feature. Drop the ones
you don't need to keep the binary lean:

```bash
cargo install --path crates/recast \
  --no-default-features \
  --features script,lang-rust,lang-ts
```

## Build from source

```bash
cargo build --release --workspace --all-features
cargo test --workspace --all-features
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
```

136 tests on Linux + macOS. Proptest harness covers every public
entry point with randomized input.

## For AI agents (MCP server)

`recast-mcp` exposes the engine as a [Model Context Protocol] server
that MCP-aware agents (Claude Desktop, Cursor, Continue, Cline, …)
discover automatically through their tool registry. Same engine as
`recast-cli`, library-linked — no subprocess, no shell escaping, no
version skew.

```bash
cargo install recast-mcp
```

Add to your MCP client config (Claude Desktop example):

```json
{
  "mcpServers": {
    "recast": { "command": "recast-mcp" }
  }
}
```

Restart the client. Four tools become available:

| Tool | Purpose |
|---|---|
| `recast_preview` | Dry-run a regex rewrite, return plan + diffs. |
| `recast_apply`   | Atomically apply a regex rewrite to disk. |
| `recast_structural` | Tree-sitter `--ast` rewrite (dry-run or apply). |
| `recast_recover` | Reconcile leftover `.recast.bak.*` / `.tmp.*` siblings. |

Why agents reach for it instead of `write_file` loops or `sed`:
default `--at-least 1` guard turns silent zero-match runs into
errors, convergence check refuses non-idempotent patterns, two-phase
commit rolls back mid-failure, and every response is structured JSON
so the agent can branch on `kind` without string-matching error
messages.

To actually make the LLM pick recast over its default `Edit` muscle
memory, paste the [agent rules snippet][agent-rules] into your
project's `AGENTS.md` / `CLAUDE.md` / `.cursor/rules`. The snippet
encodes the "3+ files → recast" decision rule the agent's
tool-ranker needs to flip its default.

[Model Context Protocol]: https://modelcontextprotocol.io
[agent-rules]: https://stoica-mihai.github.io/recast/agent-rules.html

### Benchmarks

```bash
cargo bench -p recast-core --features lang-rust,script --bench engine
```

Criterion suite under `crates/recast-core/benches/engine.rs` measures
`plan_rewrite`, `plan_structural_rewrite`, `pattern_compile`, and the
structural rewrite hot path. HTML reports land under
`target/criterion/`.

### Fuzzing

The `fuzz/` crate (excluded from the workspace) holds `cargo-fuzz`
targets for the byte-walker / parser / compiler surfaces. Nightly +
`cargo-fuzz` required; see [`fuzz/README.md`](./fuzz/README.md) for
the target list and run instructions.

## License

MIT. See [`LICENSE`](./LICENSE).

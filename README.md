# recast

CLI for safe, atomic, transparent multi-file text rewrites. Pure Rust.
Tuned for LLM coding agents driving mechanical edits; equally usable by
humans for mechanical refactors.

`recast` differs from `sed` / `sd` / a Python heredoc in five places:

1. **Match-required guard.** Default `--at-least 1` exits non-zero when
   nothing matches. Silent no-match is impossible by default.
2. **Idempotency check.** Refuses non-convergent rewrites; reports
   "already applied" on the second run.
3. **Atomic apply.** Two-phase commit (sibling temp + fsync + rename)
   with rollback if any per-file step fails. Crash-recovery sweep
   reconciles leftover `.recast.bak` / `.recast.tmp` siblings.
4. **Agent-friendly JSON.** `--json` emits a single-line,
   schema-locked report for plan / apply / check / error.
5. **Three pattern modes.** Regex (default), Rhai script callback
   (`--script`), or tree-sitter structural (`--lang` + `--query` /
   `--ast`).

Status: alpha (v0.1.3). All phases of [`PLAN.md`](./PLAN.md) landed.
Pre-built binaries on the [GitHub Releases](https://github.com/Stoica-Mihai/recast/releases) page.
See [`AGENTS.md`](./AGENTS.md) for the operating manual.

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
| Windows x86_64    | `x86_64-pc-windows-msvc`                                 |

The `musl` builds are statically linked — drop into an Alpine container
or distroless image without a glibc dependency.

### Cargo install (from source)

```bash
git clone https://github.com/Stoica-Mihai/recast
cd recast
cargo install --path crates/recast            # full feature set
cargo install --path crates/recast --no-default-features --features lang-rust  # slim
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

122 tests on Linux + macOS + Windows. Proptest harness covers every
public entry point with randomized input.

## License

MIT. See [`LICENSE`](./LICENSE).

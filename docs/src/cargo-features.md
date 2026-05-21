# Cargo features

`recast` (the binary) and `recast-core` (the library) expose a matching
set of cargo features so users can opt out of grammars or modes they
don't need.

## `recast` binary

| Feature       | Default | Pulls in                                          |
|---------------|:-------:|---------------------------------------------------|
| `script`      | ✅      | `recast-core/script` (Rhai callback)              |
| `lang-rust`   | ✅      | `tree-sitter` + `tree-sitter-rust`                |
| `lang-ts`     | ✅      | `tree-sitter` + `tree-sitter-typescript`          |
| `lang-js`     | ✅      | `tree-sitter` + `tree-sitter-javascript`          |
| `lang-python` | ✅      | `tree-sitter` + `tree-sitter-python`              |
| `lang-bash`   | ✅      | `tree-sitter` + `tree-sitter-bash`                |
| `lang-go`     | ✅      | `tree-sitter` + `tree-sitter-go`                  |
| `lang-json`   | ✅      | `tree-sitter` + `tree-sitter-json`                |
| `lang-md`     | ✅      | `tree-sitter` + `tree-sitter-md`                  |
| `lang-all`    | ✅      | Meta — enables every `lang-*` above               |

Stock install:

```bash
cargo install --path crates/recast
```

Slim install — Rust grammar only, no script engine:

```bash
cargo install --path crates/recast \
  --no-default-features \
  --features lang-rust
```

Slim install — Python + TypeScript only:

```bash
cargo install --path crates/recast \
  --no-default-features \
  --features lang-python,lang-ts
```

## `recast-core` library

The library exposes the same `lang-*` features plus `serde` (JSON output
schema). At least one `lang-*` must be enabled for structural mode to
compile in.

## Binary size

Each grammar adds ~5–15 MB of compiled tables. The full `--features
lang-all` binary is ~80 MB. A slim `lang-rust`-only build is ~25 MB.
Static musl builds are a few MB larger.

# Install

## Pre-built binary

Grab the matching artifact from the
[Releases page](https://github.com/Stoica-Mihai/recast/releases/latest).

| Platform               | Artifact                                                   |
|------------------------|------------------------------------------------------------|
| Linux x86_64 (glibc)   | `recast-vX.Y.Z-x86_64-unknown-linux-gnu.tar.gz`            |
| Linux x86_64 (musl)    | `recast-vX.Y.Z-x86_64-unknown-linux-musl.tar.gz`           |
| Linux aarch64 (glibc)  | `recast-vX.Y.Z-aarch64-unknown-linux-gnu.tar.gz`           |
| Linux aarch64 (musl)   | `recast-vX.Y.Z-aarch64-unknown-linux-musl.tar.gz`          |
| macOS Intel            | `recast-vX.Y.Z-x86_64-apple-darwin.tar.gz`                 |
| macOS Apple Silicon    | `recast-vX.Y.Z-aarch64-apple-darwin.tar.gz`                |
| Windows x86_64         | `recast-vX.Y.Z-x86_64-pc-windows-msvc.zip`                 |

The `musl` builds are statically linked — drop into Alpine, distroless,
or scratch containers without a glibc dependency.

Each archive ships with a `.sha256` sidecar; verify before extracting:

```bash
shasum -a 256 -c recast-v0.1.3-x86_64-unknown-linux-gnu.tar.gz.sha256
tar -xzf recast-v0.1.3-x86_64-unknown-linux-gnu.tar.gz
sudo install -m 0755 recast /usr/local/bin/
```

## Cargo install (crates.io)

```bash
cargo install recast-cli
```

The crate is published as **`recast-cli`** on crates.io (the bare
`recast` name was already claimed by an unrelated serialization
library). The installed binary is still called `recast` — every
`recast …` command in these docs works as written.

The stock install ships every grammar, the Rhai script engine, and JSON
output. Slim it down with `--no-default-features` and pick only the
features you actually use — see [Cargo features](./cargo-features.md).

```bash
cargo install recast-cli --no-default-features --features lang-rust
```

## From source

```bash
git clone https://github.com/Stoica-Mihai/recast
cd recast
cargo install --path crates/recast
```

## Shell completions

```bash
recast --completions bash  > /etc/bash_completion.d/recast
recast --completions zsh   > ~/.config/zsh/completions/_recast
recast --completions fish  > ~/.config/fish/completions/recast.fish
```

Also supported: `elvish`, `powershell`.

## Verify

```bash
recast --version
# recast 0.1.3
```

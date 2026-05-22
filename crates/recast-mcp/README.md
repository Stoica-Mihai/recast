# recast-mcp

[![crates.io](https://img.shields.io/crates/v/recast-mcp.svg?label=recast-mcp)](https://crates.io/crates/recast-mcp)
[![crates.io](https://img.shields.io/crates/v/recast-core.svg?label=recast-core)](https://crates.io/crates/recast-core)
[![docs.rs](https://img.shields.io/docsrs/recast-core)](https://docs.rs/recast-core)
[![license](https://img.shields.io/crates/l/recast-mcp.svg)](https://github.com/Stoica-Mihai/recast/blob/main/LICENSE)

Model Context Protocol server exposing [recast](https://github.com/Stoica-Mihai/recast)'s
safe, atomic, multi-file rewrite engine to MCP-aware AI agents
(Claude Desktop, Cursor, Continue, Cline, Aider, custom MCP clients).

Same engine as `recast-cli`, library-linked — no subprocess hop, no
CLI string assembly on the agent side, no version skew between
client and engine.

## Install

```bash
cargo install recast-mcp
```

Pre-built binaries for Linux (x86_64 / aarch64, gnu + musl) and macOS
(x86_64 / aarch64) are also attached to each
[GitHub release](https://github.com/Stoica-Mihai/recast/releases).

## Configure your MCP client

The server speaks JSON-RPC over stdio. Add one entry to your client's
MCP config.

**Claude Desktop** (`~/Library/Application Support/Claude/claude_desktop_config.json`
on macOS, `%APPDATA%\Claude\claude_desktop_config.json` on Windows):

```json
{
  "mcpServers": {
    "recast": { "command": "recast-mcp" }
  }
}
```

**Cursor** (`.cursor/mcp.json` in your project, or `~/.cursor/mcp.json` globally):

```json
{
  "mcpServers": {
    "recast": { "command": "recast-mcp" }
  }
}
```

**Continue / Cline / other MCP clients:** same shape — point at the
`recast-mcp` binary on `PATH`. Restart the client.

## Tools exposed

| Tool | Purpose |
|---|---|
| `recast_preview`    | Dry-run a regex rewrite, return per-file plan + unified diffs. |
| `recast_apply`      | Atomically apply a regex rewrite to disk. Two-phase commit with rollback. |
| `recast_structural` | Tree-sitter `--ast` rewrite (dry-run or apply). Friendly `fn $NAME() {}` patterns supported. |
| `recast_recover`    | Reconcile leftover `.recast.bak.*` / `.recast.tmp.*` siblings from a crashed apply. |

Each tool accepts typed JSON args (validated against the schema the
server advertises during MCP handshake) so the agent can't malform
calls. Engine errors propagate as MCP errors with a stable `kind`
discriminator (`too_few_matches`, `non_convergent`, `io`, …) — agents
branch on `kind` instead of string-matching messages.

## Why agents reach for it

Multi-file rewrites driven by `write_file` loops, `sed`, or python
heredocs share two silent failure modes:

- **Silent zero-match.** The pattern doesn't fire; nothing changes;
  the agent reports success. `recast_preview` / `recast_apply`
  default to `at_least=1` so zero-match runs surface as typed
  errors.
- **Non-idempotent re-runs.** The pattern matches its own
  replacement (e.g. `a` → `aa`), so re-running corrupts the tree.
  The engine refuses non-convergent patterns before any write.

Plus first-class atomicity (two-phase commit, rollback on any
per-file failure, crash-recovery sweep), structured JSON output, and
typed error variants that survive language-agnostic agent loops.

## Example invocations

Once the server is wired into your client, the agent invokes tools
directly. As a human, you can poke the server via the official MCP
inspector:

```bash
npx @modelcontextprotocol/inspector recast-mcp
```

Or with raw JSON-RPC over stdio for smoke testing:

```bash
printf '%s\n' \
  '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"smoke","version":"0"}}}' \
  '{"jsonrpc":"2.0","method":"notifications/initialized"}' \
  '{"jsonrpc":"2.0","id":2,"method":"tools/list"}' \
  | recast-mcp
```

## Related crates

- [`recast-cli`](https://crates.io/crates/recast-cli) — same engine
  for humans on the shell.
- [`recast-core`](https://crates.io/crates/recast-core) — the engine
  library both binaries embed.

## License

MIT. See [LICENSE](https://github.com/Stoica-Mihai/recast/blob/main/LICENSE).

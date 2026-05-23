# Agent rules snippet

If you've installed `recast-mcp` for an AI agent (Claude Code, Cursor,
Continue, Claude Desktop, Cline, …), the agent gets the tools but not
always the *judgment* about when to reach for them. Default LLM
behavior is to fall back to `Edit` / `write_file` / `sed` because
those are top-of-mind and recast feels new.

Paste the block below into your project's agent-instructions file —
whichever your runtime picks up:

| Runtime         | File                                  |
| --------------- | ------------------------------------- |
| Claude Code     | `AGENTS.md` (or `CLAUDE.md`)          |
| Cursor          | `.cursor/rules/recast.md` or `.cursorrules` |
| Continue        | `.continue/rules.md`                  |
| Cline           | `.clinerules`                         |
| Aider           | `CONVENTIONS.md`                      |
| Generic / other | any markdown file your agent reads at session start |

The snippet doubles up with the `recast-mcp` server's own
`instructions` field (which the client injects into the system prompt
during MCP handshake). Two prompts hitting the same nail — the agent
hears the decision rule from both sides.

## Copy-paste block

````markdown
## Multi-file rewrites — use `recast` MCP tools

For ANY edit that touches the same syntactic shape in 3+ files
(renames, struct-field additions, fn-signature changes, enum-variant
reshapes, version bumps, dependency migrations), call
`recast_preview` first, inspect the diff, then `recast_apply`.

Do NOT default to:
- `Edit` / `write_file` loops over N files
- `sed -i` across the tree
- per-file copy-paste of the same transform

Those silently fail on zero matches, can't roll back mid-failure,
and blast-radius across unintended sites. The recast engine catches
all three: typed `too_few_matches` error on zero hits, atomic
two-phase commit with rollback per file, and a convergence check
that refuses non-idempotent patterns (`a` → `aa`) before any write.

### Decision flow

1. About to make the same change in **3+ files** → `recast_preview`.
2. About to make a **shape-sensitive** change (struct literal, enum
   variant, fn signature, AST node) → `recast_structural` with
   `ast_pattern` instead of regex.
3. Preview returned 0 matches → **iterate the pattern**, do NOT fall
   back to per-file `Edit`.
4. Preview looks right → `recast_apply` with identical args.
5. A prior apply was killed mid-run → `recast_recover`.

### Concrete triggers

- "Rename `OldName` to `NewName` across the codebase"
- "Add field `direction: None` to every `ClientMessage::SplitPane { ... }` literal"
- "Update the N call sites of `pane_title(state)` to `pane_title(state, slot)`"
- "Bump every `1.2.3` to `1.2.4` in the workspace"
- "Migrate `.unwrap()` to `.expect("…")` everywhere"

### Tool pick

- `recast_apply` — regex / literal / Rhai script. Works on any
  language; text-level.
- `recast_structural` — tree-sitter `ast_pattern`. Use when the
  change is shape-sensitive. Supported langs: rust, ts, tsx, js,
  python, bash, go, json, markdown.
- `recast_recover` — only after a crash mid-apply.
````

## Why this is needed (the survey)

A Claude Code session was given recast-mcp access and asked to do a
multi-file refactor. The agent defaulted to `Edit` for every change
even though `recast_apply` would have done the whole sweep in one
call. Post-session debrief:

> "Edit was top-of-mind every time. Each individual change felt small
> enough to justify staying in the familiar tool. The compound cost of
> 'small Edit × 50' sneaked past me."

> "I usually saw 'this is a repeated transform across N sites' only
> AFTER hitting the third or fourth site. By then I'd already done the
> manual edits and finishing felt cheaper than switching tools."

The fix: tell the agent the decision rule directly. The system prompt
beats latent tool-ranker preferences every time.

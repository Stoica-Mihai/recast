# Structural mode (`--lang`)

Tree-sitter-backed AST matching. Pick a language with `--lang`, then
pass either a friendly `--ast` pattern or a raw tree-sitter `--query`.

## Supported languages

| Language    | CLI name                       | Cargo feature |
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

YAML / TOML are pending — the upstream tree-sitter grammars don't yet
target the v0.25 ABI recast uses.

## Friendly `--ast` patterns

Write the pattern in the target language with `$NAME` (single-node) and
`$$$NAME` (variable-shape subtree) metavars:

```bash
recast --lang rust --apply \
  --ast 'fn $NAME($$$ARGS) { $$$BODY }' \
  '' 'fn ${NAME}_v2$ARGS $BODY' \
  src/
```

Matches every function regardless of signature or body shape; renames it
and keeps the original args + body verbatim.

### Metavar rules

- **`$NAME`** matches a single AST node at the placeholder's position.
- **`$$$NAME`** matches whatever the surrounding node contains (any number
  of statements, params, fields, etc.). The capture text is the *whole
  wrapper node*: `{ $$$BODY }` captures `{ ... }`, not just the inside.
  Templates therefore should *not* re-add the wrapper: write `$BODY`, not
  `{ $BODY }`.
- Literal identifiers in the pattern (anything that isn't a metavar) must
  match exactly — `fn old_name() {}` only matches `fn old_name() {}`, not
  every nullary empty-body function.

### Template substitution

Templates use the same `$NAME` / `${NAME}` substitution rules. `${NAME}`
is needed when the name is followed by `_` / alphanumeric characters that
would otherwise extend the identifier:

```text
fn ${NAME}_v2     # explicit boundary
fn $NAME_v2       # error: no capture named `NAME_v2`
```

## Raw `--query` patterns

Pass a tree-sitter S-expression query directly. Use this when you need
predicates (`#eq?`, `#match?`, …) or want to scope a match to a specific
node kind:

```bash
recast --lang rust --apply \
  --query '((identifier) @id (#eq? @id "old_name"))' \
  '' 'new_name' src/
```

The capture named `@root` (or, absent that, the outermost capture in
each match) defines the byte range to replace. Templates can reference
any captured node by name (`$id`, `${id}`).

## Deleting items with their attributes (`--include-leading-attrs`)

Deleting a function with a plain match replaces only the
`function_item` node — its `#[test]` / `#[cfg(...)]` attributes and
`///` doc comments are siblings, so they survive as orphans:

```bash
# leaves an orphaned `#[test]` behind
recast --lang rust --apply --ast 'fn drop_me() {}' '' '' src/
```

`--include-leading-attrs` extends each match backward over the
contiguous run of preceding `attribute_item` / doc-comment siblings, so
the attributes and docs go with the item:

```bash
recast --lang rust --apply --include-leading-attrs \
  --ast 'fn drop_me() {}' '' '' src/
```

A blank line ends the run (an attribute separated from the item by an
empty line is treated as detached and left in place), and plain `//` /
`/* */` comments are never swallowed — only doc comments (`///`, `//!`,
`/**`, `/*!`). The node kinds are Rust's; languages without
`attribute_item` simply never extend. MCP: `include_leading_attrs: true`
on `recast_structural`.

## Error messages

When a query fails to compile, recast surfaces a line/column-pinned error:

```text
recast: structural: query error: tree-sitter query unknown node
  type error at line 1, column 2: zzz
  | (zzz) @x
  |  ^
```

When a friendly `--ast` pattern fails to parse, the error mentions which
grammar choked and what the legal positions for metavars are.

## When to use what

- **Identifier-level renames across a tree** → `--query '((identifier) @id (#eq? @id "X"))'`.
- **Whole-construct rewrites that need shape** → `--ast`.
- **Cross-cutting transforms that need predicates or alternatives** →
  raw `--query` with `#eq?`, `#match?`, etc.

For anything more complex than what tree-sitter Query expresses,
[ast-grep](https://ast-grep.github.io/) or [comby](https://comby.dev/)
are richer. recast's structural mode is intentionally minimal — it
trades expressiveness for the same atomicity / idempotency / JSON-output
guarantees the other modes have.

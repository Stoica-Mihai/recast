# Rename mode (`--rename`)

Apply N renames in **one** pass.

```bash
recast --apply --rename 'Foo=Bar' --rename 'Bar=Baz' src/
```

## The failure this exists to prevent

Run those two renames as separate commands and they eat each other:

```bash
recast --apply '\bFoo\b' 'Bar' src/     # exit 0
recast --apply '\bBar\b' 'Baz' src/     # exit 0
```

```rust
// before                      // after
use Foo;                       use Baz;
use Bar;                       use Baz;
```

The original `Foo` and the original `Bar` are now the same name, and
nothing complained. Both runs matched, both were convergent on their
own, both exited 0. No guard in recast looks across invocations.

One `--rename` pass keeps them apart:

```rust
use Bar;
use Baz;
```

## What a map may be

Keys match as **whole words** and are **literal** — this is a map over
identifiers, not a regex pipeline. That restriction is what makes the
check below decidable.

| Shape | Example | Verdict |
|---|---|---|
| Independent | `Foo=Qux`, `Bar=Quux` | Accepted, re-runnable |
| Chain | `Foo=Bar`, `Bar=Baz` | Accepted, correct **exactly once** |
| Permutation | `Foo=Bar`, `Bar=Foo` | Accepted, correct **exactly once** |
| Self-feeding | `Foo=Foo Bar` | **Refused** |

"Correct exactly once" means a replacement reuses a name the map also
renames, so a second run keeps rewriting. recast says so on stderr, and
the `recast_rename` MCP tool returns the same sentence alongside the
plan:

```
recast: this map is correct exactly once — a replacement reuses a name
the map also renames, so running it again would keep rewriting.
```

That is a statement about the map, not a warning about this run. The run
itself is correct.

## How the check works

recast re-applies the map to its own replacements until the text stops
changing or repeats a string it has already seen.

- Settles immediately → no replacement contains a key → re-runnable.
- Settles after a few rounds, or returns to a string already seen → a
  chain or a cycle. Bounded, so correct once.
- Never settles → the map feeds itself. Refused with
  `rename_map_diverges`.

```bash
recast --rename 'Foo=Foo Bar' src/
# recast: rename map does not settle: re-applying it to its own
# replacement of `Foo` -> `Foo Bar` kept producing new text after 64
# rounds, so the map feeds itself and grows.
```

This is a different question from the ordinary [idempotency
check](./regex-mode.md#idempotency-check). That one asks "is this
rewrite a no-op the second time"; a permutation is not, and would be
rejected. This one asks "does this map terminate", which a permutation
does.

## Key order does not matter

Keys are sorted longest-first before they are compiled into one
alternation, because the `regex` crate matches alternation
leftmost-first. Without the sort, `Foo=A` listed before `Foo-Bar=B`
would rewrite `Foo-Bar` into `A-Bar`.

```bash
recast --apply --rename 'Foo=A' --rename 'Foo-Bar=B' src/   # Foo-Bar → B
recast --apply --rename 'Foo-Bar=B' --rename 'Foo=A' src/   # same
```

## Notes

- `OLD=NEW` splits on the **first** `=`, so `=` may appear in NEW.
- `--rename` conflicts with `--script`, `--lang`, `--search`,
  `--literal`, `--word`, `--ignore-case`, and `--single-line`. The
  pattern-shaping flags are rejected rather than ignored: renames are
  always literal and always whole-word.
- PATTERN and REPLACEMENT are not used; pass paths directly.
- Works with `--stdin`, `--check`, `--diff`, `--json`, the match-count
  guard, and every filter flag.

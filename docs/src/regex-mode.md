# Regex mode

Default mode. Powered by the [`regex`](https://docs.rs/regex) crate, so
the syntax is Perl-compatible minus lookaround (catastrophic backtracking
is gone too — `regex` is linear-time).

## Multi-line by default

`.` matches `\n` by default (implicit `(?s)`); `--single-line` (`-s`)
turns that off. This matches what LLMs usually expect.

## Capture interpolation

```bash
recast 'fn (\w+)_old\b' 'fn ${1}_new' src/
```

`$1`, `${name}` interpolated. To treat the pattern and replacement as
literal text, pass `--literal` (`-L`).

## Case-insensitive

```bash
recast -i 'todo' 'TODO' .
```

## Match-count guard

`--at-least N` (default `1`) and `--at-most N` (default unbounded) bracket
the total matches across all files. Violations exit `2`.

```bash
recast --at-least 5 'foo' 'bar' src/       # require ≥5 matches
recast --at-most 0 'TODO' 'FIXME' src/     # CI gate: there must be zero TODOs
recast --at-least 0 'maybe' 'def' src/     # allow zero matches (no guard)
```

## Idempotency check

The plan step reapplies the pattern to its own post-image. If any file
would change again, recast aborts with `non_convergent` — the rewrite
isn't safe to run twice. Override with `--allow-non-convergent` if you
know what you're doing.

Examples of patterns recast rejects:

- `'a' -> 'aa'`   (grows on every run)
- `'foo' -> 'foofoo'`

Examples it accepts:

- `'old' -> 'new'`
- `'fn (\w+)_old' -> 'fn ${1}_new'`

## Filters

```bash
recast -t rust 'Old' 'New' .                # only Rust files
recast -T markdown 'Old' 'New' .            # everything except Markdown
recast -g '!vendor/**' 'Old' 'New' .        # exclude vendor dir
recast --no-ignore 'Old' 'New' .            # bypass .gitignore
recast --hidden 'Old' 'New' .               # include dot-files
recast --max-bytes 102400 'Old' 'New' .     # skip files > 100KiB
recast --max-files 50 'Old' 'New' .         # cap total file count
```

`-t` / `-T` accept the same shorthand vocabulary as ripgrep
(`rust`, `js`, `py`, `markdown`, …). `-g` accepts ripgrep-style
include/exclude globs.

## Stdin mode

```bash
echo 'fn old_name() {}' | recast --stdin 'old_name' 'new_name'
# fn new_name() {}
```

Read one buffer, rewrite once, write to stdout. Skips the walker and
commit phases. The match-count guard still applies.

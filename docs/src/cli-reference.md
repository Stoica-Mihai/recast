# CLI flags

```text
recast [OPTIONS] <PATTERN> <REPLACEMENT> [PATHS]...
```

## Modes

| Flag                | Effect                                                     |
|---------------------|------------------------------------------------------------|
| (default)           | Diff preview to stdout                                     |
| `--apply`           | Atomic two-phase commit                                    |
| `--check`           | Exit 1 if any file would change; no output, no writes      |
| `--stdin`           | Read one buffer from stdin → stdout                         |
| `--recover`         | Reconcile leftover `.recast.bak.*` / `.recast.tmp.*` siblings |

## Match guards

| Flag                | Default | Effect                                          |
|---------------------|---------|-------------------------------------------------|
| `--at-least N`      | `1`     | Require at least N matches across all files     |
| `--at-most N`       | `∞`     | Require at most N matches                       |
| `--allow-non-convergent` | off | Skip the idempotency check                     |
| `--allow-syntax-errors` | off | Skip the syntax-regression guard (see [Safety](safety.md)) |

## Filters

| Flag                  | Effect                                                 |
|-----------------------|--------------------------------------------------------|
| `-t LANG`, `--type LANG`     | Include only files of this type (`rust`, `js`, `py`, …) |
| `-T LANG`, `--type-not LANG` | Exclude this file type                              |
| `-g PAT`, `--glob PAT`        | Include / exclude glob (`!pat` to exclude)         |
| `--hidden`            | Include dot-files                                      |
| `--no-ignore`         | Bypass `.gitignore` filtering                          |
| `--max-bytes N`       | Refuse files larger than N bytes (default 10 MiB)      |
| `--max-files N`       | Refuse runs touching more than N files (default 1000)  |

## Regex options

| Flag                  | Effect                                              |
|-----------------------|-----------------------------------------------------|
| `-L`, `--literal`     | Treat pattern + replacement as literal text         |
| `-i`, `--ignore-case` | Case-insensitive                                    |
| `-s`, `--single-line` | Disable implicit `(?s)` — `.` no longer matches `\n` |

## Script mode

| Flag           | Effect                                                |
|----------------|-------------------------------------------------------|
| `--script PATH`| Rhai script file run per match; return value = replacement |

## Structural mode

| Flag                   | Effect                                            |
|------------------------|---------------------------------------------------|
| `--lang LANG`          | Tree-sitter grammar (`rust`, `ts`, `python`, …)   |
| `--query QUERY`        | Raw tree-sitter S-expression query                |
| `--ast PATTERN`        | Friendly source-shaped pattern with `$NAME` metavars |
| `--include-leading-attrs` | Extend each match backward over leading `#[attr]` / doc-comment lines (see [Structural mode](structural-mode.md)) |

## Output

| Flag                   | Effect                                            |
|------------------------|---------------------------------------------------|
| `--json`               | Emit single-line JSON on stdout                   |
| `--quiet`              | Suppress diff body; print only the summary        |
| `-v`, `--verbose`      | Per-file timing and counters                      |

## Misc

| Flag                   | Effect                                            |
|------------------------|---------------------------------------------------|
| `--threads N`          | Worker threads (default = num CPUs)               |
| `--force`              | Bypass the workspace lock check                   |
| `--completions SHELL`  | Print shell completion script to stdout           |
| `--help`               | Help summary                                      |
| `--version`            | Version string                                    |

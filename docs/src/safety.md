# Safety guarantees

The six things that make recast safer than `sed` / `sd` / a Python
heredoc.

## 1. Match-required guard

Default `--at-least 1` makes a silent zero-match exit impossible. An
agent that types the wrong pattern gets an immediate non-zero exit
instead of "looks like it worked".

Override with `--at-least 0` if you really do want to allow no-op runs.

## 2. Idempotency / convergence

Before any write, recast re-applies the rewrite to its own post-image.
If any file would change again, the run is aborted with a
`non_convergent` error.

Examples that pass:
- `'old' -> 'new'`
- `'fn (\w+)_old' -> 'fn ${1}_new'`

Examples that get rejected:
- `'a' -> 'aa'` (grows on every run)
- `'foo' -> 'foofoo'`

A successful first run followed by a re-run reports `already_applied`
and exits 0, so retry loops are safe.

## 3. Syntax-regression guard

For every changed file whose extension maps to a compiled tree-sitter
grammar (`.rs`, `.ts`, `.tsx`, `.js`/`.mjs`/`.cjs`/`.jsx`, `.py`,
`.sh`/`.bash`, `.go`, `.json`, `.md`), recast re-parses the post-image
and counts parse errors. If the rewrite introduces *new* errors
relative to the pre-image, the run is aborted with a
`syntax_regression` error before anything is written.

The comparison is a count delta, so a file that was already unparsable
(mid-refactor, exotic macro) stays acceptable as long as the rewrite
doesn't make it worse. This catches a greedy regex that strands a brace
or truncates an expression.

```text
# regex deletes the `fn open(` line but leaves the body + closing brace
recast --apply 'fn open\(\) \{\n' '' src/   # → syntax_regression, nothing written
```

**Limitation — syntactic, not semantic.** The guard sees parse errors,
not compiler errors. Deleting a function body while leaving its
`#[test]` attribute behind produces *valid syntax* (the attribute just
binds to the next item); tree-sitter does not flag it, so the guard does
not fire. That class is a job for `cargo check` or, better, for
[structural mode](structural-mode.md), which removes the attribute along
with the item.

Override per run with `--allow-syntax-errors` (CLI) /
`allow_syntax_errors: true` (MCP). Files with no compiled grammar — and
`--no-default-features` builds with every `lang-*` feature off — skip the
guard and pass through unchecked.

## 4. Two-phase atomic apply

```
Phase A (stage)   per file: write sibling .recast.tmp, fsync, copy mode
Phase B (commit)  per file: rename original→.recast.bak, rename .tmp→original
Phase C (cleanup) per dir:  delete backups, fsync parent dir
```

Any failure in Phase B walks the rename log in reverse — every
already-renamed file is restored from its backup, leaving the tree
bit-identical to the pre-image. Failure in Phase A just deletes the
staged temps; originals are never touched.

This applies to regex, script, and structural modes.

## 5. Crash-recovery sweep

If the process dies mid-commit (SIGKILL, panic, power loss), the tree
may be left with leftover `.foo.recast.bak.N` / `.foo.recast.tmp.N`
siblings. Reconcile with:

```bash
recast --recover src/
```

- Target exists + stale backup/temp → delete leftovers
- Target missing + backup present → rename newest backup back to target
- Target missing + only temps → leave untouched (can't safely decide)

## 6. Workspace lock

`--apply` and `--recover` take an exclusive non-blocking lock on
`<root>/.recast.lock` so two concurrent rewrites against the same tree
don't interleave. Second invocation gets an immediate
`locked` error with exit 3 instead of corrupting the tree.

`--force` bypasses the lock for cases you genuinely understand (e.g.,
the previous holder crashed and you've already run `--recover`).
`--check` and `--diff` skip the lock since they don't write.

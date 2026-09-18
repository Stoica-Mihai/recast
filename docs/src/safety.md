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
If any file would change again, the run is aborted with one of three
errors, split by cause because the fixes differ:
`non_convergent_replacement` (the replacement still matches the pattern
— narrow it, usually with word boundaries), `non_convergent_context`
(the replacement is clean but the rewrite pulls surrounding text into a
new match — word boundaries will not help), and
`non_convergent_script` (a `--script` run, where the replacement is
computed per match so the cause is not knowable up front). All three
name `--allow-non-convergent` as the override.

Examples that pass:
- `'old' -> 'new'`
- `'fn (\w+)_old' -> 'fn ${1}_new'`

Examples that get rejected:
- `'a' -> 'aa'` (grows on every run)
- `'foo' -> 'foofoo'`

A successful first run followed by a re-run has nothing left to match,
so it is the guard in section 1 that decides the outcome, not
convergence. Re-run with `--at-least 0` to get `already_applied` and
exit 0; the default `--at-least 1` exits 2, because from the planner's
side an already-converted tree and a mistyped pattern are the same
observation.

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

`--apply` and `--recover` take an exclusive non-blocking lock so two
concurrent rewrites against the same tree don't interleave. The second
invocation gets an immediate `locked` error with exit 3 instead of
corrupting the tree. The same lock now covers the MCP `recast_apply`,
`recast_structural` (with `apply`), and `recast_recover` tools.

**What the lock is keyed on.** The enclosing VCS checkout — the nearest
ancestor holding `.git`, `.hg`, `.jj`, or `.svn` — not the paths a given
invocation happens to name. So `recast --apply … src/` and
`recast --apply … src/sub/` contend with each other, as do two runs from
different working directories.

**Where the file lives.** `$XDG_RUNTIME_DIR/recast/<hash>.lock`, or the
system temp directory when `XDG_RUNTIME_DIR` is unset. Never inside your
working tree, so a rewrite leaves no untracked file behind. The name is
a stable hash of the canonical root; the error message names the tree so
you don't have to decode it.

**The file is never deleted, on purpose.** Unlinking a lockfile on
release breaks mutual exclusion: a process still holding a descriptor on
the old inode and a process creating a fresh file at the same path end
up locking two different objects, and both succeed. The leftover files
are zero bytes and live in a directory the system clears.

**Limits worth knowing.**

- The lock is advisory and per-user. `$XDG_RUNTIME_DIR` is mode `0700`,
  so two *different* users rewriting one shared tree will not see each
  other's lock.
- A tree with no VCS marker has no principled root, so it falls back to
  the common ancestor of the path arguments. For those trees the
  `src/` versus `src/sub/` gap above still applies.
- Atomicity is per-invocation, not cross-invocation. The lock stops
  interleaving; it is not a transaction across separate runs.

`--force` bypasses the lock for cases you genuinely understand (e.g.,
the previous holder crashed and you've already run `--recover`).
`--check` and `--diff` skip the lock since they don't write.

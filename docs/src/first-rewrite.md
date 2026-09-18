# First rewrite

The three-step flow agents and humans both use.

## 1. Preview

```bash
recast 'OldName' 'NewName' src/
```

Output:

```diff
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -1 +1 @@
-fn OldName() {}
+fn NewName() {}
recast: 1 file(s) would change, 1 match(es) across 8 scanned.
```

No writes happen until you pass `--apply`.

## 2. Apply

```bash
recast --apply 'OldName' 'NewName' src/
```

Output:

```
recast: applying 1 file(s), 1 match(es).
```

Under the hood: every file is staged in a sibling `.recast.tmp.N`
(written + `fsync`'d), then renamed into place via a per-file
`original → .recast.bak.N` / `temp → original` swap. A failure at any
step reverse-renames every committed file from its backup, leaving the
tree bit-identical to the pre-image. See [Safety guarantees](./safety.md).

## 3. Re-run

```bash
recast --apply 'OldName' 'NewName' src/
# recast: match-count guard violated: found 0, required at least 1
# exit 2
```

The second run finds nothing to match, and by default that is a guard
violation — the same answer you get for a mistyped pattern, because the
two are indistinguishable from the outside.

For a retry loop in CI or from an LLM agent, say so explicitly:

```bash
recast --apply --at-least 0 'OldName' 'NewName' src/
# recast: already applied; no changes needed.
# exit 0
```

If the pattern is non-convergent (e.g. `'a' -> 'aa'`), `recast` refuses
with a `non_convergent` error before touching any file.

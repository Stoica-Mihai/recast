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

## 3. Re-run is safe

```bash
recast --apply 'OldName' 'NewName' src/
# recast: already applied; no changes needed.
```

`recast` checks convergence (re-applying the pattern to its own output
produces no further change) and reports "already applied" with exit 0,
so retrying a rewrite in CI or from an LLM-agent retry loop is safe.

If the pattern is non-convergent (e.g. `'a' -> 'aa'`), `recast` refuses
with a `non_convergent` error before touching any file.

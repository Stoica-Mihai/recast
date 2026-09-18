# Exit codes

| Code | Meaning                                                                 |
|-----:|-------------------------------------------------------------------------|
| `0`  | Success, or "no changes needed"                                         |
| `1`  | `--check` set and at least one file would change                        |
| `2`  | Match-count guard violated (`--at-least` / `--at-most`)                  |
| `3`  | Internal error (regex / glob parse, I/O, non-convergent pattern, syntax regression, script error, structural query error, workspace lock held, …) |

Agents can branch on these without parsing stdout. Combined with `--json`,
exit-code 2 always pairs with `kind: "error"` + `error: "too_few_matches"`
or `"too_many_matches"`, and exit-code 3 with one of the remaining
`error` discriminants.

## Examples

```bash
recast --check --at-least 0 'TODO' 'FIXME' .
echo "exit=$?"
# exit=0 → no files would change (clean)
# exit=1 → at least one file would change (CI gate fail)

recast --at-least 5 'foo' 'bar' src/
# exit=2 → fewer than 5 matches; nothing applied
```

**A `--check` gate needs `--at-least 0`.** Without it, a clean tree is a
zero-match run and exits 2, not 0 — the guard fires before `--check`
classifies anything. That is deliberate: the guard cannot tell a clean
tree from a pattern you mistyped, so it makes you say which one you
meant. A CI gate that greps for something it expects to be absent wants
`--at-least 0`; one that expects matches and asserts they all got
rewritten wants the default.

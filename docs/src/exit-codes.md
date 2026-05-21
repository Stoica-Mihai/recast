# Exit codes

| Code | Meaning                                                                 |
|-----:|-------------------------------------------------------------------------|
| `0`  | Success, or "no changes needed"                                         |
| `1`  | `--check` set and at least one file would change                        |
| `2`  | Match-count guard violated (`--at-least` / `--at-most`)                  |
| `3`  | Internal error (regex / glob parse, I/O, non-convergent pattern, script error, structural query error, workspace lock held, …) |

Agents can branch on these without parsing stdout. Combined with `--json`,
exit-code 2 always pairs with `kind: "error"` + `error: "too_few_matches"`
or `"too_many_matches"`, and exit-code 3 with one of the remaining
`error` discriminants.

## Examples

```bash
recast --check 'TODO' 'FIXME' .
echo "exit=$?"
# exit=0 → no files would change (clean)
# exit=1 → at least one file would change (CI gate fail)

recast --at-least 5 'foo' 'bar' src/
# exit=2 → fewer than 5 matches; nothing applied
```

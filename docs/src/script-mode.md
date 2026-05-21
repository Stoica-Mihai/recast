# Script mode (`--script`)

When regex's mad-libs template (`$1`, `${name}`) can't compute the
replacement, drop in a Rhai script callback.

## Quick example: version bump

```bash
cat > bump.rhai <<'RHAI'
(parse_int(captures[1]) + 1).to_string()
RHAI

echo "version 3" | recast --stdin --script bump.rhai '(\d+)' ''
# version 4
```

## API the script sees

| Binding   | Type   | Meaning                                         |
|-----------|--------|-------------------------------------------------|
| `captures`| Array  | `captures[0]` is the full match, `captures[1..]` are the named/numbered groups in order |
| `whole`   | String | Alias for `captures[0]` (`match` is a Rhai keyword) |

The return value is coerced to string and used as the replacement.

## Conditional rewrites

```rhai
if captures[1] == "old" {
    "new"
} else {
    captures[1]                 // keep as-is
}
```

## Uppercase a capture

```rhai
captures[1].to_upper()
```

## Mode notes

- The positional `REPLACEMENT` argument is still required when using
  `--script` (pass `""`); its value is ignored.
- Scripted scans run sequentially — the Rhai engine isn't `Sync`. That's
  usually fine since `--script` runs are dominated by per-script work,
  not file I/O. Use plain regex mode for big trees.
- Sandbox limits: 1 M operations, 1 MiB strings, 1024 array entries,
  expression depth 64.
- Match-count guard, idempotency check, atomic apply, and recovery all
  still apply.

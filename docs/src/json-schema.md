# JSON output schema

`--json` emits exactly **one line of compact JSON** on stdout per
invocation. Snapshot-locked in `crates/recast-core/src/snapshots/` —
changing field names or order is a breaking change.

Errors go to **stdout** too (not stderr) so agents have a single stream
to parse.

## Common shape

Every report carries a `kind` discriminator:

```text
kind ∈ "plan" | "apply" | "check" | "error"
```

Non-error reports share `outcome`, `files_scanned`, and `total_matches`.

## `plan` (default mode)

```jsonc
{
  "kind": "plan",
  "outcome": "changes" | "already_applied",
  "files_scanned": 5,
  "files_changed": 2,
  "total_matches": 3,
  "changes": [
    { "path": "src/a.rs", "matches": 2 },
    { "path": "src/b.rs", "matches": 1 }
  ]
}
```

## `apply`

```jsonc
{
  "kind": "apply",
  "outcome": "changes" | "already_applied",
  "files_scanned": 5,
  "files_written": 2,
  "total_matches": 3
}
```

## `check`

```jsonc
{
  "kind": "check",
  "outcome": "changes" | "already_applied",
  "files_scanned": 5,
  "files_would_change": 2,
  "total_matches": 3
}
```

## `error`

```jsonc
{
  "kind": "error",
  "error":
      "too_few_matches"
    | "too_many_matches"
    | "non_convergent"
    | "too_many_files"
    | "file_too_large"
    | "invalid_regex"
    | "invalid_glob"
    | "walk"
    | "io"
    | "script_parse"
    | "script_runtime"
    | "unknown_language"
    | "structural_query"
    | "structural_template"
    | "structural_parse"
    | "locked",
  "message": "human-readable description",
  "exit_code": 2 | 3
}
```

The `exit_code` field mirrors the process exit code so agents can branch
on `kind: "error"` without re-reading `$?`.

## Stability

Every shape above has an `insta` snapshot test. Any PR that changes
field names, drops a field, or reorders them shows up as a snapshot
diff in review — there's no quiet schema drift.

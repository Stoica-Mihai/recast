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

Non-error reports share `outcome`, `files_scanned`, and `total_matches`
as a header that appears in that order; the mode-specific count
(`files_changed` / `files_written` / `files_would_change`) follows.

## `plan` (default mode)

```jsonc
{
  "kind": "plan",
  "outcome": "changes" | "already_applied",
  "files_scanned": 5,
  "total_matches": 3,
  "files_changed": 2,
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
  "total_matches": 3,
  "files_written": 2
}
```

## `check`

```jsonc
{
  "kind": "check",
  "outcome": "changes" | "already_applied",
  "files_scanned": 5,
  "total_matches": 3,
  "files_would_change": 2
}
```

## `error`

```jsonc
{
  "kind": "error",
  "error":
      "too_few_matches"
    | "too_many_matches"
    | "non_convergent_replacement"
    | "non_convergent_context"
    | "non_convergent_script"
    | "invalid_rename_map"
    | "rename_map_diverges"
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
    | "locked"
    | "invalid_threads"
    | "thread_pool",
  "message": "human-readable description",
  "exit_code": 2 | 3,
  "remedies": [
      "at_least" | "at_most" | "max_bytes" | "max_files"
    | "word" | "allow_non_convergent" | "allow_syntax_errors"
    | "force_lock" | "threads"
  ]
}
```

The `exit_code` field mirrors the process exit code so agents can branch
on `kind: "error"` without re-reading `$?`.

`remedies` lists the knobs that could clear this error, most useful
first, and is empty when the fix is to change the pattern or the tree
instead. **Branch on it rather than reading `message`**, which is prose.
`recast-core` names the knob, not the flag, because it does not know
whether it is serving the CLI or the MCP server — so the same
`non_convergent_replacement` reports `["word", "allow_non_convergent"]`
here, prints `see --word / --allow-non-convergent` on the CLI, and says
`set word or allow_non_convergent` over MCP.

One knob is deliberately CLI-only. `force_lock` appears in this list but
has **no MCP argument**: a crashed holder releases its `flock` on exit,
so a held lock always means a live process is mid-apply, and the MCP
`locked` error returns `"remedies": []`. Wait and retry, or stop and
ask — there is nothing to set.

## Stability

Every shape above has an `insta` snapshot test. Any PR that changes
field names, drops a field, or reorders them shows up as a snapshot
diff in review — there's no quiet schema drift.

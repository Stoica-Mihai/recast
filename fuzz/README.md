# recast fuzzing harness

`cargo-fuzz` targets for the byte-walker / parser / compiler surfaces
that the v0.1.8 audit identified as the highest-value fuzz candidates.
The crate is excluded from the workspace (`[workspace] exclude = ["fuzz"]`)
because `cargo-fuzz` requires a nightly-only libFuzzer harness and
wants its own sanitizer flags.

## Prereqs

```bash
rustup install nightly
cargo install cargo-fuzz
```

## Targets

| Target | What it stresses |
|---|---|
| `compile_friendly_query` | Friendly `--ast` pattern compile: metavar substitution + tree-sitter parse + S-expr emit. Panic / OOM / stack-overflow on adversarial pattern. |
| `structural_rewrite_friendly` | Full friendly structural pipeline: compile + parse source + splice template + emit rewritten source. |
| `pattern_compile_convergence` | `CompiledPattern::compile` + `is_convergent`. Stresses the `replacement_probe` byte walker (UTF-8 corruption site fixed in v0.1.8). |

## Run

```bash
cd fuzz
cargo +nightly fuzz run compile_friendly_query
cargo +nightly fuzz run structural_rewrite_friendly
cargo +nightly fuzz run pattern_compile_convergence
```

A target loops forever; `Ctrl-C` to stop. Crash artifacts land in
`fuzz/artifacts/<target>/`. Re-run with the artifact path to reproduce:

```bash
cargo +nightly fuzz run compile_friendly_query \
    fuzz/artifacts/compile_friendly_query/crash-<hash>
```

## Adding targets

1. Drop a new file in `fuzz_targets/<name>.rs` using the
   `libfuzzer_sys::fuzz_target!` macro.
2. Register the binary in `fuzz/Cargo.toml` under a new `[[bin]]`
   block.
3. Keep targets read-only on the disk — fuzzers run thousands of
   iterations per second and any per-iteration filesystem state will
   dominate the signal.

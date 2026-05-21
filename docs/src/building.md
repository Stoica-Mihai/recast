# Building from source

```bash
git clone https://github.com/Stoica-Mihai/recast
cd recast
cargo build --release --workspace --all-features
```

The full test + lint matrix CI runs:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo doc --workspace --no-deps --all-features      # RUSTDOCFLAGS=-D warnings in CI
```

Property tests run as part of `cargo test`; criterion benchmarks are
opt-in:

```bash
cargo bench --features lang-rust,script -p recast-core
```

HTML bench reports land under `target/criterion/`.

## Cutting a release

1. Bump `version` in `Cargo.toml` (workspace) and the
   `recast-core` path-dep pin.
2. Promote `## [Unreleased]` → `## [X.Y.Z] — YYYY-MM-DD` in
   `CHANGELOG.md`.
3. Commit, tag, push:

   ```bash
   git commit -am "chore: bump to X.Y.Z"
   git tag -a vX.Y.Z -m "vX.Y.Z — short tagline"
   git push origin main
   git push origin vX.Y.Z
   ```

4. `.github/workflows/release.yml` cross-compiles seven targets, packages
   each, extracts the `[X.Y.Z]` CHANGELOG section as release notes, and
   attaches everything to the matching GitHub Release. Workflow_dispatch
   re-runs `gh release edit --notes-file` against an existing tag.

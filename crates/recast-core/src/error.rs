//! Typed errors returned by the planner, walker, and commit phases.
//!
//! [`Error`] is the single source of truth for failure shapes; the
//! [`crate::json::ErrorKind`] discriminator (under the `serde` feature)
//! tags each variant for machine consumption.

use std::path::{Path, PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("invalid regex: {0}")]
    InvalidRegex(#[from] regex::Error),

    #[error("invalid glob: {0}")]
    InvalidGlob(#[from] globset::Error),

    #[error("walk failed: {0}")]
    Walk(#[from] ignore::Error),

    #[error("i/o error at {path}: {source}")]
    Io { path: PathBuf, source: std::io::Error },

    #[error("file {path} exceeds --max-bytes ({size} > {limit})")]
    FileTooLarge { path: PathBuf, size: u64, limit: u64 },

    #[error("scan touched {count} files; refusing (--max-files = {limit})")]
    TooManyFiles { count: usize, limit: usize },

    #[error(
        "pattern is non-convergent: re-applying it to the rewrite of {path} would produce {extra} more match(es)"
    )]
    NonConvergent { path: PathBuf, extra: usize },

    #[error("match-count guard violated: found {found}, required at least {required}")]
    TooFewMatches { found: usize, required: usize },

    #[error("match-count guard violated: found {found}, allowed at most {allowed}")]
    TooManyMatches { found: usize, allowed: usize },

    #[error("script parse error: {0}")]
    ScriptParse(String),

    #[error("script runtime error: {0}")]
    ScriptRuntime(String),

    #[error("structural: unknown language `{0}`")]
    UnknownLanguage(String),

    #[error("structural: query error: {0}")]
    StructuralQuery(String),

    #[error("structural: template error: {0}")]
    StructuralTemplate(String),

    #[error("structural: parse error")]
    StructuralParse,

    #[error(
        "another recast is already applying to this tree (lockfile {path} held); use --force to override"
    )]
    Locked { path: PathBuf },
}

pub type Result<T> = std::result::Result<T, Error>;

/// Extension that converts a `std::io::Result<T>` into a `crate::Result<T>`
/// by attaching the offending path to the `Error::Io` variant. Cuts the
/// repeated `|e| Error::Io { path: ..., source: e }` closure that would
/// otherwise show up at every `fs::*` call site.
pub(crate) trait IoCtx<T> {
    fn io_ctx(self, path: &Path) -> Result<T>;
}

impl<T> IoCtx<T> for std::result::Result<T, std::io::Error> {
    fn io_ctx(self, path: &Path) -> Result<T> {
        self.map_err(|source| Error::Io { path: path.to_path_buf(), source })
    }
}

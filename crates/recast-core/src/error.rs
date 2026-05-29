//! Typed errors returned by the planner, walker, and commit phases.
//!
//! [`Error`] is the single source of truth for failure shapes;
//! [`ErrorKind`] is the machine-readable discriminator that tags each
//! variant for JSON output. The mapping lives here so adding an
//! [`Error`] variant without extending [`ErrorKind`] is a compile
//! error rather than a runtime mis-tag.

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
        "rewrite introduced {new_errors} new syntax error(s) in {path} ({lang}); pass allow_syntax_errors to override"
    )]
    SyntaxRegression { path: PathBuf, lang: &'static str, new_errors: usize },

    #[error(
        "another recast is already applying to this tree (lockfile {path} held); use --force to override"
    )]
    Locked { path: PathBuf },

    #[error("invalid --threads value: must be at least 1")]
    InvalidThreads,

    #[error("failed to build worker thread pool: {0}")]
    ThreadPool(String),
}

/// Machine-readable tag for an [`Error`] variant. Stable across releases;
/// every variant in [`Error`] has exactly one [`ErrorKind`] counterpart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum ErrorKind {
    InvalidRegex,
    InvalidGlob,
    Walk,
    Io,
    FileTooLarge,
    TooManyFiles,
    NonConvergent,
    TooFewMatches,
    TooManyMatches,
    ScriptParse,
    ScriptRuntime,
    UnknownLanguage,
    StructuralQuery,
    StructuralTemplate,
    StructuralParse,
    SyntaxRegression,
    Locked,
    InvalidThreads,
    ThreadPool,
}

impl Error {
    /// Tag for this variant. The match is exhaustive; adding a new
    /// variant without extending [`ErrorKind`] is a compile error.
    pub fn kind(&self) -> ErrorKind {
        match self {
            Error::InvalidRegex(_) => ErrorKind::InvalidRegex,
            Error::InvalidGlob(_) => ErrorKind::InvalidGlob,
            Error::Walk(_) => ErrorKind::Walk,
            Error::Io { .. } => ErrorKind::Io,
            Error::FileTooLarge { .. } => ErrorKind::FileTooLarge,
            Error::TooManyFiles { .. } => ErrorKind::TooManyFiles,
            Error::NonConvergent { .. } => ErrorKind::NonConvergent,
            Error::TooFewMatches { .. } => ErrorKind::TooFewMatches,
            Error::TooManyMatches { .. } => ErrorKind::TooManyMatches,
            Error::ScriptParse(_) => ErrorKind::ScriptParse,
            Error::ScriptRuntime(_) => ErrorKind::ScriptRuntime,
            Error::UnknownLanguage(_) => ErrorKind::UnknownLanguage,
            Error::StructuralQuery(_) => ErrorKind::StructuralQuery,
            Error::StructuralTemplate(_) => ErrorKind::StructuralTemplate,
            Error::StructuralParse => ErrorKind::StructuralParse,
            Error::SyntaxRegression { .. } => ErrorKind::SyntaxRegression,
            Error::Locked { .. } => ErrorKind::Locked,
            Error::InvalidThreads => ErrorKind::InvalidThreads,
            Error::ThreadPool(_) => ErrorKind::ThreadPool,
        }
    }
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

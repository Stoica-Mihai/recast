//! Typed errors returned by the planner, walker, and commit phases.
//!
//! [`Error`] is the single source of truth for failure shapes;
//! [`ErrorKind`] is the machine-readable discriminator that tags each
//! variant for JSON output. The mapping lives here so adding an
//! [`Error`] variant without extending [`ErrorKind`] is a compile
//! error rather than a runtime mis-tag.
//!
//! **Messages here name no flags.** A knob that could clear the error is
//! reported as a typed [`Remedy`]; each front end renders it in its own
//! vocabulary, because this crate does not know whether it is serving
//! the CLI (`--word`) or the MCP server (`word: true`). A test asserts
//! no message leaks either spelling.

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

    #[error("file {path} is {size} bytes, over the {limit}-byte per-file limit")]
    FileTooLarge { path: PathBuf, size: u64, limit: u64 },

    #[error("scan touched {count} files, over the limit of {limit}")]
    TooManyFiles { count: usize, limit: usize },

    #[error(
        "pattern is non-convergent: the replacement itself still matches the pattern, so re-applying the rewrite to {path} would produce {extra} more match(es); matching whole words usually fixes this"
    )]
    NonConvergentReplacement { path: PathBuf, extra: usize },

    #[error(
        "pattern is non-convergent: the replacement is clean, but the rewrite brings surrounding text in {path} into {extra} new match(es); the pattern overlaps itself, so matching whole words will not help"
    )]
    NonConvergentContext { path: PathBuf, extra: usize },

    #[error(
        "pattern is non-convergent: re-applying the script to the rewrite of {path} would produce {extra} more match(es); the script's output is dynamic, so the cause cannot be narrowed statically"
    )]
    NonConvergentScript { path: PathBuf, extra: usize },

    #[error("invalid rename map: {reason}")]
    InvalidRenameMap { reason: String },

    #[error(
        "rename map does not settle: re-applying it to its own replacement of `{key}` -> `{value}` kept producing new text after {rounds} rounds, so the map feeds itself and grows. A map may be a permutation (`Foo`->`Bar`, `Bar`->`Foo`) or a chain (`Foo`->`Bar`, `Bar`->`Baz`); it may not expand a name into text containing that same name"
    )]
    RenameMapDiverges { key: String, value: String, rounds: usize },

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

    #[error("rewrite introduced {new_errors} new syntax error(s) in {path} ({lang})")]
    SyntaxRegression { path: PathBuf, lang: &'static str, new_errors: usize },

    #[error("another recast is already applying to {root} (lockfile {path} held)")]
    Locked { path: PathBuf, root: PathBuf },

    #[error("invalid thread count: must be at least 1")]
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
    NonConvergentReplacement,
    NonConvergentContext,
    NonConvergentScript,
    InvalidRenameMap,
    RenameMapDiverges,
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

/// A knob that could clear an [`Error`], named by what it *is* rather
/// than by what any one front end calls it.
///
/// The CLI renders `Word` as `--word`; the MCP server renders it as
/// `word: true` and ships the slug in the error payload so an agent can
/// branch on it instead of parsing prose. `ForceLock` has no MCP
/// spelling on purpose — see [`Error::remedies`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum Remedy {
    AtLeast,
    AtMost,
    MaxBytes,
    MaxFiles,
    Word,
    AllowNonConvergent,
    AllowSyntaxErrors,
    ForceLock,
    Threads,
}

impl Error {
    /// Knobs that could clear this error, most useful first. Empty when
    /// the caller has to change the pattern or the tree instead.
    ///
    /// The match is exhaustive, so a new [`Error`] variant cannot be
    /// added without deciding its remedy — which is how the flag names
    /// drifted out of sync before this existed.
    pub fn remedies(&self) -> &'static [Remedy] {
        match self {
            Error::FileTooLarge { .. } => &[Remedy::MaxBytes],
            Error::TooManyFiles { .. } => &[Remedy::MaxFiles],
            Error::NonConvergentReplacement { .. } => &[Remedy::Word, Remedy::AllowNonConvergent],
            Error::NonConvergentContext { .. } | Error::NonConvergentScript { .. } => {
                &[Remedy::AllowNonConvergent]
            }
            Error::TooFewMatches { .. } => &[Remedy::AtLeast],
            Error::TooManyMatches { .. } => &[Remedy::AtMost],
            Error::SyntaxRegression { .. } => &[Remedy::AllowSyntaxErrors],
            Error::Locked { .. } => &[Remedy::ForceLock],
            Error::InvalidThreads | Error::ThreadPool(_) => &[Remedy::Threads],
            Error::InvalidRegex(_)
            | Error::InvalidGlob(_)
            | Error::Walk(_)
            | Error::Io { .. }
            | Error::InvalidRenameMap { .. }
            | Error::RenameMapDiverges { .. }
            | Error::ScriptParse(_)
            | Error::ScriptRuntime(_)
            | Error::UnknownLanguage(_)
            | Error::StructuralQuery(_)
            | Error::StructuralTemplate(_)
            | Error::StructuralParse => &[],
        }
    }

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
            Error::NonConvergentReplacement { .. } => ErrorKind::NonConvergentReplacement,
            Error::NonConvergentContext { .. } => ErrorKind::NonConvergentContext,
            Error::NonConvergentScript { .. } => ErrorKind::NonConvergentScript,
            Error::InvalidRenameMap { .. } => ErrorKind::InvalidRenameMap,
            Error::RenameMapDiverges { .. } => ErrorKind::RenameMapDiverges,
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

#[cfg(test)]
#[path = "error_tests.rs"]
mod tests;

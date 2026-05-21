use std::path::PathBuf;

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
}

pub type Result<T> = std::result::Result<T, Error>;

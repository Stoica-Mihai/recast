use std::path::PathBuf;

use rayon::prelude::*;

use crate::error::{Error, Result};
use crate::pattern::{CompiledPattern, PatternOptions};
use crate::plan::{check_match_counts, read_text_or_skip_binary};
use crate::walker::{WalkOptions, walk_paths};

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct SearchMatch {
    pub line: usize,
    pub column: usize,
    pub snippet: String,
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub capture: Option<String>,
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct SearchFile {
    pub path: PathBuf,
    pub matches: Vec<SearchMatch>,
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct SearchPlan {
    pub files: Vec<SearchFile>,
    pub total_matches: usize,
    pub files_scanned: usize,
}

// PatternOptions and WalkOptions are not Serialize; serde omitted for SearchOptions
#[derive(Debug, Clone)]
pub struct SearchOptions {
    pub pattern_options: PatternOptions,
    pub walk_options: WalkOptions,
    pub at_least: Option<usize>,
    pub at_most: Option<usize>,
    pub max_bytes: u64,
    pub max_files: usize,
}

impl Default for SearchOptions {
    fn default() -> Self {
        Self {
            pattern_options: PatternOptions::default(),
            walk_options: WalkOptions::default(),
            at_least: Some(1),
            at_most: None,
            max_bytes: 10 * 1024 * 1024,
            max_files: 1000,
        }
    }
}

// column is byte-based — consistent with tree-sitter's `start_position`
pub(crate) fn line_col(source: &str, byte_offset: usize) -> (usize, usize) {
    debug_assert!(source.is_char_boundary(byte_offset), "byte_offset must be on a char boundary");
    let prefix = &source[..byte_offset];
    let line = prefix.bytes().filter(|&b| b == b'\n').count() + 1;
    let col = match prefix.rfind('\n') {
        Some(nl) => byte_offset - nl,
        None => byte_offset + 1,
    };
    (line, col)
}

pub(crate) fn truncate_snippet(s: &str) -> String {
    let first_line = s.lines().next().unwrap_or("").trim();
    first_line.chars().take(200).collect()
}

pub fn plan_search<P: AsRef<std::path::Path>>(
    pattern: &str,
    roots: &[P],
    opts: &SearchOptions,
) -> Result<SearchPlan> {
    let compiled = CompiledPattern::compile(pattern, "", &opts.pattern_options)?;
    let files = scan(roots, opts)?;
    let files_scanned = files.len();

    let results: Vec<Result<Option<SearchFile>>> = files
        .par_iter()
        .map(|path| search_one(&compiled, path, opts))
        .collect();

    let found = collect(results)?;
    let total_matches: usize = found.iter().map(|f| f.matches.len()).sum();
    check_match_counts(total_matches, opts.at_least, opts.at_most)?;

    Ok(SearchPlan { files: found, total_matches, files_scanned })
}

pub(crate) fn scan<P: AsRef<std::path::Path>>(
    roots: &[P],
    opts: &SearchOptions,
) -> Result<Vec<PathBuf>> {
    let files = walk_paths(roots, &opts.walk_options)?;
    if files.len() > opts.max_files {
        return Err(Error::TooManyFiles { count: files.len(), limit: opts.max_files });
    }
    Ok(files)
}

pub(crate) fn collect(results: Vec<Result<Option<SearchFile>>>) -> Result<Vec<SearchFile>> {
    let mut out = Vec::new();
    for r in results {
        if let Some(f) = r? {
            out.push(f);
        }
    }
    Ok(out)
}

fn search_one(
    compiled: &CompiledPattern,
    path: &std::path::Path,
    opts: &SearchOptions,
) -> Result<Option<SearchFile>> {
    let (source, _) = match read_text_or_skip_binary(path, opts.max_bytes)? {
        Some(pair) => pair,
        None => return Ok(None),
    };

    let matches: Vec<SearchMatch> = compiled
        .regex()
        .find_iter(&source)
        .map(|m| {
            let (line, column) = line_col(&source, m.start());
            SearchMatch { line, column, snippet: truncate_snippet(m.as_str()), capture: None }
        })
        .collect();

    if matches.is_empty() {
        return Ok(None);
    }
    Ok(Some(SearchFile { path: path.to_path_buf(), matches }))
}

#[cfg(test)]
#[path = "search_tests.rs"]
mod tests;

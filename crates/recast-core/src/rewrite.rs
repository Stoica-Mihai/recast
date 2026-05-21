//! Per-file rewrite engine and unified-diff renderer.
//!
//! [`rewrite_text`] runs the compiled regex over a single in-memory
//! string and produces a [`RewriteOutcome`] with the match count and
//! the post-image. [`unified_diff`] turns a before/after pair into a
//! standard unified-diff string via the `similar` crate.
//! [`label_for_path`] cleans `./`-prefixed paths so diff headers stay
//! readable when the user passes `.` as the root.

use std::path::{Component, Path, PathBuf};

use similar::TextDiff;

#[cfg(feature = "script")]
use crate::error::Result;
use crate::pattern::CompiledPattern;
#[cfg(feature = "script")]
use crate::script::ScriptRewriter;

/// Post-image of a single-file rewrite plus the match count that produced
/// it. The pre-image stays with the caller — that's why it isn't carried
/// here.
#[derive(Debug, Clone)]
pub struct RewriteOutcome {
    pub after: String,
    pub matches: usize,
}

/// Apply `pattern` to `before` and return the rewrite outcome. Counts
/// matches and produces the new text in a single pass via
/// `regex::replace_all` with an `expand`-driven closure.
pub fn rewrite_text(pattern: &CompiledPattern, before: &str) -> RewriteOutcome {
    let regex = pattern.regex();
    let template = pattern.replacement();
    let mut matches = 0usize;
    let after = regex
        .replace_all(before, |caps: &regex::Captures<'_>| {
            matches += 1;
            let mut dst = String::new();
            caps.expand(template, &mut dst);
            dst
        })
        .into_owned();
    RewriteOutcome { after, matches }
}

/// Apply `pattern` to `before`, calling `script` once per match. The
/// script's return value replaces each occurrence. Script errors abort
/// the whole rewrite.
#[cfg(feature = "script")]
pub fn rewrite_text_scripted(
    pattern: &CompiledPattern,
    script: &ScriptRewriter,
    before: &str,
) -> Result<RewriteOutcome> {
    use std::cell::RefCell;

    let regex = pattern.regex();
    let err_slot: RefCell<Option<crate::error::Error>> = RefCell::new(None);
    let mut matches = 0usize;

    let after = regex.replace_all(before, |caps: &regex::Captures<'_>| {
        if err_slot.borrow().is_some() {
            return String::new();
        }
        matches += 1;
        let caps_vec: Vec<&str> =
            caps.iter().map(|m| m.map(|m| m.as_str()).unwrap_or("")).collect();
        match script.replace(&caps_vec) {
            Ok(s) => s,
            Err(e) => {
                *err_slot.borrow_mut() = Some(e);
                String::new()
            }
        }
    });

    if let Some(e) = err_slot.into_inner() {
        return Err(e);
    }
    Ok(RewriteOutcome { after: after.into_owned(), matches })
}

/// Drop leading `./` (and repeats thereof) from a path so unified-diff
/// headers read `a/src/a.rs` instead of `a/./src/a.rs`. Absolute paths
/// and plain relative paths pass through unchanged. On Windows the
/// separator is normalized to `/` so diff output is platform-agnostic.
pub fn label_for_path(path: &Path) -> String {
    let mut buf = PathBuf::new();
    let mut leading = true;
    for c in path.components() {
        if leading && matches!(c, Component::CurDir) {
            continue;
        }
        leading = false;
        buf.push(c.as_os_str());
    }
    if buf.as_os_str().is_empty() {
        ".".to_owned()
    } else {
        buf.to_string_lossy().replace('\\', "/")
    }
}

/// Render a unified diff between `before` and `after` with three lines
/// of context, using `label` for the `a/`+`b/` header paths.
pub fn unified_diff(label: &str, before: &str, after: &str) -> String {
    let diff = TextDiff::from_lines(before, after);
    let mut out = diff
        .unified_diff()
        .context_radius(3)
        .header(&format!("a/{label}"), &format!("b/{label}"))
        .to_string();
    if !out.ends_with('\n') {
        out.push('\n');
    }
    out
}

#[cfg(test)]
#[path = "rewrite_tests.rs"]
mod tests;

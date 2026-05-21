use std::path::{Component, Path, PathBuf};

use similar::TextDiff;

use crate::pattern::CompiledPattern;

#[derive(Debug, Clone)]
pub struct RewriteOutcome {
    pub before: String,
    pub after: String,
    pub matches: usize,
}

impl RewriteOutcome {
    pub fn changed(&self) -> bool {
        self.before != self.after
    }
}

pub fn rewrite_text(pattern: &CompiledPattern, before: &str) -> RewriteOutcome {
    let matches = pattern.regex().find_iter(before).count();
    let after = pattern.regex().replace_all(before, pattern.replacement()).into_owned();
    RewriteOutcome { before: before.to_owned(), after, matches }
}

/// Drop leading `./` (and repeats thereof) from a path so unified-diff
/// headers read `a/src/a.rs` instead of `a/./src/a.rs`. Absolute paths
/// and plain relative paths pass through unchanged.
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
    if buf.as_os_str().is_empty() { ".".to_owned() } else { buf.to_string_lossy().into_owned() }
}

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

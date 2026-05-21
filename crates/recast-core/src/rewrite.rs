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

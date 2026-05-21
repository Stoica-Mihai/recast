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
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use crate::pattern::{CompiledPattern, PatternOptions};

    #[test]
    fn rewrite_counts_matches_and_replaces() {
        let p = CompiledPattern::compile("foo", "bar", &PatternOptions::default()).unwrap();
        let r = rewrite_text(&p, "foo and foo");
        assert_eq!(r.matches, 2);
        assert_eq!(r.after, "bar and bar");
        assert!(r.changed());
    }

    #[test]
    fn rewrite_with_no_match_is_unchanged() {
        let p = CompiledPattern::compile("foo", "bar", &PatternOptions::default()).unwrap();
        let r = rewrite_text(&p, "nothing here");
        assert_eq!(r.matches, 0);
        assert!(!r.changed());
    }

    #[test]
    fn unified_diff_has_file_header_and_hunk() {
        let d = unified_diff("a.txt", "alpha\n", "beta\n");
        assert!(d.contains("--- a/a.txt"));
        assert!(d.contains("+++ b/a.txt"));
        assert!(d.contains("-alpha"));
        assert!(d.contains("+beta"));
    }
}

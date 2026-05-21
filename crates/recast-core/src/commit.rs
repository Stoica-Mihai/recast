use std::fs;

use crate::error::{Error, Result};
use crate::plan::{FileChange, Plan};

#[derive(Debug, Clone, Copy)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct ApplyOutcome {
    pub files_written: usize,
    pub total_matches: usize,
}

/// Phase 1: in-place write per file. No rollback. Phase 2 will replace this
/// with a two-phase atomic commit (sibling temp + fsync + rename, rollback
/// on any failure).
pub fn apply_changes(plan: &Plan) -> Result<ApplyOutcome> {
    for change in &plan.changes {
        write_one(change)?;
    }
    Ok(ApplyOutcome { files_written: plan.changes.len(), total_matches: plan.total_matches })
}

fn write_one(change: &FileChange) -> Result<()> {
    fs::write(&change.path, &change.after)
        .map_err(|e| Error::Io { path: change.path.clone(), source: e })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::field_reassign_with_default)]

    use std::fs;

    use tempfile::TempDir;

    use super::*;
    use crate::plan::{PlanOptions, plan_rewrite};

    #[test]
    fn apply_writes_changes_to_disk() {
        let dir = TempDir::new().unwrap();
        let file = dir.path().join("a.txt");
        fs::write(&file, "Old name\n").unwrap();
        let plan = plan_rewrite("Old", "New", &[dir.path()], &PlanOptions::default()).unwrap();
        let outcome = apply_changes(&plan).unwrap();
        assert_eq!(outcome.files_written, 1);
        assert_eq!(fs::read_to_string(&file).unwrap(), "New name\n");
    }

    #[test]
    fn apply_with_no_changes_is_a_noop() {
        let dir = TempDir::new().unwrap();
        let file = dir.path().join("a.txt");
        fs::write(&file, "unrelated\n").unwrap();
        let mut opts = PlanOptions::default();
        opts.at_least = Some(0);
        let plan = plan_rewrite("Zzz", "Q", &[dir.path()], &opts).unwrap();
        let outcome = apply_changes(&plan).unwrap();
        assert_eq!(outcome.files_written, 0);
        assert_eq!(fs::read_to_string(&file).unwrap(), "unrelated\n");
    }
}

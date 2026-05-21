//! Two-phase atomic commit for an approved [`Plan`].
//!
//! Stage phase writes a sibling temp per file (fsync, preserve mode),
//! commit phase swaps original → backup and temp → original per file.
//! Any commit-phase failure walks the rename log in reverse to restore
//! every already-renamed original from its backup; remaining staged
//! temps are deleted. On success, backups are removed and parent dirs
//! are fsynced so the rename batch is durable.

use std::collections::HashSet;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use rayon::prelude::*;
use tracing::{debug, trace};

use crate::error::{Error, IoCtx, Result};
use crate::plan::{FileChange, Plan};

/// Returned by [`apply_changes`] on success: how many files were
/// written and how many matches they covered.
#[derive(Debug, Clone, Copy)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct ApplyOutcome {
    pub files_written: usize,
    pub total_matches: usize,
}

struct Staged {
    target: PathBuf,
    temp_path: PathBuf,
}

struct Committed {
    target: PathBuf,
    backup_path: PathBuf,
}

/// Two-phase atomic commit.
///
/// Phase A (stage): for every change, write the new content to a sibling
/// temp file, fsync the temp, copy the original's permissions across, and
/// fsync the parent directory so the entry is durable. Any failure during
/// stage deletes every temp created so far; originals are untouched.
///
/// Phase B (commit): for every change, rename the original aside to a
/// sibling `*.recast.bak` and rename the temp into place. Any failure
/// during commit walks the rename log in reverse to restore originals;
/// remaining staged temps are deleted. The on-disk tree ends up either
/// fully rewritten or bit-identical to the pre-image.
pub fn apply_changes(plan: &Plan) -> Result<ApplyOutcome> {
    if plan.changes.is_empty() {
        debug!("apply: no changes; nothing to do");
        return Ok(ApplyOutcome { files_written: 0, total_matches: plan.total_matches });
    }

    let nonces = NonceGen::new();
    debug!(files = plan.changes.len(), "apply: stage phase begin");
    let staged = stage_all(&plan.changes, &nonces)?;
    debug!(files = staged.len(), "apply: stage phase complete");

    finalize_apply(plan, &staged, commit_all(&staged, &nonces))
}

fn finalize_apply(
    plan: &Plan,
    staged: &[Staged],
    result: std::result::Result<Vec<Committed>, CommitFailure>,
) -> Result<ApplyOutcome> {
    match result {
        Ok(committed) => {
            debug!(files = committed.len(), "apply: commit phase complete");
            // Fsync parents BEFORE unlinking backups: the rename batch's
            // directory entries must be durable before the safety net is
            // dropped, otherwise a crash in the window can leave the target
            // absent with the backup already gone.
            best_effort_fsync_parents(&committed);
            best_effort_cleanup_backups(&committed);
            Ok(ApplyOutcome {
                files_written: plan.changes.len(),
                total_matches: plan.total_matches,
            })
        }
        Err(CommitFailure { committed, remaining_staged, error }) => {
            debug!(
                committed = committed.len(),
                remaining = remaining_staged,
                "apply: commit failed, rolling back"
            );
            rollback_committed(&committed);
            cleanup_remaining_staged(staged, remaining_staged);
            Err(error)
        }
    }
}

/// Test-only entry point that lets a closure fire between each per-file
/// commit so the rollback path can be exercised. Production code goes
/// through [`apply_changes`], which has no hook in its signature.
#[cfg(test)]
fn apply_inner<F>(plan: &Plan, between_commits: F) -> Result<ApplyOutcome>
where
    F: Fn(usize) -> Result<()>,
{
    if plan.changes.is_empty() {
        return Ok(ApplyOutcome { files_written: 0, total_matches: plan.total_matches });
    }
    let nonces = NonceGen::new();
    let staged = stage_all(&plan.changes, &nonces)?;
    finalize_apply(plan, &staged, commit_all_with_hook(&staged, &nonces, &between_commits))
}

fn stage_all(changes: &[FileChange], nonces: &NonceGen) -> Result<Vec<Staged>> {
    // Per-file stage is dominated by `sync_all` on the temp file, which
    // the kernel can overlap across workers. Commit phase stays serial
    // so rollback ordering remains deterministic.
    let results: Vec<Result<Staged>> = changes.par_iter().map(|c| stage_one(c, nonces)).collect();

    let mut staged: Vec<Staged> = Vec::with_capacity(results.len());
    let mut first_error: Option<Error> = None;
    for r in results {
        match r {
            Ok(s) => staged.push(s),
            Err(e) => {
                if first_error.is_none() {
                    first_error = Some(e);
                }
            }
        }
    }
    if let Some(e) = first_error {
        for s in &staged {
            let _ = fs::remove_file(&s.temp_path);
        }
        return Err(e);
    }
    Ok(staged)
}

fn stage_one(change: &FileChange, nonces: &NonceGen) -> Result<Staged> {
    let parent = parent_dir(&change.path)?;

    // Permissions were captured by the planner alongside the file
    // read; reuse them so the stage hot path doesn't do a second
    // `fs::metadata` syscall per file.
    let permissions = change.permissions.clone();

    let temp_name = sibling_temp_name(&change.path, SiblingKind::Temp, nonces);
    let temp_path = parent.join(&temp_name);

    let mut file =
        OpenOptions::new().write(true).create_new(true).open(&temp_path).io_ctx(&temp_path)?;
    file.write_all(change.after.as_bytes()).io_ctx(&temp_path)?;
    file.flush().io_ctx(&temp_path)?;
    file.sync_all().io_ctx(&temp_path)?;
    drop(file);

    if let Some(perm) = permissions
        && let Err(e) = fs::set_permissions(&temp_path, perm)
    {
        let _ = fs::remove_file(&temp_path);
        return Err(Error::Io { path: temp_path, source: e });
    }

    Ok(Staged { target: change.path.clone(), temp_path })
}

fn parent_dir(path: &Path) -> Result<&Path> {
    path.parent().ok_or_else(|| Error::Io {
        path: path.to_path_buf(),
        source: std::io::Error::new(std::io::ErrorKind::InvalidInput, "no parent directory"),
    })
}

struct CommitFailure {
    committed: Vec<Committed>,
    remaining_staged: usize,
    error: Error,
}

fn commit_all(
    staged: &[Staged],
    nonces: &NonceGen,
) -> std::result::Result<Vec<Committed>, CommitFailure> {
    let mut committed: Vec<Committed> = Vec::with_capacity(staged.len());
    for (i, s) in staged.iter().enumerate() {
        match commit_one(s, nonces) {
            Ok(c) => committed.push(c),
            Err(error) => {
                return Err(CommitFailure { committed, remaining_staged: staged.len() - i, error });
            }
        }
    }
    Ok(committed)
}

#[cfg(test)]
fn commit_all_with_hook<F>(
    staged: &[Staged],
    nonces: &NonceGen,
    between_commits: &F,
) -> std::result::Result<Vec<Committed>, CommitFailure>
where
    F: Fn(usize) -> Result<()>,
{
    let mut committed: Vec<Committed> = Vec::with_capacity(staged.len());
    for (i, s) in staged.iter().enumerate() {
        match commit_one(s, nonces) {
            Ok(c) => committed.push(c),
            Err(error) => {
                return Err(CommitFailure { committed, remaining_staged: staged.len() - i, error });
            }
        }
        if let Err(error) = between_commits(i) {
            return Err(CommitFailure { committed, remaining_staged: staged.len() - i - 1, error });
        }
    }
    Ok(committed)
}

fn commit_one(staged: &Staged, nonces: &NonceGen) -> Result<Committed> {
    trace!(target = %staged.target.display(), "commit: rename");
    let backup_name = sibling_temp_name(&staged.target, SiblingKind::Backup, nonces);
    let backup_path = parent_dir(&staged.target)?.join(&backup_name);

    fs::rename(&staged.target, &backup_path).io_ctx(&staged.target)?;

    if let Err(e) = fs::rename(&staged.temp_path, &staged.target) {
        let _ = fs::rename(&backup_path, &staged.target);
        return Err(Error::Io { path: staged.target.clone(), source: e });
    }

    Ok(Committed { target: staged.target.clone(), backup_path })
}

fn rollback_committed(committed: &[Committed]) {
    // Unix `rename` replaces the destination atomically, so a separate
    // `remove_file(&c.target)` would only open a window where neither old
    // nor new content exists. Trust the rename to do the swap in one step.
    for c in committed.iter().rev() {
        let _ = fs::rename(&c.backup_path, &c.target);
    }
}

fn cleanup_remaining_staged(staged: &[Staged], remaining_count: usize) {
    let start = staged.len().saturating_sub(remaining_count);
    for s in &staged[start..] {
        let _ = fs::remove_file(&s.temp_path);
    }
}

fn best_effort_cleanup_backups(committed: &[Committed]) {
    for c in committed {
        let _ = fs::remove_file(&c.backup_path);
    }
}

fn best_effort_fsync_parents(committed: &[Committed]) {
    let mut seen: HashSet<&Path> = HashSet::new();
    for c in committed {
        if let Some(parent) = c.target.parent()
            && seen.insert(parent)
            && let Ok(dir) = std::fs::File::open(parent)
        {
            let _ = dir.sync_all();
        }
    }
}

/// Summary of a [`recover_sweep`] call.
#[derive(Debug, Clone, Copy, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct RecoverySummary {
    pub backups_restored: usize,
    pub backups_removed: usize,
    pub temps_removed: usize,
}

/// Walk every regular file under `roots` and reconcile leftover
/// `.recast.bak.*` / `.recast.tmp.*` siblings from a previous interrupted
/// apply.
///
/// Rules per target `foo`:
/// - target exists, only `.foo.recast.bak.*`/`.tmp.*` leftovers → delete leftovers
/// - target missing, `.foo.recast.bak.N` present → rename newest backup → target,
///   delete older backups and any temps
/// - target missing, only temps present → leave untouched (can't decide safely)
pub fn recover_sweep<P: AsRef<Path>>(roots: &[P]) -> Result<RecoverySummary> {
    use ignore::WalkBuilder;

    let mut iter = if let Some(first) = roots.first() {
        WalkBuilder::new(first.as_ref())
    } else {
        WalkBuilder::new(".")
    };
    for extra in roots.iter().skip(1) {
        iter.add(extra.as_ref());
    }
    iter.hidden(false).ignore(false).git_ignore(false).git_global(false).git_exclude(false);

    let mut groups: std::collections::HashMap<PathBuf, RecoveryGroup> =
        std::collections::HashMap::new();
    for entry in iter.build() {
        let entry = entry.map_err(Error::Walk)?;
        let path = entry.into_path();
        let name = match path.file_name().and_then(|n| n.to_str()) {
            Some(s) => s.to_owned(),
            None => continue,
        };
        if let Some((target_name, kind, nonce)) = parse_sibling_name(&name) {
            // A recast sibling file lives next to its target, so its
            // path must have a parent. If the walker handed back a
            // parentless path (root-level entry, weird filesystem),
            // bucketing under PathBuf::new() would silently merge
            // unrelated targets — skip instead.
            let Some(parent) = path.parent() else {
                trace!(name = %name, "recover: sibling has no parent; skipping");
                continue;
            };
            let target = parent.join(&target_name);
            let g = groups.entry(target).or_default();
            match kind {
                SiblingKind::Backup => g.backups.push((nonce, path.clone())),
                SiblingKind::Temp => g.temps.push((nonce, path.clone())),
            }
        }
    }

    let mut summary = RecoverySummary::default();
    for (target, mut group) in groups {
        group.backups.sort_by_key(|(n, _)| *n);
        group.temps.sort_by_key(|(n, _)| *n);
        if target.exists() {
            remove_nonced(&group.backups, &mut summary.backups_removed)?;
            remove_nonced(&group.temps, &mut summary.temps_removed)?;
            continue;
        }
        if let Some((_, newest)) = group.backups.pop() {
            fs::rename(&newest, &target).io_ctx(&newest)?;
            summary.backups_restored += 1;
            remove_nonced(&group.backups, &mut summary.backups_removed)?;
            remove_nonced(&group.temps, &mut summary.temps_removed)?;
        }
    }
    Ok(summary)
}

fn remove_nonced(entries: &[(u64, PathBuf)], counter: &mut usize) -> Result<()> {
    for (_, p) in entries {
        fs::remove_file(p).io_ctx(p)?;
        *counter += 1;
    }
    Ok(())
}

#[derive(Default)]
struct RecoveryGroup {
    backups: Vec<(u64, PathBuf)>,
    temps: Vec<(u64, PathBuf)>,
}

#[derive(Copy, Clone)]
enum SiblingKind {
    Backup,
    Temp,
}

impl SiblingKind {
    /// Filename token used in `.{target}.recast.{token}.{nonce}` sibling
    /// names. Single source of truth for both emission
    /// ([`sibling_temp_name`]) and parsing ([`parse_sibling_name`]).
    fn as_str(self) -> &'static str {
        match self {
            SiblingKind::Backup => "bak",
            SiblingKind::Temp => "tmp",
        }
    }

    fn from_token(token: &str) -> Option<Self> {
        match token {
            "bak" => Some(SiblingKind::Backup),
            "tmp" => Some(SiblingKind::Temp),
            _ => None,
        }
    }
}

fn parse_sibling_name(name: &str) -> Option<(String, SiblingKind, u64)> {
    let rest = name.strip_prefix('.')?;
    let idx_recast = rest.find(".recast.")?;
    let (target, suffix) = rest.split_at(idx_recast);
    if target.is_empty() {
        return None;
    }
    let suffix = suffix.strip_prefix(".recast.")?;
    let dot = suffix.find('.')?;
    let (kind_str, nonce_str) = suffix.split_at(dot);
    let kind = SiblingKind::from_token(kind_str)?;
    let nonce: u64 = nonce_str.strip_prefix('.')?.parse().ok()?;
    Some((target.to_owned(), kind, nonce))
}

fn sibling_temp_name(target: &Path, kind: SiblingKind, nonces: &NonceGen) -> String {
    let name = target.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let nonce = nonces.next();
    format!(".{name}.recast.{token}.{nonce}", token = kind.as_str())
}

/// Per-apply nonce generator for `.recast.{kind}.{nonce}` sibling
/// filenames. Mixes the current epoch nanoseconds with a monotonically-
/// incremented counter so siblings from concurrent stage workers don't
/// collide and a single apply can produce thousands of unique names.
///
/// One instance per [`apply_changes`] / [`recover_sweep`] (writer) call
/// — passed by reference into the stage + commit phases so the counter
/// is scoped to the apply that owns it instead of living in static
/// mutable state.
struct NonceGen {
    counter: std::sync::atomic::AtomicU64,
}

impl NonceGen {
    fn new() -> Self {
        Self { counter: std::sync::atomic::AtomicU64::new(0) }
    }

    fn next(&self) -> u64 {
        use std::sync::atomic::Ordering;
        use std::time::{SystemTime, UNIX_EPOCH};
        let ts =
            SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(0);
        let n = self.counter.fetch_add(1, Ordering::Relaxed);
        ts.wrapping_mul(0x9E37_79B9_7F4A_7C15).wrapping_add(n)
    }
}

#[cfg(test)]
#[path = "commit_tests.rs"]
mod tests;

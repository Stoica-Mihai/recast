//! recast core engine.
//!
//! Compiles a regex pattern, walks a set of paths honoring ignore rules,
//! produces per-file rewrites with unified-diff previews, enforces the
//! match-count guard, and runs an idempotency (convergence) check before
//! applying any change. The binary crate wires this engine to a CLI.

mod commit;
mod error;
#[cfg(feature = "serde")]
pub mod json;
mod lockfile;
mod parallel;
mod pattern;
mod plan;
#[cfg(test)]
mod proptests;
mod rewrite;
#[cfg(feature = "script")]
mod script;
#[cfg(any(
    feature = "lang-rust",
    feature = "lang-ts",
    feature = "lang-js",
    feature = "lang-python",
))]
mod structural;
mod walker;

pub use commit::{ApplyOutcome, RecoverySummary, apply_changes, recover_sweep};
pub use error::{Error, Result};
pub use lockfile::{WorkspaceLock, acquire_workspace_lock};
pub use parallel::build_pool;
pub use pattern::{CompiledPattern, PatternOptions};
#[cfg(feature = "script")]
pub use plan::plan_rewrite_scripted;
pub use plan::{FileChange, Plan, PlanOptions, PlanOutcome, plan_rewrite};
#[cfg(feature = "script")]
pub use rewrite::rewrite_text_scripted;
pub use rewrite::{RewriteOutcome, label_for_path, rewrite_text, unified_diff};
#[cfg(feature = "script")]
pub use script::ScriptRewriter;
#[cfg(any(
    feature = "lang-rust",
    feature = "lang-ts",
    feature = "lang-js",
    feature = "lang-python",
))]
pub use structural::{
    Language, StructuralOutcome, compile_friendly_query, structural_rewrite,
    structural_rewrite_friendly,
};
pub use walker::{WalkOptions, walk_paths};

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
mod parallel;
mod pattern;
mod plan;
mod rewrite;
#[cfg(feature = "script")]
mod script;
#[cfg(feature = "structural")]
mod structural;
mod walker;

pub use commit::{ApplyOutcome, RecoverySummary, apply_changes, recover_sweep};
pub use error::{Error, Result};
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
#[cfg(feature = "structural")]
pub use structural::{Language, StructuralOutcome, structural_rewrite};
pub use walker::{WalkOptions, walk_paths};

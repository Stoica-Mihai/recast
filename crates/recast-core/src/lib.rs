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
mod pattern;
mod plan;
mod rewrite;
mod walker;

pub use commit::{ApplyOutcome, apply_changes};
pub use error::{Error, Result};
pub use pattern::{CompiledPattern, PatternOptions};
pub use plan::{FileChange, Plan, PlanOptions, PlanOutcome, plan_rewrite};
pub use rewrite::{RewriteOutcome, rewrite_text, unified_diff};
pub use walker::{WalkOptions, walk_paths};

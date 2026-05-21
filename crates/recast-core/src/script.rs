//! Rhai-backed scripted replacement (feature `script`).
//!
//! Compiles a single Rhai script and evaluates it once per regex match.
//! The script sees `captures` (array of strings; index 0 is the full
//! match) and `whole` (the full match as a convenience — `match` is a
//! Rhai reserved keyword). Its return value, coerced to a string,
//! becomes the replacement.

use std::fs;
use std::path::Path;

use rhai::{AST, Array, Dynamic, Engine, Scope};

use crate::error::{Error, IoCtx, Result};

/// Pre-compiled Rhai script used as a per-match replacement callback.
pub struct ScriptRewriter {
    engine: Engine,
    ast: AST,
}

impl std::fmt::Debug for ScriptRewriter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ScriptRewriter").finish_non_exhaustive()
    }
}

impl ScriptRewriter {
    /// Compile `source` directly. Returns [`Error::ScriptParse`] on
    /// syntax errors.
    pub fn from_source(source: &str) -> Result<Self> {
        let engine = sandboxed_engine();
        let ast = engine.compile(source).map_err(|e| Error::ScriptParse(e.to_string()))?;
        Ok(Self { engine, ast })
    }

    /// Read the script from `path` and compile it.
    pub fn from_file(path: &Path) -> Result<Self> {
        let source = fs::read_to_string(path).io_ctx(path)?;
        Self::from_source(&source)
    }

    /// Build a sibling rewriter that shares the compiled AST with `self`
    /// but owns a fresh sandboxed engine. Rhai `Engine` is `!Sync`, so
    /// parallel pipelines that want to evaluate the same script on
    /// multiple worker threads call `fresh()` per worker (e.g. via
    /// `rayon::par_iter().map_init(|| script.fresh(), ...)`).
    pub fn fresh(&self) -> Self {
        Self { engine: sandboxed_engine(), ast: self.ast.clone() }
    }

    /// Evaluate the script with `captures` (index 0 = full match) and
    /// return the resulting replacement string.
    pub fn replace(&self, captures: &[&str]) -> Result<String> {
        let mut scope = Scope::new();
        let arr: Array = captures.iter().map(|s| Dynamic::from((*s).to_string())).collect();
        scope.push("captures", arr);
        let full = captures.first().copied().unwrap_or("").to_string();
        scope.push("whole", full);
        let out: Dynamic = self
            .engine
            .eval_ast_with_scope(&mut scope, &self.ast)
            .map_err(|e| Error::ScriptRuntime(e.to_string()))?;
        Ok(out.to_string())
    }
}

fn sandboxed_engine() -> Engine {
    let mut engine = Engine::new();
    // CPU sandbox: rough cap so a runaway loop in a user script doesn't
    // wedge the planner.
    engine.set_max_operations(1_000_000);
    engine.set_max_string_size(1024 * 1024);
    engine.set_max_array_size(1024);
    engine.set_max_expr_depths(64, 64);
    engine
}

#[cfg(test)]
#[path = "script_tests.rs"]
mod tests;

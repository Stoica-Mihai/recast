//! MCP tool surface for the recast engine.
//!
//! Four tools, 1:1 with the planner API:
//! - `recast_preview` — dry-run a regex rewrite, return the per-file plan.
//! - `recast_apply` — atomically apply a regex rewrite to disk.
//! - `recast_structural` — same shape via tree-sitter `--ast` patterns.
//! - `recast_recover` — sweep leftover `.recast.bak.*` / `.tmp.*` siblings.
//!
//! Each tool wraps the corresponding `recast-core` entry point directly
//! — no subprocess, no JSON parse round-trip. Engine errors are
//! converted to `McpError` with the original `kind` and message
//! preserved so callers can branch on guard violations vs. IO errors
//! vs. parse failures without string matching.

use std::path::PathBuf;

use recast_core::{
    Language, PatternOptions, PlanOptions, RecoverySummary, WalkOptions, apply_changes, json,
    plan_rewrite, plan_structural_rewrite, recover_sweep,
};
use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{
    CallToolResult, Content, Implementation, ProtocolVersion, ServerCapabilities, ServerInfo,
};
use rmcp::schemars::JsonSchema;
use rmcp::{ErrorData as McpError, ServerHandler, tool, tool_handler, tool_router};
use serde::{Deserialize, Serialize};

const DEFAULT_MAX_BYTES: u64 = 10 * 1024 * 1024;
const DEFAULT_MAX_FILES: usize = 1000;

/// Server handle. Stateless — every tool call constructs its own plan
/// from the supplied arguments. `tool_router` is consumed by the
/// `#[tool_handler]` macro at the trait impl below; rustc can't see
/// that use because it's macro-generated.
#[derive(Clone)]
pub struct RecastServer {
    #[allow(dead_code)]
    tool_router: ToolRouter<RecastServer>,
}

impl RecastServer {
    pub fn new() -> Self {
        Self { tool_router: Self::tool_router() }
    }
}

#[tool_router]
impl RecastServer {
    #[tool(description = "Preview a multi-file regex rewrite without writing anything to disk. \
                       Returns a per-file plan with match counts and rendered unified diffs. \
                       The `--at-least 1` guard fires by default so silent zero-match runs \
                       become errors. Use this before `recast_apply` to verify the pattern \
                       does what you expect.")]
    async fn recast_preview(
        &self,
        Parameters(args): Parameters<RewriteArgs>,
    ) -> Result<CallToolResult, McpError> {
        let opts = args.plan_options()?;
        let paths = args.paths_as_pathbufs();
        let plan =
            plan_rewrite(&args.pattern, &args.replacement, &paths, &opts).map_err(to_mcp_err)?;
        Ok(CallToolResult::success(vec![Content::json(json::from_plan(&plan))?]))
    }

    #[tool(description = "Atomically apply a multi-file regex rewrite to disk. Two-phase commit \
                       with rollback: every change is staged as a sibling temp + fsync, then \
                       renamed into place; any per-file failure restores every already-renamed \
                       original from its backup. Surfaces non-convergent patterns (e.g. \
                       `a` -> `aa`) before any write. Use `recast_preview` first to inspect \
                       the diff.")]
    async fn recast_apply(
        &self,
        Parameters(args): Parameters<RewriteArgs>,
    ) -> Result<CallToolResult, McpError> {
        let opts = args.plan_options()?;
        let paths = args.paths_as_pathbufs();
        let plan =
            plan_rewrite(&args.pattern, &args.replacement, &paths, &opts).map_err(to_mcp_err)?;
        let outcome = apply_changes(&plan).map_err(to_mcp_err)?;
        Ok(CallToolResult::success(vec![Content::json(json::from_apply(&plan, &outcome))?]))
    }

    #[tool(description = "Structural rewrite via tree-sitter `--ast` pattern. Match and replace \
                       AST nodes in a target language using friendly placeholders \
                       (`$NAME`, `$$$BODY`) instead of regex. Pass `apply: true` to write \
                       changes; default is dry-run. Supported langs: rust, ts, tsx, js, \
                       python, bash, go, json, markdown.")]
    async fn recast_structural(
        &self,
        Parameters(args): Parameters<StructuralArgs>,
    ) -> Result<CallToolResult, McpError> {
        let opts = args.plan_options()?;
        let paths = args.paths_as_pathbufs();
        let lang = Language::from_name(&args.lang).map_err(to_mcp_err)?;
        let plan = plan_structural_rewrite(lang, &args.query, &args.template, &paths, &opts)
            .map_err(to_mcp_err)?;
        if args.apply {
            let outcome = apply_changes(&plan).map_err(to_mcp_err)?;
            Ok(CallToolResult::success(vec![Content::json(json::from_apply(&plan, &outcome))?]))
        } else {
            Ok(CallToolResult::success(vec![Content::json(json::from_plan(&plan))?]))
        }
    }

    #[tool(description = "Reconcile leftover `.recast.bak.*` / `.recast.tmp.*` siblings from \
                       an interrupted `recast_apply` (panic, signal, power loss). Restores \
                       from backup when the target is missing; deletes stale temps and \
                       backups when the target is present. Idempotent — safe to run \
                       proactively after any failed apply.")]
    async fn recast_recover(
        &self,
        Parameters(args): Parameters<RecoverArgs>,
    ) -> Result<CallToolResult, McpError> {
        let paths: Vec<PathBuf> = args.paths.iter().map(PathBuf::from).collect();
        let summary: RecoverySummary = recover_sweep(&paths).map_err(to_mcp_err)?;
        Ok(CallToolResult::success(vec![Content::json(summary)?]))
    }
}

#[tool_handler]
impl ServerHandler for RecastServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::from_build_env())
            .with_protocol_version(ProtocolVersion::V_2025_03_26)
            .with_instructions(
                "Safe, atomic, multi-file rewrites. Prefer `recast_preview` -> inspect diff -> \
                 `recast_apply` over hand-rolled write_file loops or sed; the engine catches \
                 silent zero-match runs, refuses non-convergent patterns, and rolls back \
                 mid-commit failures atomically."
                    .to_owned(),
            )
    }
}

/// Arguments shared by `recast_preview` and `recast_apply`.
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct RewriteArgs {
    /// Regex pattern to match. Multi-line by default; `.` matches `\n`.
    pub pattern: String,
    /// Replacement template. `$1`, `${name}` interpolated unless `literal: true`.
    pub replacement: String,
    /// Paths or globs to scan. Defaults to `["."]` if omitted.
    #[serde(default = "default_paths")]
    pub paths: Vec<String>,
    /// Treat pattern and replacement as literal strings (no regex metas).
    #[serde(default)]
    pub literal: bool,
    /// Case-insensitive matching.
    #[serde(default)]
    pub ignore_case: bool,
    /// Disable implicit `(?s)` so `.` no longer matches `\n`.
    #[serde(default)]
    pub single_line: bool,
    /// Include hidden files in the walk.
    #[serde(default)]
    pub hidden: bool,
    /// Disable `.gitignore` filtering.
    #[serde(default)]
    pub no_ignore: bool,
    /// Follow symlinks. Off by default for safety.
    #[serde(default)]
    pub follow_symlinks: bool,
    /// Ripgrep `--type` filter (e.g. `["rust", "ts"]`).
    #[serde(default)]
    pub types: Vec<String>,
    /// Ripgrep `--type-not` filter.
    #[serde(default)]
    pub types_not: Vec<String>,
    /// Ripgrep glob include/exclude (e.g. `["!vendor/**"]`).
    #[serde(default)]
    pub globs: Vec<String>,
    /// Require at least N total matches. Defaults to 1 — set to 0 to
    /// allow zero-match runs explicitly.
    #[serde(default = "default_at_least")]
    pub at_least: Option<usize>,
    /// Require at most N total matches. None = unbounded.
    #[serde(default)]
    pub at_most: Option<usize>,
    /// Skip the convergence (idempotency) check.
    #[serde(default)]
    pub allow_non_convergent: bool,
    /// Refuse files larger than this many bytes. Defaults to 10 MiB.
    #[serde(default = "default_max_bytes")]
    pub max_bytes: u64,
    /// Refuse runs touching more than this many files. Defaults to 1000.
    #[serde(default = "default_max_files")]
    pub max_files: usize,
}

/// Arguments for `recast_structural`.
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct StructuralArgs {
    /// Target language (rust, ts, tsx, js, python, bash, go, json, markdown).
    pub lang: String,
    /// Tree-sitter S-expression query, OR a friendly `--ast` pattern
    /// compiled by the caller. The MCP server expects the already-compiled
    /// query form; use `recast-core::compile_friendly_query` upstream
    /// to translate `$NAME` patterns first.
    pub query: String,
    /// Rewrite template; captures referenced as `$name` / `${name}`.
    pub template: String,
    /// Paths or globs to scan.
    #[serde(default = "default_paths")]
    pub paths: Vec<String>,
    /// Apply changes atomically. Default false = dry-run preview only.
    #[serde(default)]
    pub apply: bool,
    /// Shared filters (mirrors RewriteArgs).
    #[serde(default)]
    pub hidden: bool,
    #[serde(default)]
    pub no_ignore: bool,
    #[serde(default)]
    pub follow_symlinks: bool,
    #[serde(default)]
    pub types: Vec<String>,
    #[serde(default)]
    pub types_not: Vec<String>,
    #[serde(default)]
    pub globs: Vec<String>,
    #[serde(default = "default_at_least")]
    pub at_least: Option<usize>,
    #[serde(default)]
    pub at_most: Option<usize>,
    #[serde(default = "default_max_bytes")]
    pub max_bytes: u64,
    #[serde(default = "default_max_files")]
    pub max_files: usize,
}

/// Arguments for `recast_recover`.
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct RecoverArgs {
    /// Paths to sweep for leftover `.recast.bak.*` / `.tmp.*` siblings.
    #[serde(default = "default_paths")]
    pub paths: Vec<String>,
}

fn default_paths() -> Vec<String> {
    vec![".".to_owned()]
}

fn default_at_least() -> Option<usize> {
    Some(1)
}

fn default_max_bytes() -> u64 {
    DEFAULT_MAX_BYTES
}

fn default_max_files() -> usize {
    DEFAULT_MAX_FILES
}

impl RewriteArgs {
    fn paths_as_pathbufs(&self) -> Vec<PathBuf> {
        self.paths.iter().map(PathBuf::from).collect()
    }

    fn plan_options(&self) -> Result<PlanOptions, McpError> {
        Ok(PlanOptions {
            pattern_options: PatternOptions {
                literal: self.literal,
                ignore_case: self.ignore_case,
                single_line: self.single_line,
            },
            walk_options: walk_options_from(
                self.hidden,
                self.no_ignore,
                self.follow_symlinks,
                &self.types,
                &self.types_not,
                &self.globs,
            ),
            at_least: self.at_least,
            at_most: self.at_most,
            allow_non_convergent: self.allow_non_convergent,
            max_bytes: self.max_bytes,
            max_files: self.max_files,
        })
    }
}

impl StructuralArgs {
    fn paths_as_pathbufs(&self) -> Vec<PathBuf> {
        self.paths.iter().map(PathBuf::from).collect()
    }

    fn plan_options(&self) -> Result<PlanOptions, McpError> {
        Ok(PlanOptions {
            pattern_options: PatternOptions::default(),
            walk_options: walk_options_from(
                self.hidden,
                self.no_ignore,
                self.follow_symlinks,
                &self.types,
                &self.types_not,
                &self.globs,
            ),
            at_least: self.at_least,
            at_most: self.at_most,
            allow_non_convergent: true,
            max_bytes: self.max_bytes,
            max_files: self.max_files,
        })
    }
}

fn walk_options_from(
    hidden: bool,
    no_ignore: bool,
    follow_symlinks: bool,
    types: &[String],
    types_not: &[String],
    globs: &[String],
) -> WalkOptions {
    WalkOptions {
        hidden,
        no_ignore,
        follow_symlinks,
        types: types.to_vec(),
        types_not: types_not.to_vec(),
        globs: globs.to_vec(),
    }
}

/// Convert a `recast-core::Error` into an `McpError` while preserving
/// the typed kind so callers can branch on the error variant without
/// string-matching. The MCP error payload carries `{kind, message}` —
/// agents can dispatch on `kind` programmatically.
fn to_mcp_err(err: recast_core::Error) -> McpError {
    let kind = err.kind();
    let kind_str = serde_json::to_value(kind)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_else(|| "internal".to_owned());
    McpError::invalid_params(
        format!("recast: {err}"),
        Some(serde_json::json!({ "kind": kind_str })),
    )
}

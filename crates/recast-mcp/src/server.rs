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
    Language, PatternOptions, Plan, PlanOptions, RecoverySummary, ScriptRewriter, WalkOptions,
    apply_changes, compile_friendly_query, json, plan_rewrite, plan_rewrite_scripted,
    plan_structural_rewrite, recover_sweep,
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
    #[tool(description = "Dry-run a multi-file regex/literal rewrite. Returns the per-file plan \
                       (paths, match counts, unified diffs) without touching disk. Always call \
                       this before `recast_apply` for any non-trivial change.\n\
                       \n\
                       WHEN TO USE:\n\
                       - Same simple text change in 5+ files (rename, version bump, import \
                       path migration). Below that, `Edit` is usually faster.\n\
                       - ANY shape-sensitive change (struct literal, enum variant, fn \
                       signature) — use `recast_structural` instead, but the decision is the \
                       same: regex on AST shapes is fragile.\n\
                       - Atomicity required (the change must not half-apply on failure).\n\
                       \n\
                       WHEN NOT TO USE: 1-4 isolated edits where you can see all callsites \
                       and the change is simple text. The escape-encoding round-trip and \
                       preview-then-apply two-step costs more than 4 `Edit` calls would.\n\
                       \n\
                       FOOTGUN — `replacement` is NOT escape-decoded. `\\n` in the JSON \
                       value becomes literal backslash-n on disk, NOT a newline. To insert a \
                       newline, put a real LF in the JSON string value (multiline JSON \
                       string), not the `\\n` escape. Same for `\\t`. Backreferences `$1` / \
                       `${name}` ARE interpolated.\n\
                       \n\
                       EXAMPLES:\n\
                       Rename `OldName` → `NewName` across src/ (literal):\n\
                       \x20  { \"pattern\":\"OldName\", \"replacement\":\"NewName\",\n\
                       \x20    \"paths\":[\"src/\"], \"literal\":true }\n\
                       \n\
                       Insert `, slot` as a second arg of `pane_title(...)` everywhere:\n\
                       \x20  { \"pattern\":\"pane_title\\\\(([^)]+)\\\\)\",\n\
                       \x20    \"replacement\":\"pane_title($1, slot)\" }\n\
                       \n\
                       Bump every version triplet via Rhai callback:\n\
                       \x20  { \"pattern\":\"\\\\d+\\\\.\\\\d+\\\\.\\\\d+\",\n\
                       \x20    \"script_source\":\"let v = whole.split('.'); v[2] = (parse_int(v[2])+1).to_string(); v.reduce(|a,b| a+\\\".\\\"+b)\",\n\
                       \x20    \"replacement\":\"\" }\n\
                       \n\
                       For shape-sensitive rewrites (struct literals, enum variants, fn \
                       signatures) use `recast_structural` instead — regex on AST shapes is \
                       fragile.")]
    async fn recast_preview(
        &self,
        Parameters(args): Parameters<RewriteArgs>,
    ) -> Result<CallToolResult, McpError> {
        let plan = plan_for(&args)?;
        Ok(CallToolResult::success(vec![Content::json(json::from_plan(&plan))?]))
    }

    #[tool(description = "Atomically apply a multi-file regex/literal rewrite to disk. \
                       Same argument shape as `recast_preview`; call preview first to inspect \
                       the diff, then call this to land it.\n\
                       \n\
                       WHEN TO USE:\n\
                       - Same simple text change in 5+ files (rename, version bump, import \
                       path migration). Below that, `Edit` is usually faster.\n\
                       - ANY shape-sensitive change → reach for `recast_structural`.\n\
                       - Atomicity required (the change must not half-apply).\n\
                       \n\
                       WHEN NOT TO USE: 1-4 isolated edits where `Edit` with enough context \
                       fits in the same number of calls and avoids the escape-encoding \
                       round-trip.\n\
                       \n\
                       FOOTGUN — `replacement` is NOT escape-decoded. `\\n` in the JSON \
                       value becomes literal backslash-n on disk, NOT a newline. To insert \
                       a newline, put a real LF in the JSON string value, not the `\\n` \
                       escape. Same for `\\t`. Backreferences `$1` / `${name}` ARE \
                       interpolated.\n\
                       \n\
                       SAFETY (already enforced — no extra flags needed):\n\
                       - Two-phase commit: every file staged as sibling temp + fsync, then \
                       renamed into place. Any per-file failure rolls back every \
                       already-renamed original from its backup. End state is either fully \
                       rewritten or bit-identical to the pre-image.\n\
                       - Convergence check: refuses patterns whose replacement still matches \
                       (e.g. `a` → `aa`) so re-runs can't corrupt the tree.\n\
                       - Match-count guard: default `at_least=1` turns silent zero-match runs \
                       into typed errors.\n\
                       - Syntax-regression guard: for files whose extension maps to a \
                       tree-sitter grammar (rust, ts, tsx, js, py, sh, go, json, md), a \
                       rewrite whose output introduces NEW parse errors is rejected. Catches \
                       greedy regex that strands a brace or truncates an expression. Note: \
                       it is syntactic only — it will NOT catch an orphaned `#[test]` left on \
                       the wrong item (that parses clean; use `recast_structural` for \
                       shape-sensitive deletes). Pass `allow_syntax_errors=true` to override.\n\
                       - Crash recovery: if killed mid-apply, `recast_recover` restores from \
                       backup.\n\
                       \n\
                       USE INSTEAD OF: Edit-tool loops, sed -i across files, write_file \
                       rewriting whole files for a one-token change, hand-rolled find+replace \
                       — once the site count + atomicity needs justify it (see WHEN TO USE).\n\
                       \n\
                       EXAMPLES: see `recast_preview` description — args are identical.")]
    async fn recast_apply(
        &self,
        Parameters(args): Parameters<RewriteArgs>,
    ) -> Result<CallToolResult, McpError> {
        let plan = plan_for(&args)?;
        let outcome = apply_changes(&plan).map_err(to_mcp_err)?;
        Ok(CallToolResult::success(vec![Content::json(json::from_apply(&plan, &outcome))?]))
    }

    #[tool(description = "AST-aware multi-file rewrite via tree-sitter. Use whenever the \
                       change is shape-sensitive — adding a field to every enum variant, \
                       renaming a function and all call sites, swapping arg order, reshaping \
                       struct literals. Regex on AST shapes is fragile; this is not.\n\
                       \n\
                       SUPPORTED LANGS: rust, ts, tsx, js, python, bash, go, json, markdown.\n\
                       \n\
                       PATTERN SYNTAX (`ast_pattern`): write the match in normal \
                       target-language source with `$NAME` for single-node placeholders and \
                       `$$$NAME` for variadic subtree placeholders. The engine compiles it \
                       to a tree-sitter Query. `template` is the rewrite; reference captures \
                       as `$NAME` / `${NAME}`.\n\
                       \n\
                       EXAMPLES:\n\
                       Rename every 0-arg fn `foo()` to `foo_v2()`:\n\
                       \x20  { \"lang\":\"rust\", \"ast_pattern\":\"fn $NAME() {}\",\n\
                       \x20    \"template\":\"fn ${NAME}_v2() {}\", \"apply\":true }\n\
                       \n\
                       Add `direction: None` field to every `ClientMessage::SplitPane { ... }` \
                       literal:\n\
                       \x20  { \"lang\":\"rust\",\n\
                       \x20    \"ast_pattern\":\"ClientMessage::SplitPane { $$$REST }\",\n\
                       \x20    \"template\":\"ClientMessage::SplitPane { direction: None, $REST }\",\n\
                       \x20    \"apply\":true }\n\
                       \n\
                       Default `apply:false` returns a dry-run plan. Exactly one of \
                       `query` (raw S-expression) or `ast_pattern` (friendly form) is required.")]
    async fn recast_structural(
        &self,
        Parameters(args): Parameters<StructuralArgs>,
    ) -> Result<CallToolResult, McpError> {
        let opts = args.plan_options()?;
        let paths = args.paths_as_pathbufs();
        let lang = Language::from_name(&args.lang).map_err(to_mcp_err)?;
        let query = match (&args.query, &args.ast_pattern) {
            (Some(_), Some(_)) => {
                return Err(invalid_args("supply either `query` or `ast_pattern`, not both"));
            }
            (Some(q), None) => q.clone(),
            (None, Some(pat)) => compile_friendly_query(lang, pat).map_err(to_mcp_err)?,
            (None, None) => {
                return Err(invalid_args("one of `query` or `ast_pattern` is required"));
            }
        };
        let plan = plan_structural_rewrite(lang, &query, &args.template, &paths, &opts)
            .map_err(to_mcp_err)?;
        if args.apply {
            let outcome = apply_changes(&plan).map_err(to_mcp_err)?;
            Ok(CallToolResult::success(vec![Content::json(json::from_apply(&plan, &outcome))?]))
        } else {
            Ok(CallToolResult::success(vec![Content::json(json::from_plan(&plan))?]))
        }
    }

    #[tool(description = "Reconcile leftover `.recast.bak.*` / `.recast.tmp.*` sibling files \
                       from an interrupted `recast_apply` (panic, signal, power loss, OOM). \
                       Restores from backup when the target is missing; deletes stale temps \
                       and backups when the target is present. Idempotent — safe to run \
                       proactively after any failed apply.\n\
                       \n\
                       WHEN TO USE: only after a previous `recast_apply` was killed mid-run, \
                       or when you see `.recast.bak.*` / `.recast.tmp.*` files lingering in \
                       the tree. Normal applies clean up after themselves; you don't need to \
                       call this routinely.")]
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
                "Safe, atomic, multi-file rewrites. Read this in full — the tool \
                 descriptions are short, the decision rule is here.\n\
                 \n\
                 DECISION RULE: If you're about to make the same syntactic change in 3+ \
                 files (renames, struct-field additions, fn-signature changes, enum-variant \
                 reshapes, version bumps), call `recast_preview` FIRST. Do NOT default to \
                 Edit / write_file loops or sed for repeated transforms — those silently \
                 fail on zero matches, can't roll back mid-failure, and blast-radius across \
                 unintended sites.\n\
                 \n\
                 TWO-STEP WORKFLOW:\n\
                 1. `recast_preview` → inspect the per-file plan + diffs.\n\
                 2. If the plan looks right, call `recast_apply` with identical args.\n\
                 If `recast_preview` returns 0 matches, the pattern is wrong — iterate the \
                 pattern, do NOT fall back to per-file Edit.\n\
                 \n\
                 TOOL PICK:\n\
                 - `recast_apply` (regex / literal / Rhai script) for text-level rewrites \
                 anywhere — works on any language.\n\
                 - `recast_structural` (tree-sitter `ast_pattern`) when the change is \
                 shape-sensitive (struct literals, enum variants, fn signatures, AST node \
                 reshapes). Supported langs: rust, ts, tsx, js, python, bash, go, json, \
                 markdown.\n\
                 - `recast_recover` only after a prior `recast_apply` was killed mid-run.\n\
                 \n\
                 SAFETY ALREADY ON: atomic two-phase commit with rollback on per-file \
                 failure, convergence check (refuses `a` → `aa`), `at_least=1` guard \
                 (refuses silent zero-match runs), syntax-regression guard (refuses rewrites \
                 that introduce new tree-sitter parse errors; `allow_syntax_errors` to \
                 override), crash-recovery sweep, workspace lock."
                    .to_owned(),
            )
    }
}

/// Arguments shared by `recast_preview` and `recast_apply`.
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct RewriteArgs {
    /// Regex pattern to match. Multi-line by default; `.` matches `\n`.
    pub pattern: String,
    /// Replacement template. `$1`, `${name}` interpolated unless
    /// `literal: true`. Ignored when `script_source` or `script_path`
    /// is set — pass `""` in that case.
    #[serde(default)]
    pub replacement: String,
    /// Inline Rhai script source. Mutually exclusive with
    /// `script_path`. When set, the script runs per regex match and
    /// its return value is the replacement; `replacement` is ignored.
    /// Script sees `captures` (array; index 0 is the full match) and
    /// `whole` (full match alias).
    #[serde(default)]
    pub script_source: Option<String>,
    /// Path to a Rhai script file. Mutually exclusive with
    /// `script_source`. Same semantics — return value per match
    /// becomes the replacement.
    #[serde(default)]
    pub script_path: Option<PathBuf>,
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
    /// Skip the syntax-regression guard. By default a rewrite whose
    /// output introduces new tree-sitter parse errors (in a file whose
    /// extension maps to a compiled grammar) is rejected.
    #[serde(default)]
    pub allow_syntax_errors: bool,
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
    /// Tree-sitter S-expression query. Mutually exclusive with
    /// `ast_pattern`; exactly one is required.
    #[serde(default)]
    pub query: Option<String>,
    /// Friendly `--ast` pattern in target-language source with `$NAME`
    /// / `$$$NAME` placeholders. Compiled to a tree-sitter Query by
    /// the engine. Mutually exclusive with `query`; exactly one is
    /// required.
    #[serde(default)]
    pub ast_pattern: Option<String>,
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
    /// Skip the syntax-regression guard (see RewriteArgs).
    #[serde(default)]
    pub allow_syntax_errors: bool,
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

/// Run the planner for a `RewriteArgs` call, picking the regex or
/// scripted variant based on whether a Rhai script was supplied.
/// Both `recast_preview` and `recast_apply` route through here so the
/// script-vs-template branching lives in one place.
fn plan_for(args: &RewriteArgs) -> Result<Plan, McpError> {
    let opts = args.plan_options()?;
    let paths = args.paths_as_pathbufs();
    let script = match (&args.script_source, &args.script_path) {
        (Some(_), Some(_)) => {
            return Err(invalid_args("supply either `script_source` or `script_path`, not both"));
        }
        (Some(src), None) => Some(ScriptRewriter::from_source(src).map_err(to_mcp_err)?),
        (None, Some(path)) => Some(ScriptRewriter::from_file(path).map_err(to_mcp_err)?),
        (None, None) => None,
    };
    match script {
        Some(s) => plan_rewrite_scripted(&args.pattern, &s, &paths, &opts).map_err(to_mcp_err),
        None => plan_rewrite(&args.pattern, &args.replacement, &paths, &opts).map_err(to_mcp_err),
    }
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
            allow_syntax_errors: self.allow_syntax_errors,
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
            allow_syntax_errors: self.allow_syntax_errors,
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

#[cfg(test)]
#[path = "server_tests.rs"]
mod tests;

/// MCP error for caller-side mistakes (mutual-exclusion violation,
/// missing required field). Distinguishes "agent sent bad args" from
/// "engine returned a typed error" so the agent's recovery branches
/// stay separable.
fn invalid_args(msg: &str) -> McpError {
    McpError::invalid_params(msg.to_owned(), None)
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

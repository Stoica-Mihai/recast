//! In-process unit tests for the MCP tool handlers. Calls each tool
//! method directly with constructed `Parameters<T>` values — no
//! JSON-RPC framing, no transport, no subprocess. Faster than spinning
//! up the stdio server and exercises the same code paths the live
//! handler does.

#![allow(clippy::unwrap_used)]

use std::fs;

use rmcp::handler::server::wrapper::Parameters;
use tempfile::TempDir;

use super::*;

fn server() -> RecastServer {
    RecastServer::new()
}

fn rewrite_args(pattern: &str, replacement: &str, path: &std::path::Path) -> RewriteArgs {
    RewriteArgs {
        pattern: pattern.to_owned(),
        replacement: replacement.to_owned(),
        script_source: None,
        script_path: None,
        paths: vec![path.to_string_lossy().into_owned()],
        literal: false,
        ignore_case: false,
        single_line: false,
        hidden: false,
        no_ignore: false,
        follow_symlinks: false,
        types: vec![],
        types_not: vec![],
        globs: vec![],
        at_least: Some(1),
        at_most: None,
        allow_non_convergent: false,
        allow_syntax_errors: false,
        max_bytes: DEFAULT_MAX_BYTES,
        max_files: DEFAULT_MAX_FILES,
    }
}

fn structural_args(path: &std::path::Path) -> StructuralArgs {
    StructuralArgs {
        lang: "rust".to_owned(),
        query: None,
        ast_pattern: None,
        template: String::new(),
        paths: vec![path.to_string_lossy().into_owned()],
        apply: false,
        hidden: false,
        no_ignore: false,
        follow_symlinks: false,
        types: vec![],
        types_not: vec![],
        globs: vec![],
        at_least: Some(1),
        at_most: None,
        allow_syntax_errors: false,
        max_bytes: DEFAULT_MAX_BYTES,
        max_files: DEFAULT_MAX_FILES,
    }
}

fn extract_text(result: CallToolResult) -> String {
    assert!(!result.is_error.unwrap_or(false), "tool returned isError=true: {result:?}");
    match &result.content[0].raw {
        rmcp::model::RawContent::Text(t) => t.text.clone(),
        other => panic!("expected text content, got {other:?}"),
    }
}

#[tokio::test]
async fn preview_emits_plan_json_for_matching_pattern() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("a.txt"), "old line\n").unwrap();
    let out =
        server().recast_preview(Parameters(rewrite_args("old", "new", dir.path()))).await.unwrap();
    let body = extract_text(out);
    assert!(body.contains("\"kind\":\"plan\""), "missing kind=plan: {body}");
    assert!(body.contains("\"total_matches\":1"), "expected 1 match: {body}");
    // Dry-run: file content must be untouched.
    assert_eq!(fs::read_to_string(dir.path().join("a.txt")).unwrap(), "old line\n");
}

#[tokio::test]
async fn apply_writes_changes_to_disk() {
    let dir = TempDir::new().unwrap();
    let target = dir.path().join("a.txt");
    fs::write(&target, "old line\n").unwrap();
    let out =
        server().recast_apply(Parameters(rewrite_args("old", "new", dir.path()))).await.unwrap();
    let body = extract_text(out);
    assert!(body.contains("\"kind\":\"apply\""), "missing kind=apply: {body}");
    assert!(body.contains("\"files_written\":1"), "expected 1 file written: {body}");
    assert_eq!(fs::read_to_string(&target).unwrap(), "new line\n");
}

#[tokio::test]
async fn preview_returns_already_applied_for_zero_matches() {
    // Zero matches + convergent rewrite is intentionally a success
    // outcome (the run is a no-op against an already-converted tree),
    // not a guard violation. The TooFewMatches guard only fires when
    // the planner can't classify the run as already-applied.
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("a.txt"), "no match here\n").unwrap();
    let out = server()
        .recast_preview(Parameters(rewrite_args("nonexistent", "x", dir.path())))
        .await
        .unwrap();
    let body = extract_text(out);
    assert!(body.contains("\"outcome\":\"already_applied\""), "expected already_applied: {body}");
}

#[tokio::test]
async fn preview_surfaces_too_few_matches_when_at_least_exceeds_actual() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("a.txt"), "old\n").unwrap();
    let mut args = rewrite_args("old", "new", dir.path());
    args.at_least = Some(5); // 1 actual match < 5 required
    let err = server().recast_preview(Parameters(args)).await.unwrap_err();
    let data = err.data.as_ref().unwrap_or(&serde_json::Value::Null);
    assert_eq!(data["kind"], "too_few_matches", "wrong kind: {data}");
}

#[tokio::test]
async fn preview_refuses_non_convergent_pattern() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("a.txt"), "aaaa\n").unwrap();
    let err =
        server().recast_preview(Parameters(rewrite_args("a", "aa", dir.path()))).await.unwrap_err();
    let data = err.data.as_ref().unwrap_or(&serde_json::Value::Null);
    assert_eq!(data["kind"], "non_convergent", "wrong kind: {data}");
}

#[tokio::test]
async fn preview_with_scripted_replacement_runs_rhai() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("a.txt"), "v=3\n").unwrap();
    let mut args = rewrite_args(r"\d+", "", dir.path());
    args.script_source = Some("(parse_int(captures[0]) + 1).to_string()".to_owned());
    args.allow_non_convergent = true; // \d+ still matches the post-image; that's fine here.
    let out = server().recast_preview(Parameters(args)).await.unwrap();
    let body = extract_text(out);
    assert!(body.contains("\"total_matches\":1"), "scripted preview: {body}");
}

#[tokio::test]
async fn rejects_both_script_source_and_script_path() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("a.txt"), "x\n").unwrap();
    let mut args = rewrite_args("x", "y", dir.path());
    args.script_source = Some("'X'".to_owned());
    args.script_path = Some(dir.path().join("nope.rhai"));
    let err = server().recast_preview(Parameters(args)).await.unwrap_err();
    assert!(
        err.message.contains("script_source") && err.message.contains("script_path"),
        "unexpected message: {}",
        err.message
    );
}

#[tokio::test]
async fn structural_friendly_ast_pattern_compiles_and_runs() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("a.rs"), "fn foo() {}\n").unwrap();
    let mut args = structural_args(dir.path());
    args.ast_pattern = Some("fn $NAME() {}".to_owned());
    args.template = "fn ${NAME}_v2() {}".to_owned();
    let out = server().recast_structural(Parameters(args)).await.unwrap();
    let body = extract_text(out);
    assert!(body.contains("\"total_matches\":1"), "ast_pattern compile: {body}");
}

#[tokio::test]
async fn structural_rejects_both_query_and_ast_pattern() {
    let dir = TempDir::new().unwrap();
    let mut args = structural_args(dir.path());
    args.query = Some("(identifier) @id".to_owned());
    args.ast_pattern = Some("fn $NAME() {}".to_owned());
    args.template = "x".to_owned();
    let err = server().recast_structural(Parameters(args)).await.unwrap_err();
    assert!(
        err.message.contains("query") && err.message.contains("ast_pattern"),
        "unexpected message: {}",
        err.message
    );
}

#[tokio::test]
async fn structural_requires_one_of_query_or_ast_pattern() {
    let dir = TempDir::new().unwrap();
    let mut args = structural_args(dir.path());
    args.template = "x".to_owned();
    let err = server().recast_structural(Parameters(args)).await.unwrap_err();
    assert!(err.message.contains("required"), "unexpected message: {}", err.message);
}

#[tokio::test]
async fn recover_with_no_leftovers_returns_zero_summary() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("a.txt"), "clean\n").unwrap();
    let out = server()
        .recast_recover(Parameters(RecoverArgs {
            paths: vec![dir.path().to_string_lossy().into_owned()],
        }))
        .await
        .unwrap();
    let body = extract_text(out);
    assert!(body.contains("\"backups_restored\":0"));
    assert!(body.contains("\"temps_removed\":0"));
}

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
        word: false,
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
        include_leading_attrs: false,
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

fn rename_args(pairs: &[(&str, &str)], path: &std::path::Path) -> RenameArgs {
    RenameArgs {
        renames: pairs.iter().map(|(k, v)| ((*k).to_owned(), (*v).to_owned())).collect(),
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

#[tokio::test]
async fn rename_tool_keeps_dependent_renames_apart() {
    let dir = TempDir::new().unwrap();
    let target = dir.path().join("c.rs");
    fs::write(&target, "use Foo;\nuse Bar;\n").unwrap();

    let mut args = rename_args(&[("Foo", "Bar"), ("Bar", "Baz")], dir.path());
    args.apply = true;
    server().recast_rename(Parameters(args)).await.unwrap();

    assert_eq!(fs::read_to_string(&target).unwrap(), "use Bar;\nuse Baz;\n");
}

#[tokio::test]
async fn rename_tool_says_when_a_map_is_correct_only_once() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("a.txt"), "Foo Bar\n").unwrap();

    let out = server()
        .recast_rename(Parameters(rename_args(&[("Foo", "Bar"), ("Bar", "Baz")], dir.path())))
        .await
        .unwrap();
    let body = format!("{out:?}");
    assert!(body.contains("correct exactly once"), "missing rerun note: {body}");
}

#[tokio::test]
async fn rename_tool_says_when_a_map_is_rerunnable() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("a.txt"), "Foo Bar\n").unwrap();

    let out = server()
        .recast_rename(Parameters(rename_args(&[("Foo", "Qux"), ("Bar", "Quux")], dir.path())))
        .await
        .unwrap();
    let body = format!("{out:?}");
    assert!(body.contains("re-runnable"), "missing rerun note: {body}");
}

#[tokio::test]
async fn rename_tool_refuses_a_map_that_feeds_itself() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("a.txt"), "Foo\n").unwrap();

    let err = server()
        .recast_rename(Parameters(rename_args(&[("Foo", "Foo Bar")], dir.path())))
        .await
        .unwrap_err();
    let data = err.data.as_ref().unwrap_or(&serde_json::Value::Null);
    assert_eq!(data["kind"], "rename_map_diverges", "wrong kind: {data}");
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

/// The MCP write paths previously called `apply_changes` with no lock
/// at all, so an agent contended with nothing — not even a concurrent
/// CLI `--apply`.
#[tokio::test]
async fn apply_refuses_when_the_workspace_lock_is_held() {
    let dir = TempDir::new().unwrap();
    fs::create_dir_all(dir.path().join(".git")).unwrap();
    let target = dir.path().join("a.txt");
    fs::write(&target, "old\n").unwrap();

    let root = recast_core::workspace_root(&[dir.path().to_path_buf()]);
    let lock_path = recast_core::workspace_lock_path(&root);
    fs::create_dir_all(lock_path.parent().unwrap()).unwrap();
    let held = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .open(&lock_path)
        .unwrap();
    fs2::FileExt::try_lock_exclusive(&held).unwrap();

    let err = server()
        .recast_apply(Parameters(rewrite_args("old", "new", dir.path())))
        .await
        .unwrap_err();
    let data = err.data.as_ref().unwrap_or(&serde_json::Value::Null);
    assert_eq!(data["kind"], "locked", "wrong kind: {data}");
    assert_eq!(fs::read_to_string(&target).unwrap(), "old\n", "wrote despite the lock");

    // `force` has no MCP argument on purpose: a crashed holder releases
    // its flock, so a held lock means a live peer. An empty list is the
    // honest answer, and the message must not tell the agent otherwise.
    assert_eq!(data["remedies"].as_array().map(Vec::len), Some(0), "{data}");
    assert!(!err.message.contains("--force"), "{}", err.message);
    assert!(!err.message.contains("set "), "{}", err.message);
}

#[tokio::test]
async fn errors_carry_remedies_in_this_server_s_own_vocabulary() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("a.txt"), "let x = Outcome;\n").unwrap();

    let mut args = rewrite_args("Outcome", "ReadOutcome", dir.path());
    args.literal = true;
    let err = server().recast_preview(Parameters(args)).await.unwrap_err();
    let data = err.data.as_ref().unwrap_or(&serde_json::Value::Null);

    assert_eq!(data["kind"], "non_convergent_replacement", "{data}");
    assert_eq!(data["remedies"], serde_json::json!(["word", "allow_non_convergent"]), "{data}");
    // MCP spelling in the prose, never the CLI one.
    assert!(err.message.contains("set word or allow_non_convergent"), "{}", err.message);
    assert!(!err.message.contains("--"), "{}", err.message);
}

#[tokio::test]
async fn a_guard_violation_points_at_its_own_argument() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("a.txt"), "old\n").unwrap();

    let mut args = rewrite_args("nope", "x", dir.path());
    args.at_least = Some(1);
    let err = server().recast_preview(Parameters(args)).await.unwrap_err();
    let data = err.data.as_ref().unwrap_or(&serde_json::Value::Null);
    assert_eq!(data["remedies"], serde_json::json!(["at_least"]), "{data}");
}

#[tokio::test]
async fn preview_zero_matches_is_a_guard_violation() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("a.txt"), "no match here\n").unwrap();
    let err = server()
        .recast_preview(Parameters(rewrite_args("nonexistent", "x", dir.path())))
        .await
        .unwrap_err();
    let data = err.data.as_ref().unwrap_or(&serde_json::Value::Null);
    assert_eq!(data["kind"], "too_few_matches", "wrong kind: {data}");
}

#[tokio::test]
async fn preview_returns_already_applied_when_at_least_zero() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("a.txt"), "no match here\n").unwrap();
    let mut args = rewrite_args("nonexistent", "x", dir.path());
    args.at_least = Some(0);
    let out = server().recast_preview(Parameters(args)).await.unwrap();
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
    assert_eq!(data["kind"], "non_convergent_replacement", "wrong kind: {data}");
}

#[tokio::test]
async fn preview_distinguishes_context_driven_non_convergence() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("a.txt"), "aabb\n").unwrap();
    let err =
        server().recast_preview(Parameters(rewrite_args("ab", "a", dir.path()))).await.unwrap_err();
    let data = err.data.as_ref().unwrap_or(&serde_json::Value::Null);
    assert_eq!(data["kind"], "non_convergent_context", "wrong kind: {data}");
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

fn search_args(pattern: &str, path: &std::path::Path) -> SearchArgs {
    SearchArgs {
        pattern: Some(pattern.to_owned()),
        lang: None,
        query: None,
        ast_pattern: None,
        paths: vec![path.to_string_lossy().into_owned()],
        literal: false,
        word: false,
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
        max_bytes: DEFAULT_MAX_BYTES,
        max_files: DEFAULT_MAX_FILES,
    }
}

#[tokio::test]
async fn search_tool_finds_matches() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("a.txt"), "foo bar foo\n").unwrap();

    let out = server().recast_search(Parameters(search_args("foo", dir.path()))).await.unwrap();
    let body = extract_text(out);
    assert!(body.contains("\"kind\":\"search\""), "missing kind=search: {body}");
    assert!(body.contains("\"total_matches\":2"), "expected 2 matches: {body}");
    assert!(body.contains("\"line\":1"), "expected line 1: {body}");
    assert!(body.contains("\"column\":1"), "expected column 1: {body}");
}

#[tokio::test]
async fn search_tool_guard_error_on_no_match() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("a.txt"), "bar\n").unwrap();

    let err = server().recast_search(Parameters(search_args("foo", dir.path()))).await.unwrap_err();
    let data = err.data.as_ref().unwrap_or(&serde_json::Value::Null);
    assert_eq!(data["kind"], "too_few_matches", "wrong kind: {data}");
}

#[tokio::test]
async fn search_tool_structural_surfaces_capture_name() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("a.rs"), "fn foo() {}\n").unwrap();

    let args = SearchArgs {
        pattern: None,
        lang: Some("rust".to_owned()),
        query: None,
        ast_pattern: Some("fn $NAME() {}".to_owned()),
        paths: vec![dir.path().to_string_lossy().into_owned()],
        at_least: Some(1),
        at_most: None,
        literal: false,
        word: false,
        ignore_case: false,
        single_line: false,
        hidden: false,
        no_ignore: false,
        follow_symlinks: false,
        types: vec![],
        types_not: vec![],
        globs: vec![],
        max_bytes: DEFAULT_MAX_BYTES,
        max_files: DEFAULT_MAX_FILES,
    };
    let out = server().recast_search(Parameters(args)).await.unwrap();
    let body = extract_text(out);
    assert!(body.contains("\"kind\":\"search\""), "missing kind=search: {body}");
    assert!(body.contains("\"total_matches\":1"), "expected 1 match: {body}");
    assert!(body.contains("\"capture\":"), "expected capture field: {body}");
}

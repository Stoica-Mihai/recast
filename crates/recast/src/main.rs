mod completion;

use std::io::{self, Read, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Context, Result, anyhow};
use clap::{ArgAction, Args, Parser};
use clap_complete::Shell;
use recast_core::{
    CompiledPattern, Error as CoreError, Language, PatternOptions, Plan, PlanOptions, PlanOutcome,
    ScriptRewriter, WalkOptions, WorkspaceLock, acquire_workspace_lock, apply_changes, build_pool,
    check_match_counts, json, plan_rewrite, plan_rewrite_scripted, plan_structural_rewrite,
    recover_sweep, rewrite_text, rewrite_text_scripted, structural_rewrite,
};

const EXIT_OK: u8 = 0;
const EXIT_CHECK_WOULD_CHANGE: u8 = 1;
const EXIT_GUARD_VIOLATED: u8 = 2;
const EXIT_INTERNAL: u8 = 3;

/// `--diff` / `--json` / `--quiet` / `--verbose`. Anything that
/// controls *how* we print, not *what* we plan.
#[derive(Debug, Args)]
pub(crate) struct OutputOptions {
    /// Show unified diff per file (default when --apply absent).
    #[arg(long, action = ArgAction::SetTrue)]
    diff: bool,

    /// Emit machine-readable JSON summary on stdout.
    #[arg(long, action = ArgAction::SetTrue)]
    json: bool,

    /// Suppress diff body; print only the summary.
    #[arg(long, action = ArgAction::SetTrue)]
    quiet: bool,

    /// Per-file timing and counters.
    #[arg(short = 'v', long, action = ArgAction::SetTrue)]
    verbose: bool,
}

/// `--at-least` / `--at-most` / `--allow-non-convergent` /
/// `--max-bytes` / `--max-files`. Knobs that shape the planner's
/// safety bounds.
#[derive(Debug, Args)]
pub(crate) struct GuardOptions {
    /// Require at least N matches across all files (default 1). 0 disables
    /// the guard.
    #[arg(long, value_name = "N")]
    at_least: Option<usize>,

    /// Require at most N matches (default unbounded).
    #[arg(long, value_name = "N")]
    at_most: Option<usize>,

    /// Skip the convergence (idempotency) check.
    #[arg(long, action = ArgAction::SetTrue)]
    allow_non_convergent: bool,

    /// Skip the syntax-regression guard. By default a rewrite whose
    /// output introduces new tree-sitter parse errors (in a file whose
    /// extension maps to a compiled grammar) is rejected.
    #[arg(long, action = ArgAction::SetTrue)]
    allow_syntax_errors: bool,

    /// Refuse files larger than N bytes (default 10485760).
    #[arg(long, value_name = "N", default_value_t = 10 * 1024 * 1024)]
    max_bytes: u64,

    /// Refuse runs touching more than N files (default 1000).
    #[arg(long, value_name = "N", default_value_t = 1000)]
    max_files: usize,
}

/// `--lang` / `--query` / `--ast`. Structural-mode dispatch.
#[derive(Debug, Args)]
pub(crate) struct StructuralCli {
    /// Structural mode: tree-sitter language to parse with. Requires
    /// `--query`. In this mode the positional PATTERN is ignored;
    /// REPLACEMENT is used as the template (with `$name` / `${name}`
    /// capture substitutions). Currently supported: `rust`.
    #[arg(long, value_name = "LANG", requires = "query")]
    lang: Option<String>,

    /// Tree-sitter S-expression query (structural mode). The capture
    /// named `@root` (or, absent that, the outermost capture in each
    /// match) defines the byte range to replace.
    #[arg(long, value_name = "QUERY", requires = "lang", conflicts_with = "ast_pattern")]
    query: Option<String>,

    /// Friendly structural pattern written in the target language with
    /// `$NAME` placeholders (compiled to a tree-sitter query). Use
    /// either `--query` or `--ast` with `--lang`, not both.
    #[arg(long = "ast", value_name = "PATTERN", requires = "lang", conflicts_with = "query")]
    ast_pattern: Option<String>,

    /// Structural mode: extend each match backward over its contiguous
    /// leading `#[attr]` / doc-comment lines, so deleting an item also
    /// removes its attributes and docs instead of orphaning them. A
    /// blank line ends the run.
    #[arg(long, requires = "lang", action = ArgAction::SetTrue)]
    include_leading_attrs: bool,
}

#[derive(Debug, Parser)]
#[command(name = "recast", about = "Safe, atomic, transparent multi-file text rewrites.", version)]
pub(crate) struct Cli {
    /// Regex pattern. Multi-line by default. Use --literal for plain-string
    /// matching.
    #[arg(required_unless_present_any = ["completions", "recover"])]
    pattern: Option<String>,

    /// Replacement template. $1, $2, ${name} interpolated unless --literal
    /// is set.
    #[arg(required_unless_present_any = ["completions", "recover"])]
    replacement: Option<String>,

    /// Paths or globs to scan. Defaults to the current directory if omitted.
    /// .gitignore respected by default.
    #[arg(default_values_t = [".".to_owned()])]
    paths: Vec<String>,

    #[command(flatten)]
    output: OutputOptions,

    /// Atomically write the changes.
    #[arg(long, action = ArgAction::SetTrue, conflicts_with_all = ["check"])]
    apply: bool,

    /// Exit non-zero if any file would change. No output, no writes.
    #[arg(long, action = ArgAction::SetTrue, conflicts_with_all = ["apply"])]
    check: bool,

    #[command(flatten)]
    guard: GuardOptions,

    /// Include hidden files.
    #[arg(long, action = ArgAction::SetTrue)]
    hidden: bool,

    /// Disable .gitignore filtering.
    #[arg(long, action = ArgAction::SetTrue)]
    no_ignore: bool,

    /// Only files of this type (e.g. `rust`, `js`, `py`). Mirrors ripgrep
    /// `--type`. Repeatable.
    #[arg(short = 't', long = "type", value_name = "LANG", action = ArgAction::Append)]
    type_: Vec<String>,

    /// Exclude files of this type. Repeatable.
    #[arg(short = 'T', long = "type-not", value_name = "LANG", action = ArgAction::Append)]
    type_not: Vec<String>,

    /// Include/exclude glob (`!pattern` to exclude). Repeatable. Globs are
    /// applied relative to the first path argument.
    #[arg(short = 'g', long = "glob", value_name = "GLOB", action = ArgAction::Append)]
    glob: Vec<String>,

    /// Treat pattern and replacement as literal strings.
    #[arg(short = 'L', long, action = ArgAction::SetTrue)]
    literal: bool,

    /// Case-insensitive matching.
    #[arg(short = 'i', long, action = ArgAction::SetTrue)]
    ignore_case: bool,

    /// Disable implicit (?s) — make `.` not match \n.
    #[arg(short = 's', long, action = ArgAction::SetTrue)]
    single_line: bool,

    /// Worker threads (default = num CPUs).
    #[arg(long, value_name = "N")]
    threads: Option<usize>,

    /// Read input from stdin, rewrite once, write to stdout. Skips the
    /// walker, atomic commit, and convergence check — single-buffer.
    /// Match-count guard still applies.
    #[arg(long, action = ArgAction::SetTrue, conflicts_with_all = ["apply", "check", "json"])]
    stdin: bool,

    /// Rhai script file run per regex match; its return value becomes
    /// the replacement. The positional REPLACEMENT argument is still
    /// required (pass any placeholder, e.g. `""`) but its value is
    /// ignored when `--script` is set. Script sees `captures`
    /// (array; index 0 is the full match) and `whole` (full-match
    /// alias — `match` is a Rhai reserved keyword).
    #[arg(long, value_name = "PATH")]
    script: Option<PathBuf>,

    #[command(flatten)]
    structural: StructuralCli,

    /// Scan PATHS for leftover `.recast.bak.*` / `.recast.tmp.*`
    /// siblings from a previous interrupted --apply and reconcile them
    /// (restore backups when the target is missing; delete stale temps
    /// and backups when the target is present). Skips all rewrite
    /// modes; PATTERN/REPLACEMENT are not consulted.
    #[arg(long, action = ArgAction::SetTrue)]
    recover: bool,

    /// Skip the workspace lock check. By default `--apply` / `--recover`
    /// take an exclusive non-blocking lock on `<root>/.recast.lock` so
    /// two concurrent rewrites against the same tree don't interleave;
    /// `--force` bypasses that guard (use only if you know what you're
    /// doing).
    #[arg(long, action = ArgAction::SetTrue)]
    force: bool,

    /// Generate a shell completion script and exit.
    #[arg(long, value_name = "SHELL", value_enum)]
    completions: Option<Shell>,
}

impl Cli {
    fn pattern_options(&self) -> PatternOptions {
        PatternOptions {
            literal: self.literal,
            ignore_case: self.ignore_case,
            single_line: self.single_line,
        }
    }

    fn plan_options(&self) -> PlanOptions {
        PlanOptions {
            pattern_options: self.pattern_options(),
            walk_options: WalkOptions {
                hidden: self.hidden,
                no_ignore: self.no_ignore,
                follow_symlinks: false,
                types: self.type_.clone(),
                types_not: self.type_not.clone(),
                globs: self.glob.clone(),
            },
            at_least: self.min_matches(),
            at_most: self.guard.at_most,
            allow_non_convergent: self.guard.allow_non_convergent,
            allow_syntax_errors: self.guard.allow_syntax_errors,
            max_bytes: self.guard.max_bytes,
            max_files: self.guard.max_files,
        }
    }

    /// `--at-least` value with the implicit default of 1 applied so the
    /// guard always fires unless the user explicitly passes `0`.
    fn min_matches(&self) -> Option<usize> {
        Some(self.guard.at_least.unwrap_or(1))
    }

    fn paths_as_pathbufs(&self) -> Vec<PathBuf> {
        self.paths.iter().map(PathBuf::from).collect()
    }

    /// `--recover` accepts any number of paths as positionals. Clap binds
    /// them to the `pattern` and `replacement` slots first (both
    /// `Option`), so fold those back into the path list and drop the
    /// trailing `"."` default-value sentinel if it was tacked on after
    /// real positionals.
    fn recover_paths(&self) -> Vec<PathBuf> {
        let mut paths: Vec<PathBuf> = Vec::new();
        if let Some(p) = self.pattern.as_deref() {
            paths.push(PathBuf::from(p));
        }
        if let Some(p) = self.replacement.as_deref() {
            paths.push(PathBuf::from(p));
        }
        for p in &self.paths {
            paths.push(PathBuf::from(p));
        }
        if paths.len() > 1 && paths.last().map(|p| p.as_os_str() == ".").unwrap_or(false) {
            paths.pop();
        }
        paths
    }
}

fn main() -> ExitCode {
    init_tracing();
    let cli = Cli::parse();
    match run(cli) {
        Ok(code) => ExitCode::from(code),
        Err(err) => {
            eprintln!("recast: {err}");
            for cause in err.chain().skip(1) {
                eprintln!("  caused by: {cause}");
            }
            ExitCode::from(EXIT_INTERNAL)
        }
    }
}

fn init_tracing() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
        )
        .with_writer(io::stderr)
        .try_init();
}

fn run(cli: Cli) -> Result<u8> {
    if let Some(shell) = cli.completions {
        completion::print(shell, &mut io::stdout().lock());
        return Ok(EXIT_OK);
    }

    let _lock_guard = match acquire_workspace_lock_for(&cli) {
        Ok(g) => g,
        Err(err) => return handle_plan_error(err, cli.output.json),
    };

    if cli.recover {
        let paths = cli.recover_paths();
        let summary = recover_sweep(&paths).context("recover sweep")?;
        eprintln!(
            "recast: recovered {} backup(s), removed {} stale backup(s), removed {} temp(s)",
            summary.backups_restored, summary.backups_removed, summary.temps_removed
        );
        return Ok(EXIT_OK);
    }

    // Resolve structural --query / --ast before constructing the worker
    // pool: a malformed structural CLI should fail without spawning N
    // rayon threads first.
    let structural = resolve_structural(&cli)?;

    let pattern = cli.pattern.as_deref();
    let replacement = cli.replacement.as_deref();

    let script = match &cli.script {
        Some(path) => Some(ScriptRewriter::from_file(path).map_err(anyhow::Error::from)?),
        None => None,
    };

    // Stdin path is single-buffer; no worker pool needed.
    if cli.stdin && structural.is_none() {
        let pattern = pattern.ok_or_else(|| anyhow!("pattern required"))?;
        let replacement = replacement.ok_or_else(|| anyhow!("replacement required"))?;
        return run_stdin(&cli, pattern, replacement, script.as_ref());
    }

    let pool = build_pool(cli.threads).context("configure worker thread pool")?;
    pool.install(|| {
        if let Some((lang, query, template)) = structural.as_ref() {
            return run_structural(&cli, *lang, query, template);
        }
        let pattern = pattern.ok_or_else(|| anyhow!("pattern required"))?;
        let replacement = replacement.ok_or_else(|| anyhow!("replacement required"))?;
        let paths = cli.paths_as_pathbufs();
        let opts = cli.plan_options();
        let result = match &script {
            Some(s) => plan_rewrite_scripted(pattern, s, &paths, &opts),
            None => plan_rewrite(pattern, replacement, &paths, &opts),
        };
        let plan = match result {
            Ok(plan) => plan,
            Err(err) => return handle_plan_error(err, cli.output.json),
        };
        dispatch_plan(&cli, &plan)
    })
}

/// Resolve the structural-mode triple (language, query string, template)
/// from the CLI. Returns `Ok(None)` for non-structural invocations;
/// returns `Err` if `--lang` is set but the user didn't supply
/// `--query`/`--ast` or the lang/pattern doesn't compile.
fn resolve_structural(cli: &Cli) -> Result<Option<(Language, String, &str)>> {
    let Some(lang_name) = cli.structural.lang.as_deref() else {
        return Ok(None);
    };
    let template = cli
        .replacement
        .as_deref()
        .ok_or_else(|| anyhow!("REPLACEMENT positional is the template in structural mode"))?;
    let lang = resolve_lang(lang_name)?;
    let query = if let Some(q) = cli.structural.query.as_deref() {
        q.to_owned()
    } else if let Some(pat) = cli.structural.ast_pattern.as_deref() {
        recast_core::compile_friendly_query(lang, pat).map_err(anyhow::Error::from)?
    } else {
        return Err(anyhow!("--query or --ast required with --lang"));
    };
    Ok(Some((lang, query, template)))
}

fn resolve_lang(name: &str) -> Result<Language> {
    Language::from_name(name).map_err(anyhow::Error::from)
}

fn dispatch_plan(cli: &Cli, plan: &Plan) -> Result<u8> {
    if cli.apply {
        emit_apply(&cli.output, plan).context("emit apply output")?;
        let outcome = apply_changes(plan).context("apply changes")?;
        if cli.output.json {
            println!("{}", json::from_apply(plan, &outcome).to_line()?);
        }
        return Ok(EXIT_OK);
    }

    if cli.check {
        let would_change = plan.changes.len();
        if cli.output.json {
            println!("{}", json::from_check(plan).to_line()?);
        }
        if matches!(plan.outcome, PlanOutcome::AlreadyApplied) || would_change == 0 {
            return Ok(EXIT_OK);
        }
        return Ok(EXIT_CHECK_WOULD_CHANGE);
    }

    emit_diff(&cli.output, plan).context("emit diff output")?;
    Ok(EXIT_OK)
}

fn run_structural(cli: &Cli, lang: Language, query: &str, template: &str) -> Result<u8> {
    if cli.stdin {
        let mut buf = String::new();
        io::stdin().lock().read_to_string(&mut buf).context("read stdin")?;
        let outcome = match structural_rewrite(lang, &buf, query, template) {
            Ok(o) => o,
            Err(e) => return handle_plan_error(e, cli.output.json),
        };
        if let Err(e) = check_match_counts(outcome.matches, cli.min_matches(), cli.guard.at_most) {
            return handle_plan_error(e, cli.output.json);
        }
        io::stdout().lock().write_all(outcome.text.as_bytes()).context("write stdout")?;
        return Ok(EXIT_OK);
    }

    let paths = cli.paths_as_pathbufs();
    let opts = cli.plan_options();
    let plan = match plan_structural_rewrite(
        lang,
        query,
        template,
        &paths,
        &opts,
        cli.structural.include_leading_attrs,
    ) {
        Ok(p) => p,
        Err(e) => return handle_plan_error(e, cli.output.json),
    };

    dispatch_plan(cli, &plan)
}

fn run_stdin(
    cli: &Cli,
    pattern: &str,
    replacement: &str,
    script: Option<&ScriptRewriter>,
) -> Result<u8> {
    let compiled = CompiledPattern::compile(pattern, replacement, &cli.pattern_options())
        .context("compile pattern")?;

    let mut buf = String::new();
    io::stdin().lock().read_to_string(&mut buf).context("read stdin")?;
    let outcome = match script {
        Some(s) => rewrite_text_scripted(&compiled, s, &buf).map_err(anyhow::Error::from)?,
        None => rewrite_text(&compiled, &buf),
    };

    if let Err(e) = check_match_counts(outcome.matches, cli.min_matches(), cli.guard.at_most) {
        return handle_plan_error(e, cli.output.json);
    }

    io::stdout().lock().write_all(outcome.after.as_bytes()).context("write stdout")?;
    Ok(EXIT_OK)
}

fn emit_diff(out: &OutputOptions, plan: &Plan) -> Result<()> {
    if out.json {
        println!("{}", json::from_plan(plan).to_line()?);
        return Ok(());
    }

    let mut stdout = io::stdout().lock();
    if matches!(plan.outcome, PlanOutcome::AlreadyApplied) {
        writeln!(stdout, "recast: already applied; no changes needed.")?;
        return Ok(());
    }
    if !out.quiet {
        for change in &plan.changes {
            stdout.write_all(change.diff.as_bytes())?;
        }
    }
    writeln!(
        stdout,
        "recast: {} file(s) would change, {} match(es) across {} scanned.",
        plan.changes.len(),
        plan.total_matches,
        plan.files_scanned
    )?;
    Ok(())
}

fn emit_apply(out: &OutputOptions, plan: &Plan) -> Result<()> {
    if out.json {
        return Ok(());
    }
    let mut stderr = io::stderr().lock();
    if matches!(plan.outcome, PlanOutcome::AlreadyApplied) {
        writeln!(stderr, "recast: already applied; no changes needed.")?;
        return Ok(());
    }
    if out.verbose {
        for change in &plan.changes {
            writeln!(
                stderr,
                "recast: writing {} ({} match(es))",
                change.path.display(),
                change.matches
            )?;
        }
    }
    writeln!(
        stderr,
        "recast: applying {} file(s), {} match(es).",
        plan.changes.len(),
        plan.total_matches
    )?;
    Ok(())
}

fn handle_plan_error(err: CoreError, as_json: bool) -> Result<u8> {
    let code = match &err {
        CoreError::TooFewMatches { .. } | CoreError::TooManyMatches { .. } => EXIT_GUARD_VIOLATED,
        _ => EXIT_INTERNAL,
    };
    if as_json {
        let line = json::from_error(&err, code).to_line().context("serialize json error")?;
        println!("{line}");
    } else {
        eprintln!("recast: {err}");
    }
    Ok(code)
}

fn acquire_workspace_lock_for(cli: &Cli) -> std::result::Result<Option<WorkspaceLock>, CoreError> {
    let writes_tree = cli.apply || cli.recover;
    if !writes_tree || cli.force || cli.stdin {
        return Ok(None);
    }
    // Collect every positional path the invocation will touch so the
    // lock sits at their common ancestor rather than the first path
    // alone. Without that, two `--apply` runs against overlapping
    // subtrees from different CWDs (or one against `src/`, one against
    // `src/sub/`) end up holding two different `.recast.lock` files
    // and proceed in parallel.
    let raw_paths: Vec<PathBuf> =
        if cli.recover { cli.recover_paths() } else { cli.paths_as_pathbufs() };
    let lock_path = workspace_lock_path(&raw_paths);
    acquire_workspace_lock(&lock_path).map(Some)
}

/// Pick the `.recast.lock` location for an apply/recover invocation.
/// Canonicalizes every input path so relative paths from different
/// CWDs collapse to the same anchor, then takes the deepest common
/// ancestor. Falls back to the original path when canonicalization
/// fails (file doesn't exist yet); falls back to `"."` when nothing
/// useful survives.
fn workspace_lock_path(paths: &[PathBuf]) -> PathBuf {
    use std::fs;
    let canonical: Vec<PathBuf> =
        paths.iter().map(|p| fs::canonicalize(p).unwrap_or_else(|_| p.clone())).collect();
    let root = common_ancestor(&canonical).unwrap_or_else(|| PathBuf::from("."));
    let lock_dir = if root.is_file() {
        root.parent().map(|p| p.to_path_buf()).unwrap_or_else(|| PathBuf::from("/"))
    } else {
        root
    };
    lock_dir.join(".recast.lock")
}

/// Deepest path prefix shared by every element of `paths`. Returns
/// `None` only if the slice is empty. For absolute paths the result is
/// at worst `"/"`; for purely-relative paths from the same CWD it can
/// degenerate to the empty path (caller substitutes `"."`).
fn common_ancestor(paths: &[PathBuf]) -> Option<PathBuf> {
    let mut iter = paths.iter();
    let first = iter.next()?;
    let mut common: Vec<&std::ffi::OsStr> = first.iter().collect();
    for p in iter {
        let shared = common.iter().zip(p.iter()).take_while(|(a, b)| **a == *b).count();
        common.truncate(shared);
        if common.is_empty() {
            return None;
        }
    }
    let mut buf = PathBuf::new();
    for c in common {
        buf.push(c);
    }
    Some(buf)
}

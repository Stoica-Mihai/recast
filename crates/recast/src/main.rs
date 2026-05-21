mod completion;

use std::io::{self, Read, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Context, Result, anyhow};
use clap::{ArgAction, Parser};
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

    /// Show unified diff per file (default when --apply absent).
    #[arg(long, action = ArgAction::SetTrue)]
    diff: bool,

    /// Atomically write the changes.
    #[arg(long, action = ArgAction::SetTrue, conflicts_with_all = ["check"])]
    apply: bool,

    /// Exit non-zero if any file would change. No output, no writes.
    #[arg(long, action = ArgAction::SetTrue, conflicts_with_all = ["apply"])]
    check: bool,

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

    /// Refuse files larger than N bytes (default 10485760).
    #[arg(long, value_name = "N", default_value_t = 10 * 1024 * 1024)]
    max_bytes: u64,

    /// Refuse runs touching more than N files (default 1000).
    #[arg(long, value_name = "N", default_value_t = 1000)]
    max_files: usize,

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

    /// Emit machine-readable JSON summary on stdout.
    #[arg(long, action = ArgAction::SetTrue)]
    json: bool,

    /// Suppress diff body; print only the summary.
    #[arg(long, action = ArgAction::SetTrue)]
    quiet: bool,

    /// Per-file timing and counters.
    #[arg(short = 'v', long, action = ArgAction::SetTrue)]
    verbose: bool,

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
            at_most: self.at_most,
            allow_non_convergent: self.allow_non_convergent,
            max_bytes: self.max_bytes,
            max_files: self.max_files,
        }
    }

    /// `--at-least` value with the implicit default of 1 applied so the
    /// guard always fires unless the user explicitly passes `0`.
    fn min_matches(&self) -> Option<usize> {
        Some(self.at_least.unwrap_or(1))
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
        Err(err) => return handle_plan_error(err, cli.json),
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

    if let Some(lang_name) = &cli.lang {
        let template = cli
            .replacement
            .as_deref()
            .ok_or_else(|| anyhow!("REPLACEMENT positional is the template in structural mode"))?;
        let lang = resolve_lang(lang_name)?;
        let query: String = if let Some(q) = cli.query.as_deref() {
            q.to_owned()
        } else if let Some(pat) = cli.ast_pattern.as_deref() {
            recast_core::compile_friendly_query(lang, pat).map_err(anyhow::Error::from)?
        } else {
            return Err(anyhow!("--query or --ast required with --lang"));
        };
        return run_structural(&cli, lang, &query, template);
    }

    let pattern = cli.pattern.as_deref().ok_or_else(|| anyhow!("pattern required"))?;
    let replacement = cli.replacement.as_deref().ok_or_else(|| anyhow!("replacement required"))?;

    let script = match &cli.script {
        Some(path) => Some(ScriptRewriter::from_file(path).map_err(anyhow::Error::from)?),
        None => None,
    };

    if cli.stdin {
        return run_stdin(&cli, pattern, replacement, script.as_ref());
    }

    let paths = cli.paths_as_pathbufs();
    let opts = cli.plan_options();

    let result = match &script {
        Some(s) => plan_rewrite_scripted(pattern, s, &paths, &opts),
        None => {
            let pool = build_pool(cli.threads).context("configure worker thread pool")?;
            pool.install(|| plan_rewrite(pattern, replacement, &paths, &opts))
        }
    };
    let plan = match result {
        Ok(plan) => plan,
        Err(err) => return handle_plan_error(err, cli.json),
    };

    dispatch_plan(&cli, &plan)
}

fn resolve_lang(name: &str) -> Result<Language> {
    Language::from_name(name).map_err(anyhow::Error::from)
}

fn dispatch_plan(cli: &Cli, plan: &Plan) -> Result<u8> {
    if cli.apply {
        emit_apply(cli, plan).context("emit apply output")?;
        let outcome = apply_changes(plan).context("apply changes")?;
        if cli.json {
            println!("{}", json::from_apply(plan, &outcome).to_line()?);
        }
        return Ok(EXIT_OK);
    }

    if cli.check {
        let would_change = plan.changes.len();
        if cli.json {
            println!("{}", json::from_check(plan).to_line()?);
        }
        if matches!(plan.outcome, PlanOutcome::AlreadyApplied) || would_change == 0 {
            return Ok(EXIT_OK);
        }
        return Ok(EXIT_CHECK_WOULD_CHANGE);
    }

    emit_diff(cli, plan).context("emit diff output")?;
    Ok(EXIT_OK)
}

fn run_structural(cli: &Cli, lang: Language, query: &str, template: &str) -> Result<u8> {
    if cli.stdin {
        let mut buf = String::new();
        io::stdin().lock().read_to_string(&mut buf).context("read stdin")?;
        let outcome = match structural_rewrite(lang, &buf, query, template) {
            Ok(o) => o,
            Err(e) => return handle_plan_error(e, cli.json),
        };
        if let Err(e) = check_match_counts(outcome.matches, cli.min_matches(), cli.at_most) {
            return handle_plan_error(e, cli.json);
        }
        io::stdout().lock().write_all(outcome.text.as_bytes()).context("write stdout")?;
        return Ok(EXIT_OK);
    }

    let paths = cli.paths_as_pathbufs();
    let opts = cli.plan_options();
    let plan = match plan_structural_rewrite(lang, query, template, &paths, &opts) {
        Ok(p) => p,
        Err(e) => return handle_plan_error(e, cli.json),
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

    if let Err(e) = check_match_counts(outcome.matches, cli.min_matches(), cli.at_most) {
        return handle_plan_error(e, cli.json);
    }

    io::stdout().lock().write_all(outcome.after.as_bytes()).context("write stdout")?;
    Ok(EXIT_OK)
}

fn emit_diff(cli: &Cli, plan: &Plan) -> Result<()> {
    if cli.json {
        println!("{}", json::from_plan(plan).to_line()?);
        return Ok(());
    }

    let mut stdout = io::stdout().lock();
    if matches!(plan.outcome, PlanOutcome::AlreadyApplied) {
        writeln!(stdout, "recast: already applied; no changes needed.")?;
        return Ok(());
    }
    if !cli.quiet {
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

fn emit_apply(cli: &Cli, plan: &Plan) -> Result<()> {
    if cli.json {
        return Ok(());
    }
    let mut stderr = io::stderr().lock();
    if matches!(plan.outcome, PlanOutcome::AlreadyApplied) {
        writeln!(stderr, "recast: already applied; no changes needed.")?;
        return Ok(());
    }
    if cli.verbose {
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
    let first =
        cli.paths.first().or(cli.pattern.as_ref()).cloned().unwrap_or_else(|| ".".to_owned());
    let root = PathBuf::from(first);
    let lock_dir = if root.is_file() {
        root.parent().map(|p| p.to_path_buf()).unwrap_or_else(|| PathBuf::from("."))
    } else {
        root
    };
    let lock_path = lock_dir.join(".recast.lock");
    acquire_workspace_lock(&lock_path).map(Some)
}

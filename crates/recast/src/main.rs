mod completion;

use std::io::{self, Read, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Context, Result, anyhow};
use clap::{ArgAction, Args, Parser};
use clap_complete::Shell;
use recast_core::{
    CompiledPattern, Error as CoreError, Language, PatternOptions, Plan, PlanOptions, PlanOutcome,
    RenameMap, ScriptRewriter, SearchOptions, SearchPlan, WalkOptions, WorkspaceLock,
    acquire_workspace_lock_for_paths, apply_changes, build_pool, check_match_counts, json,
    plan_rename, plan_rewrite, plan_rewrite_scripted, plan_search, plan_structural_rewrite,
    plan_structural_search, recover_sweep, rewrite_text, rewrite_text_scripted, structural_rewrite,
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
#[command(
    name = "recast",
    about = "Safe, atomic, transparent multi-file text rewrites.",
    version,
    override_usage = "recast [OPTIONS] <PATTERN> [REPLACEMENT] [PATHS]..."
)]
pub(crate) struct Cli {
    /// Regex pattern. Multi-line by default. Use --literal for plain-string
    /// matching.
    #[arg(required_unless_present_any = ["completions", "recover", "search", "rename"])]
    pattern: Option<String>,

    /// Replacement template. $1, $2, ${name} interpolated unless --literal
    /// is set.
    #[arg(required_unless_present_any = ["completions", "recover", "search", "rename"])]
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

    /// Find matches and report locations (file:line:col:snippet) without rewriting.
    /// Compatible with --json, --quiet, --verbose, --type, all filter flags, and
    /// structural --lang/--query/--ast.
    #[arg(long, action = ArgAction::SetTrue, conflicts_with_all = ["apply", "check", "script", "stdin", "recover"])]
    search: bool,

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

    /// Rename OLD to NEW. Repeatable. Every rename lands in a single
    /// pass, so dependent renames can't feed each other the way two
    /// separate invocations would. Names match as whole words and are
    /// taken literally, not as regexes. PATTERN / REPLACEMENT are not
    /// used in this mode.
    #[arg(
        long,
        value_name = "OLD=NEW",
        action = ArgAction::Append,
        conflicts_with_all = ["script", "lang", "search", "literal", "word", "ignore_case", "single_line"]
    )]
    rename: Vec<String>,

    /// Match whole words only. Wraps the pattern as
    /// `\b{start-half}(?:PATTERN)\b{end-half}` — the same semantics as
    /// `rg --word-regexp`, so a pattern whose own edges are not word
    /// characters stays matchable. Not available in structural mode.
    #[arg(short = 'w', long, action = ArgAction::SetTrue, conflicts_with = "lang")]
    word: bool,

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
    /// take an exclusive non-blocking lock keyed on the enclosing VCS
    /// checkout, stored in `$XDG_RUNTIME_DIR/recast/`, so two concurrent
    /// rewrites against the same tree don't interleave even when they
    /// name different subdirectories; `--force` bypasses that guard (use
    /// only if you know what you're doing).
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
            word: self.word,
        }
    }

    fn walk_options(&self) -> WalkOptions {
        WalkOptions {
            hidden: self.hidden,
            no_ignore: self.no_ignore,
            follow_symlinks: false,
            types: self.type_.clone(),
            types_not: self.type_not.clone(),
            globs: self.glob.clone(),
        }
    }

    fn plan_options(&self) -> PlanOptions {
        PlanOptions {
            pattern_options: self.pattern_options(),
            walk_options: self.walk_options(),
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

    fn search_options(&self) -> SearchOptions {
        SearchOptions {
            pattern_options: self.pattern_options(),
            walk_options: self.walk_options(),
            at_least: self.min_matches(),
            at_most: self.guard.at_most,
            max_bytes: self.guard.max_bytes,
            max_files: self.guard.max_files,
        }
    }

    fn paths_as_pathbufs(&self) -> Vec<PathBuf> {
        self.paths.iter().map(PathBuf::from).collect()
    }

    /// Split each `--rename OLD=NEW` on its first `=`, so a `=` may
    /// appear in NEW.
    fn rename_pairs(&self) -> Result<Vec<(String, String)>> {
        self.rename
            .iter()
            .map(|spec| {
                spec.split_once('=')
                    .map(|(old, new)| (old.to_owned(), new.to_owned()))
                    .ok_or_else(|| anyhow!("--rename expects OLD=NEW, got `{spec}`"))
            })
            .collect()
    }

    /// `--search` with no replacement: clap binds positionals left-to-right
    /// so `pattern` gets the regex and `replacement` gets the first path.
    /// Fold `replacement` (and any further `paths`) back into a path list.
    fn search_paths(&self) -> Vec<PathBuf> {
        let mut paths: Vec<PathBuf> = Vec::new();
        if let Some(p) = self.replacement.as_deref() {
            paths.push(PathBuf::from(p));
        }
        for p in &self.paths {
            if p != "." || paths.is_empty() {
                paths.push(PathBuf::from(p));
            }
        }
        if paths.is_empty() {
            paths.push(PathBuf::from("."));
        }
        paths
    }

    /// `--recover` accepts any number of paths as positionals. Clap binds
    /// them to the `pattern` and `replacement` slots first (both
    /// `Option`), so fold those back into the path list and drop the
    /// trailing `"."` default-value sentinel if it was tacked on after
    /// real positionals.
    /// Every positional argument treated as a path. Modes that take no
    /// PATTERN / REPLACEMENT (`--recover`, `--rename`) still have those
    /// two clap slots in front of `paths`, so a bare `recast --recover
    /// src/` lands `src/` in `pattern`. Pull all three back together and
    /// drop the trailing `.` default when it was not typed.
    fn positional_paths(&self) -> Vec<PathBuf> {
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
        let paths = cli.positional_paths();
        let summary = recover_sweep(&paths).context("recover sweep")?;
        eprintln!(
            "recast: recovered {} backup(s), removed {} stale backup(s), removed {} temp(s)",
            summary.backups_restored, summary.backups_removed, summary.temps_removed
        );
        return Ok(EXIT_OK);
    }

    if cli.search {
        return run_search_mode(cli);
    }

    if !cli.rename.is_empty() {
        return run_rename(&cli);
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
            return run_structural(&cli, *lang, query, template.as_str());
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

fn compile_structural_query(cli: &Cli, lang: Language) -> Result<String> {
    if let Some(q) = cli.structural.query.as_deref() {
        Ok(q.to_owned())
    } else if let Some(pat) = cli.structural.ast_pattern.as_deref() {
        recast_core::compile_friendly_query(lang, pat).map_err(anyhow::Error::from)
    } else {
        Err(anyhow!("--query or --ast required with --lang"))
    }
}

fn resolve_structural_for_search(cli: &Cli) -> Result<Option<(Language, String)>> {
    let Some(lang_name) = cli.structural.lang.as_deref() else {
        return Ok(None);
    };
    let lang = resolve_lang(lang_name)?;
    let query = compile_structural_query(cli, lang)?;
    Ok(Some((lang, query)))
}

fn resolve_structural(cli: &Cli) -> Result<Option<(Language, String, String)>> {
    let Some((lang, query)) = resolve_structural_for_search(cli)? else {
        return Ok(None);
    };
    let template = cli
        .replacement
        .clone()
        .ok_or_else(|| anyhow!("REPLACEMENT positional is the template in structural mode"))?;
    Ok(Some((lang, query, template)))
}

fn resolve_lang(name: &str) -> Result<Language> {
    Language::from_name(name).map_err(anyhow::Error::from)
}

fn run_search_mode(cli: Cli) -> Result<u8> {
    let structural = resolve_structural_for_search(&cli)?;
    let paths = cli.search_paths();
    let opts = cli.search_options();
    let pool = build_pool(cli.threads).context("configure worker thread pool")?;
    pool.install(|| {
        if let Some((lang, query)) = structural {
            let plan = match plan_structural_search(lang, &query, &paths, &opts) {
                Ok(p) => p,
                Err(e) => return handle_plan_error(e, cli.output.json),
            };
            emit_search_results(&cli.output, &plan)?;
            return Ok(EXIT_OK);
        }
        let pattern = cli
            .pattern
            .as_deref()
            .ok_or_else(|| anyhow!("PATTERN is required for regex search mode"))?;
        let plan = match plan_search(pattern, &paths, &opts) {
            Ok(p) => p,
            Err(e) => return handle_plan_error(e, cli.output.json),
        };
        emit_search_results(&cli.output, &plan)?;
        Ok(EXIT_OK)
    })
}

fn emit_search_results(out: &OutputOptions, plan: &SearchPlan) -> Result<()> {
    if out.json {
        println!("{}", json::from_search(plan).to_line()?);
        return Ok(());
    }

    let mut stdout = io::stdout().lock();

    if !out.quiet {
        for file in &plan.files {
            if out.verbose {
                writeln!(
                    stdout,
                    "--- {} ({} match(es)) ---",
                    file.path.display(),
                    file.matches.len()
                )?;
            }
            for m in &file.matches {
                writeln!(stdout, "{}:{}:{}: {}", file.path.display(), m.line, m.column, m.snippet)?;
            }
        }
        writeln!(stdout)?;
    }

    let nfiles = plan.files.len();
    writeln!(
        stdout,
        "{} {} in {} {}, {} files scanned",
        plan.total_matches,
        if plan.total_matches == 1 { "match" } else { "matches" },
        nfiles,
        if nfiles == 1 { "file" } else { "files" },
        plan.files_scanned,
    )?;
    Ok(())
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

fn run_rename(cli: &Cli) -> Result<u8> {
    let map = match RenameMap::new(cli.rename_pairs()?) {
        Ok(map) => map,
        Err(err) => return handle_plan_error(err, cli.output.json),
    };
    if !map.rerunnable() && !cli.output.json && !cli.output.quiet {
        eprintln!(
            "recast: this map is correct exactly once — a replacement reuses a name the map \
             also renames, so running it again would keep rewriting."
        );
    }

    if cli.stdin {
        let mut buf = String::new();
        io::stdin().lock().read_to_string(&mut buf).context("read stdin")?;
        let outcome = map.rewrite(&buf);
        if let Err(err) = check_match_counts(outcome.matches, cli.min_matches(), cli.guard.at_most)
        {
            return handle_plan_error(err, cli.output.json);
        }
        io::stdout().lock().write_all(outcome.after.as_bytes()).context("write stdout")?;
        return Ok(EXIT_OK);
    }

    let pool = build_pool(cli.threads).context("configure worker thread pool")?;
    pool.install(|| {
        let paths = cli.positional_paths();
        let opts = cli.plan_options();
        let plan = match plan_rename(&map, &paths, &opts) {
            Ok(plan) => plan,
            Err(err) => return handle_plan_error(err, cli.output.json),
        };
        dispatch_plan(cli, &plan)
    })
}

fn acquire_workspace_lock_for(cli: &Cli) -> std::result::Result<Option<WorkspaceLock>, CoreError> {
    let writes_tree = cli.apply || cli.recover;
    if !writes_tree || cli.force || cli.stdin {
        return Ok(None);
    }
    let raw_paths: Vec<PathBuf> = if cli.recover || !cli.rename.is_empty() {
        cli.positional_paths()
    } else {
        cli.paths_as_pathbufs()
    };
    acquire_workspace_lock_for_paths(&raw_paths).map(Some)
}

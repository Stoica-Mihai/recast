mod completion;

use std::io::{self, Read, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Context, Result, anyhow};
use clap::{ArgAction, Parser};
use clap_complete::Shell;
use recast_core::{
    CompiledPattern, Error as CoreError, Language, PatternOptions, Plan, PlanOptions, PlanOutcome,
    ScriptRewriter, WalkOptions, apply_changes, build_pool, json, plan_rewrite,
    plan_rewrite_scripted, rewrite_text, rewrite_text_scripted, structural_rewrite,
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
    #[arg(required_unless_present = "completions")]
    pattern: Option<String>,

    /// Replacement template. $1, $2, ${name} interpolated unless --literal
    /// is set.
    #[arg(required_unless_present = "completions")]
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
    #[arg(long, value_name = "QUERY", requires = "lang")]
    query: Option<String>,

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
            at_least: Some(self.at_least.unwrap_or(1)),
            at_most: self.at_most,
            allow_non_convergent: self.allow_non_convergent,
            max_bytes: self.max_bytes,
            max_files: self.max_files,
        }
    }
}

fn main() -> ExitCode {
    init_tracing();
    let cli = Cli::parse();
    match run(cli) {
        Ok(code) => ExitCode::from(code),
        Err(err) => {
            eprintln!("recast: {err:#}");
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

    if let Some(lang_name) = &cli.lang {
        let query = cli.query.as_deref().ok_or_else(|| anyhow!("--query required with --lang"))?;
        let template = cli
            .replacement
            .as_deref()
            .ok_or_else(|| anyhow!("REPLACEMENT positional is the template in structural mode"))?;
        return run_structural(&cli, lang_name, query, template);
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

    let paths: Vec<PathBuf> = cli.paths.iter().map(PathBuf::from).collect();
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
        Err(err) => return Ok(handle_plan_error(err, cli.json)),
    };

    if cli.apply {
        emit_apply(&cli, &plan).context("emit apply output")?;
        let outcome = apply_changes(&plan).context("apply changes")?;
        if cli.json {
            println!("{}", json::from_apply(&plan, &outcome).to_line()?);
        }
        return Ok(EXIT_OK);
    }

    if cli.check {
        let would_change = plan.changes.len();
        if cli.json {
            println!("{}", json::from_check(&plan).to_line()?);
        }
        if matches!(plan.outcome, PlanOutcome::AlreadyApplied) || would_change == 0 {
            return Ok(EXIT_OK);
        }
        return Ok(EXIT_CHECK_WOULD_CHANGE);
    }

    emit_diff(&cli, &plan).context("emit diff output")?;
    Ok(EXIT_OK)
}

fn run_structural(cli: &Cli, lang_name: &str, query: &str, template: &str) -> Result<u8> {
    let lang =
        Language::from_name(lang_name).ok_or_else(|| anyhow!("unknown language `{lang_name}`"))?;

    if cli.stdin {
        let mut buf = String::new();
        io::stdin().lock().read_to_string(&mut buf).context("read stdin")?;
        let outcome = match structural_rewrite(lang, &buf, query, template) {
            Ok(o) => o,
            Err(e) => return Ok(handle_plan_error(e, cli.json)),
        };
        if let Some(min) = cli.at_least.or(Some(1))
            && outcome.matches < min
        {
            eprintln!(
                "recast: match-count guard violated: found {}, required at least {}",
                outcome.matches, min
            );
            return Ok(EXIT_GUARD_VIOLATED);
        }
        io::stdout().lock().write_all(outcome.text.as_bytes()).context("write stdout")?;
        return Ok(EXIT_OK);
    }

    let paths: Vec<PathBuf> = cli.paths.iter().map(PathBuf::from).collect();
    let walk_opts = cli.plan_options().walk_options;
    let files = recast_core::walk_paths(&paths, &walk_opts).context("walk paths")?;

    let mut total_matches = 0usize;
    let mut files_changed: Vec<(PathBuf, String, String, usize)> = Vec::new();
    for path in &files {
        let before = match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(e) if e.kind() == std::io::ErrorKind::InvalidData => continue,
            Err(e) => return Err(e).context(format!("read {}", path.display())),
        };
        let outcome = match structural_rewrite(lang, &before, query, template) {
            Ok(o) => o,
            Err(e) => return Ok(handle_plan_error(e, cli.json)),
        };
        if outcome.text == before {
            continue;
        }
        total_matches += outcome.matches;
        files_changed.push((path.clone(), before, outcome.text, outcome.matches));
    }

    if let Some(min) = cli.at_least.or(Some(1))
        && total_matches < min
    {
        return Ok(handle_plan_error(
            CoreError::TooFewMatches { found: total_matches, required: min },
            cli.json,
        ));
    }

    if cli.check {
        if cli.json {
            println!(
                r#"{{"kind":"check","outcome":"changes","files_scanned":{},"files_would_change":{},"total_matches":{}}}"#,
                files.len(),
                files_changed.len(),
                total_matches
            );
        }
        return Ok(if files_changed.is_empty() { EXIT_OK } else { EXIT_CHECK_WOULD_CHANGE });
    }

    if cli.apply {
        for (path, _before, after, _matches) in &files_changed {
            std::fs::write(path, after).context(format!("write {}", path.display()))?;
        }
        eprintln!("recast: applying {} file(s), {} match(es).", files_changed.len(), total_matches);
        return Ok(EXIT_OK);
    }

    let mut stdout = io::stdout().lock();
    for (path, before, after, _matches) in &files_changed {
        let label = recast_core::label_for_path(path);
        let diff = recast_core::unified_diff(&label, before, after);
        stdout.write_all(diff.as_bytes())?;
    }
    writeln!(
        stdout,
        "recast: {} file(s) would change, {} match(es) across {} scanned.",
        files_changed.len(),
        total_matches,
        files.len()
    )?;
    Ok(EXIT_OK)
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

    if let Some(min) = cli.at_least.or(Some(1))
        && outcome.matches < min
    {
        eprintln!(
            "recast: match-count guard violated: found {}, required at least {}",
            outcome.matches, min
        );
        return Ok(EXIT_GUARD_VIOLATED);
    }
    if let Some(max) = cli.at_most
        && outcome.matches > max
    {
        eprintln!(
            "recast: match-count guard violated: found {}, allowed at most {}",
            outcome.matches, max
        );
        return Ok(EXIT_GUARD_VIOLATED);
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

fn handle_plan_error(err: CoreError, as_json: bool) -> u8 {
    let code = match &err {
        CoreError::TooFewMatches { .. } | CoreError::TooManyMatches { .. } => EXIT_GUARD_VIOLATED,
        _ => EXIT_INTERNAL,
    };
    if as_json {
        if let Ok(line) = json::from_error(&err, code).to_line() {
            println!("{line}");
        }
    } else {
        eprintln!("recast: {err}");
    }
    code
}

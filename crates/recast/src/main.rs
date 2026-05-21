mod completion;

use std::io::{self, Read, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Context, Result, anyhow};
use clap::{ArgAction, Parser};
use clap_complete::Shell;
use recast_core::{
    CompiledPattern, Error as CoreError, PatternOptions, Plan, PlanOptions, PlanOutcome,
    WalkOptions, apply_changes, build_pool, json, plan_rewrite, rewrite_text,
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

    let pattern = cli.pattern.as_deref().ok_or_else(|| anyhow!("pattern required"))?;
    let replacement = cli.replacement.as_deref().ok_or_else(|| anyhow!("replacement required"))?;

    if cli.stdin {
        return run_stdin(&cli, pattern, replacement);
    }

    let paths: Vec<PathBuf> = cli.paths.iter().map(PathBuf::from).collect();
    let opts = cli.plan_options();
    let pool = build_pool(cli.threads).context("configure worker thread pool")?;

    let plan = match pool.install(|| plan_rewrite(pattern, replacement, &paths, &opts)) {
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

fn run_stdin(cli: &Cli, pattern: &str, replacement: &str) -> Result<u8> {
    let compiled = CompiledPattern::compile(pattern, replacement, &cli.pattern_options())
        .context("compile pattern")?;

    let mut buf = String::new();
    io::stdin().lock().read_to_string(&mut buf).context("read stdin")?;
    let outcome = rewrite_text(&compiled, &buf);

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

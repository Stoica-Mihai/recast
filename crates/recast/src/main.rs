use std::io::{self, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Context, Result};
use clap::{ArgAction, Parser};
use recast_core::{
    Error as CoreError, PatternOptions, Plan, PlanOptions, PlanOutcome, WalkOptions, apply_changes,
    plan_rewrite,
};
use serde::Serialize;

const EXIT_OK: u8 = 0;
const EXIT_CHECK_WOULD_CHANGE: u8 = 1;
const EXIT_GUARD_VIOLATED: u8 = 2;
const EXIT_INTERNAL: u8 = 3;

#[derive(Debug, Parser)]
#[command(name = "recast", about = "Safe, atomic, transparent multi-file text rewrites.", version)]
struct Cli {
    /// Regex pattern. Multi-line by default. Use --literal for plain-string
    /// matching.
    pattern: String,

    /// Replacement template. $1, $2, ${name} interpolated unless --literal
    /// is set.
    replacement: String,

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
}

impl Cli {
    fn plan_options(&self) -> PlanOptions {
        PlanOptions {
            pattern_options: PatternOptions {
                literal: self.literal,
                ignore_case: self.ignore_case,
                single_line: self.single_line,
            },
            walk_options: WalkOptions {
                hidden: self.hidden,
                no_ignore: self.no_ignore,
                follow_symlinks: false,
            },
            at_least: Some(self.at_least.unwrap_or(1)),
            at_most: self.at_most,
            allow_non_convergent: self.allow_non_convergent,
            max_bytes: self.max_bytes,
            max_files: self.max_files,
        }
    }
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum JsonOutput<'a> {
    Plan {
        outcome: PlanOutcome,
        files_scanned: usize,
        files_changed: usize,
        total_matches: usize,
        changes: Vec<JsonFile<'a>>,
    },
    Apply {
        outcome: PlanOutcome,
        files_scanned: usize,
        files_written: usize,
        total_matches: usize,
    },
    Check {
        outcome: PlanOutcome,
        files_scanned: usize,
        files_would_change: usize,
        total_matches: usize,
    },
}

#[derive(Serialize)]
struct JsonFile<'a> {
    path: &'a std::path::Path,
    matches: usize,
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
    let paths: Vec<PathBuf> = cli.paths.iter().map(PathBuf::from).collect();
    let opts = cli.plan_options();

    let plan = match plan_rewrite(&cli.pattern, &cli.replacement, &paths, &opts) {
        Ok(plan) => plan,
        Err(err) => return Ok(handle_plan_error(err, cli.json)),
    };

    if cli.apply {
        emit_apply(&cli, &plan).context("emit apply output")?;
        let outcome = apply_changes(&plan).context("apply changes")?;
        if cli.json {
            let json = JsonOutput::Apply {
                outcome: plan.outcome,
                files_scanned: plan.files_scanned,
                files_written: outcome.files_written,
                total_matches: outcome.total_matches,
            };
            println!("{}", serde_json::to_string(&json)?);
        }
        return Ok(EXIT_OK);
    }

    if cli.check {
        let would_change = plan.changes.len();
        if cli.json {
            let json = JsonOutput::Check {
                outcome: plan.outcome,
                files_scanned: plan.files_scanned,
                files_would_change: would_change,
                total_matches: plan.total_matches,
            };
            println!("{}", serde_json::to_string(&json)?);
        }
        if matches!(plan.outcome, PlanOutcome::AlreadyApplied) || would_change == 0 {
            return Ok(EXIT_OK);
        }
        return Ok(EXIT_CHECK_WOULD_CHANGE);
    }

    emit_diff(&cli, &plan).context("emit diff output")?;
    Ok(EXIT_OK)
}

fn emit_diff(cli: &Cli, plan: &Plan) -> Result<()> {
    if cli.json {
        let json = JsonOutput::Plan {
            outcome: plan.outcome,
            files_scanned: plan.files_scanned,
            files_changed: plan.changes.len(),
            total_matches: plan.total_matches,
            changes: plan
                .changes
                .iter()
                .map(|c| JsonFile { path: c.path.as_path(), matches: c.matches })
                .collect(),
        };
        println!("{}", serde_json::to_string(&json)?);
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

fn handle_plan_error(err: CoreError, json: bool) -> u8 {
    let (code, message) = match &err {
        CoreError::TooFewMatches { .. } | CoreError::TooManyMatches { .. } => {
            (EXIT_GUARD_VIOLATED, err.to_string())
        }
        _ => (EXIT_INTERNAL, err.to_string()),
    };
    if json {
        #[derive(Serialize)]
        struct ErrJson<'a> {
            kind: &'static str,
            error: &'a str,
        }
        if let Ok(s) = serde_json::to_string(&ErrJson { kind: "error", error: &message }) {
            println!("{s}");
        }
    } else {
        eprintln!("recast: {message}");
    }
    code
}

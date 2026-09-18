#![allow(clippy::unwrap_used)]

use std::path::PathBuf;

use super::*;

/// One of every variant whose message text this crate authors.
///
/// `InvalidRegex`, `InvalidGlob` and `Walk` forward another crate's
/// `Display`, so their wording is not ours to police; `remedies()` is
/// an exhaustive match, which is what forces a new variant to be
/// considered here at all.
fn authored_messages() -> Vec<Error> {
    let p = || PathBuf::from("src/a.rs");
    vec![
        Error::Io { path: p(), source: std::io::Error::other("boom") },
        Error::FileTooLarge { path: p(), size: 20_000_000, limit: 10_485_760 },
        Error::TooManyFiles { count: 1500, limit: 1000 },
        Error::NonConvergentReplacement { path: p(), extra: 3 },
        Error::NonConvergentContext { path: p(), extra: 1 },
        Error::NonConvergentScript { path: p(), extra: 2 },
        Error::InvalidRenameMap { reason: "no renames given".to_owned() },
        Error::RenameMapDiverges { key: "Foo".to_owned(), value: "Foo Bar".to_owned(), rounds: 64 },
        Error::TooFewMatches { found: 0, required: 1 },
        Error::TooManyMatches { found: 5, allowed: 3 },
        Error::ScriptParse("bad".to_owned()),
        Error::ScriptRuntime("bad".to_owned()),
        Error::UnknownLanguage("klingon".to_owned()),
        Error::StructuralQuery("bad".to_owned()),
        Error::StructuralTemplate("bad".to_owned()),
        Error::StructuralParse,
        Error::SyntaxRegression { path: p(), lang: "rust", new_errors: 2 },
        Error::Locked { path: PathBuf::from("/run/x.lock"), root: p() },
        Error::InvalidThreads,
        Error::ThreadPool("bad".to_owned()),
    ]
}

/// The gate. Core does not know which front end it is serving, so a
/// message naming `--word` is wrong for the MCP server and a message
/// naming `allow_syntax_errors` is wrong for the CLI. Both spellings
/// used to appear; the knob belongs in `remedies()` instead.
#[test]
fn core_messages_never_name_a_front_end_flag() {
    const MCP_ARGS: [&str; 9] = [
        "at_least",
        "at_most",
        "max_bytes",
        "max_files",
        "allow_non_convergent",
        "allow_syntax_errors",
        "script_source",
        "script_path",
        "follow_symlinks",
    ];
    for err in authored_messages() {
        let msg = err.to_string();
        assert!(!msg.contains("--"), "{:?} names a CLI flag: {msg}", err.kind());
        for arg in MCP_ARGS {
            assert!(!msg.contains(arg), "{:?} names an MCP argument: {msg}", err.kind());
        }
    }
}

#[test]
fn every_authored_message_is_non_empty() {
    for err in authored_messages() {
        assert!(!err.to_string().trim().is_empty(), "{:?} has no message", err.kind());
    }
}

#[test]
fn the_convergence_errors_offer_the_knobs_that_actually_help() {
    let by_replacement = Error::NonConvergentReplacement { path: PathBuf::new(), extra: 1 };
    assert_eq!(by_replacement.remedies(), &[Remedy::Word, Remedy::AllowNonConvergent]);

    // Word boundaries cannot fix a self-overlapping pattern, so
    // offering them here would be advice that does not work.
    let by_context = Error::NonConvergentContext { path: PathBuf::new(), extra: 1 };
    assert_eq!(by_context.remedies(), &[Remedy::AllowNonConvergent]);
}

#[test]
fn guard_violations_point_at_their_own_bound() {
    assert_eq!(Error::TooFewMatches { found: 0, required: 1 }.remedies(), &[Remedy::AtLeast]);
    assert_eq!(Error::TooManyMatches { found: 5, allowed: 3 }.remedies(), &[Remedy::AtMost]);
}

#[test]
fn a_held_lock_offers_only_the_force_knob() {
    let locked = Error::Locked { path: PathBuf::new(), root: PathBuf::new() };
    assert_eq!(locked.remedies(), &[Remedy::ForceLock]);
}

#[test]
fn errors_the_caller_must_fix_themselves_offer_no_knob() {
    for err in [
        Error::InvalidRenameMap { reason: "x".to_owned() },
        Error::RenameMapDiverges { key: "a".to_owned(), value: "b".to_owned(), rounds: 64 },
        Error::StructuralParse,
        Error::UnknownLanguage("klingon".to_owned()),
    ] {
        assert!(err.remedies().is_empty(), "{:?} offers a knob it has none for", err.kind());
    }
}

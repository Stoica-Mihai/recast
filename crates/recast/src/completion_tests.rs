#![allow(clippy::unwrap_used)]

use clap_complete::Shell;

use super::*;

fn render(shell: Shell) -> String {
    let mut buf = Vec::new();
    print(shell, &mut buf);
    String::from_utf8(buf).unwrap()
}

#[test]
fn bash_completion_names_the_binary() {
    let s = render(Shell::Bash);
    assert!(!s.is_empty());
    assert!(s.contains("recast"));
}

#[test]
fn zsh_completion_names_the_binary() {
    let s = render(Shell::Zsh);
    assert!(!s.is_empty());
    assert!(s.contains("recast"));
}

#[test]
fn fish_completion_names_the_binary() {
    let s = render(Shell::Fish);
    assert!(!s.is_empty());
    assert!(s.contains("recast"));
}

#[test]
fn powershell_completion_names_the_binary() {
    let s = render(Shell::PowerShell);
    assert!(!s.is_empty());
    assert!(s.contains("recast"));
}

#[test]
fn bash_completion_mentions_a_known_flag() {
    let s = render(Shell::Bash);
    assert!(s.contains("--apply"), "missing --apply in bash completion:\n{s}");
}

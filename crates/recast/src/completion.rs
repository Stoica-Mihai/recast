use std::io::Write;

use clap::CommandFactory;
use clap_complete::Shell;

use crate::Cli;

pub fn print<W: Write>(shell: Shell, writer: &mut W) {
    let mut cmd = Cli::command();
    clap_complete::generate(shell, &mut cmd, "recast", writer);
}

#[cfg(test)]
#[path = "completion_tests.rs"]
mod tests;

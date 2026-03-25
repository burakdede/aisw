pub mod activation;
pub mod auth;
pub mod backup;
pub mod cli;
pub mod commands;
pub mod config;
pub mod next_steps;
pub mod profile;
pub mod tool_detection;
pub mod types;

/// Serializes child-process spawning across all unit tests.
///
/// On Linux, `fork()` copies the parent's entire file-descriptor table into the
/// child before `execve()` replaces the process image.  If another test thread
/// has a file open with `O_WRONLY` at that instant, the forked child inherits
/// that write fd.  The kernel then sees the target executable "open for writing"
/// and returns `ETXTBSY` — even though the write fd belongs to a completely
/// different file in a different test.
///
/// Holding this lock for the duration of any test that (a) writes an executable
/// file and immediately (b) spawns or (c) execs it prevents the race.
#[cfg(test)]
pub(crate) static SPAWN_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

use anyhow::Result;
use clap::Parser;

pub fn run() -> Result<()> {
    let cli = cli::Cli::parse();
    commands::dispatch(cli)
}

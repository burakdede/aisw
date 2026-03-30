pub mod auth;
pub mod backup;
pub mod cli;
pub mod commands;
pub mod config;
pub mod live_apply;
pub mod output;
pub mod profile;
pub mod runtime;
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

pub fn run() -> Result<()> {
    let argv: Vec<std::ffi::OsString> = std::env::args_os().collect();
    let clap_no_color = cli::preparse_no_color(&argv);
    let cli = cli::parse_from(argv, clap_no_color).unwrap_or_else(|err| err.exit());
    runtime::configure(cli.non_interactive, cli.quiet);
    output::configure(cli.no_color, cli.quiet);
    commands::dispatch(cli)
}

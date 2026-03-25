pub mod auth;
pub mod backup;
pub mod cli;
pub mod commands;
pub mod config;
pub mod profile;
pub mod tool_detection;
pub mod types;

use anyhow::Result;
use clap::Parser;

pub fn run() -> Result<()> {
    let cli = cli::Cli::parse();
    commands::dispatch(cli)
}

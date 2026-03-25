use anyhow::{Context, Result};

use crate::cli::{Cli, Command};
use crate::config::ConfigStore;

pub mod add;
pub mod backup;
pub mod init;
pub mod list;
pub mod remove;
pub mod rename;
pub mod shell_hook;
pub mod status;
pub mod use_;

pub fn dispatch(cli: Cli) -> Result<()> {
    let home = ConfigStore::aisw_home()?;
    match cli.command {
        Command::Add(args) => add::run(args, &home)?,
        Command::Use(args) => use_::run(args, &home)?,
        Command::List(args) => list::run(args, &home)?,
        Command::Remove(args) => remove::run(args, &home)?,
        Command::Rename(args) => rename::run(args, &home)?,
        Command::Status(args) => status::run(args, &home)?,
        Command::Init(args) => {
            let user_home = dirs::home_dir().context("could not determine home directory")?;
            let shell_env = std::env::var("SHELL").ok();
            init::run_inner(&home, &user_home, shell_env.as_deref(), args.yes)?;
        }
        Command::ShellHook(args) => shell_hook::run(args)?,
        Command::Backup(args) => backup::run(args.command, &home)?,
    }
    Ok(())
}

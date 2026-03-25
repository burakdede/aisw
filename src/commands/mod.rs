use anyhow::Result;

use crate::cli::{Cli, Command};
use crate::config::ConfigStore;

pub mod add;
pub mod list;
pub mod remove;
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
        Command::Status(args) => status::run(args, &home)?,
        Command::Init => todo!(),
        Command::ShellHook(args) => shell_hook::run(args)?,
        Command::Backup(_) => todo!(),
    }
    Ok(())
}

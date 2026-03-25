use anyhow::Result;

use crate::cli::{Cli, Command};
use crate::config::ConfigStore;

pub mod add;
pub mod list;
pub mod use_;

pub fn dispatch(cli: Cli) -> Result<()> {
    let home = ConfigStore::aisw_home()?;
    match cli.command {
        Command::Add(args) => add::run(args, &home)?,
        Command::Use(args) => use_::run(args, &home)?,
        Command::List(args) => list::run(args, &home)?,
        Command::Remove(_) => todo!(),
        Command::Status(_) => todo!(),
        Command::Init => todo!(),
        Command::ShellHook(_) => todo!(),
        Command::Backup(_) => todo!(),
    }
    Ok(())
}

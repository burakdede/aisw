use anyhow::Result;

use crate::cli::{Cli, Command};
use crate::config::ConfigStore;

pub mod add;

pub fn dispatch(cli: Cli) -> Result<()> {
    let home = ConfigStore::aisw_home()?;
    match cli.command {
        Command::Add(args) => add::run(args, &home)?,
        Command::Use(_) => todo!(),
        Command::List(_) => todo!(),
        Command::Remove(_) => todo!(),
        Command::Status(_) => todo!(),
        Command::Init => todo!(),
        Command::ShellHook(_) => todo!(),
        Command::Backup(_) => todo!(),
    }
    Ok(())
}

use anyhow::Result;

use crate::cli::{Cli, Command};

pub fn dispatch(cli: Cli) -> Result<()> {
    match cli.command {
        Command::Add(_) => todo!(),
        Command::Use(_) => todo!(),
        Command::List(_) => todo!(),
        Command::Remove(_) => todo!(),
        Command::Status(_) => todo!(),
        Command::Init => todo!(),
        Command::ShellHook(_) => todo!(),
        Command::Backup(_) => todo!(),
    }
}

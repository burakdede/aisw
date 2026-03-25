use anyhow::Result;

use crate::cli::{Shell, ShellHookArgs};

// Bash and zsh share POSIX-compatible function syntax.
const BASH_ZSH_HOOK: &str = "\
# Added by aisw \u{2014} do not edit manually
export AISW_SHELL_HOOK=1
aisw() {
  if [ \"$1\" = \"use\" ]; then
    eval \"$(command aisw \"$@\" --emit-env 2>/dev/null)\"
    command aisw \"$@\"
  else
    command aisw \"$@\"
  fi
}
";

pub fn run(args: ShellHookArgs) -> Result<()> {
    match args.shell {
        Shell::Bash | Shell::Zsh => print!("{}", BASH_ZSH_HOOK),
        Shell::Fish => unimplemented!("fish shell hook is not yet implemented (see AI-23)"),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::{Shell, ShellHookArgs};

    fn hook_args(shell: Shell) -> ShellHookArgs {
        ShellHookArgs { shell }
    }

    #[test]
    fn bash_hook_returns_ok() {
        assert!(run(hook_args(Shell::Bash)).is_ok());
    }

    #[test]
    fn zsh_hook_returns_ok() {
        assert!(run(hook_args(Shell::Zsh)).is_ok());
    }

    #[test]
    fn hook_contains_required_strings() {
        assert!(BASH_ZSH_HOOK.contains("AISW_SHELL_HOOK=1"));
        assert!(BASH_ZSH_HOOK.contains("aisw()"));
        assert!(BASH_ZSH_HOOK.contains("--emit-env"));
        assert!(BASH_ZSH_HOOK.contains("export AISW_SHELL_HOOK=1"));
    }

    #[test]
    fn hook_intercepts_use_subcommand() {
        assert!(BASH_ZSH_HOOK.contains("\"use\""));
        assert!(BASH_ZSH_HOOK.contains("--emit-env"));
    }
}

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

// Fish cannot eval POSIX `export KEY=VALUE`; parse line-by-line with set -gx.
const FISH_HOOK: &str = "\
# Added by aisw \u{2014} do not edit manually
set -gx AISW_SHELL_HOOK 1

function aisw
  if test \"$argv[1]\" = \"use\"
    set -l _aisw_env (command aisw $argv --emit-env 2>/dev/null)
    if test $status -ne 0
      command aisw $argv
      return $status
    end
    for line in $_aisw_env
      if string match -q 'unset *' -- $line
        set -l var (string replace 'unset ' '' -- $line)
        set -e $var
      else
        set -l parts (string replace 'export ' '' -- $line | string split '=' -m1)
        set -gx $parts[1] $parts[2]
      end
    end
    command aisw $argv
  else
    command aisw $argv
  end
end
";

pub fn run(args: ShellHookArgs) -> Result<()> {
    match args.shell {
        Shell::Bash | Shell::Zsh => print!("{}", BASH_ZSH_HOOK),
        Shell::Fish => print!("{}", FISH_HOOK),
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

    #[test]
    fn fish_hook_returns_ok() {
        assert!(run(hook_args(Shell::Fish)).is_ok());
    }

    #[test]
    fn fish_hook_contains_required_strings() {
        assert!(FISH_HOOK.contains("AISW_SHELL_HOOK"));
        assert!(FISH_HOOK.contains("set -gx AISW_SHELL_HOOK 1"));
        assert!(FISH_HOOK.contains("function aisw"));
        assert!(FISH_HOOK.contains("--emit-env"));
        assert!(FISH_HOOK.contains("set -gx"));
    }

    #[test]
    fn fish_hook_intercepts_use_subcommand() {
        assert!(FISH_HOOK.contains("\"use\""));
        assert!(FISH_HOOK.contains("--emit-env"));
    }

    #[test]
    fn fish_hook_parses_env_lines() {
        assert!(FISH_HOOK.contains("string replace"));
        assert!(FISH_HOOK.contains("string split"));
        assert!(FISH_HOOK.contains("unset "));
        assert!(FISH_HOOK.contains("set -e"));
    }
}

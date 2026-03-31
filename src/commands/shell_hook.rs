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

// Fish cannot eval POSIX `export KEY=VALUE`. aisw detects Fish via FISH_VERSION
// and emits native `set -gx` / `set -e` lines, so the hook just evals them directly.
const FISH_HOOK: &str = "\
# Added by aisw \u{2014} do not edit manually
set -gx AISW_SHELL_HOOK 1

function aisw
  if test \"$argv[1]\" = \"use\"
    eval (command aisw $argv --emit-env 2>/dev/null)
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
    fn fish_hook_evals_native_fish_output() {
        // The hook should use eval directly — aisw emits native `set -gx` / `set -e`
        // when FISH_VERSION is set, so no POSIX line-by-line parsing is needed.
        assert!(FISH_HOOK.contains("eval"));
        assert!(!FISH_HOOK.contains("string replace"));
        assert!(!FISH_HOOK.contains("string split"));
        assert!(!FISH_HOOK.contains("string unescape"));
    }
}

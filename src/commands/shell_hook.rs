use anyhow::Result;

use crate::cli::{Shell, ShellHookArgs};

const BASH_ZSH_HOOK: &str = "\
# Added by aisw - do not edit manually
export AISW_SHELL_HOOK=1
__aisw_prompt_check() {
  if [ \"${PWD-}\" != \"${AISW_LAST_PWD-}\" ]; then
    AISW_LAST_PWD=\"$PWD\"
    command aisw workspace check --prompt
  fi
}
__aisw_install_prompt_hook() {
  if [ -n \"${ZSH_VERSION-}\" ]; then
    typeset -ga precmd_functions chpwd_functions
    case \" ${precmd_functions[*]} \" in
      *\" __aisw_prompt_check \"*) ;;
      *) precmd_functions+=(__aisw_prompt_check) ;;
    esac
    case \" ${chpwd_functions[*]} \" in
      *\" __aisw_prompt_check \"*) ;;
      *) chpwd_functions+=(__aisw_prompt_check) ;;
    esac
  else
    case \";${PROMPT_COMMAND-};\" in
      *\";__aisw_prompt_check;\"*) ;;
      *) PROMPT_COMMAND=\"__aisw_prompt_check${PROMPT_COMMAND:+;$PROMPT_COMMAND}\" ;;
    esac
  fi
}
aisw() {
  if [ \"$1\" = \"use\" ] || { [ \"$1\" = \"context\" ] && [ \"$2\" = \"use\" ]; }; then
    eval \"$(command aisw \"$@\" --emit-env 2>/dev/null)\"
    command aisw \"$@\"
    __aisw_prompt_check
  else
    command aisw \"$@\"
  fi
}
claude() {
  command aisw workspace check --tool claude || return $?
  command claude \"$@\"
}
codex() {
  command aisw workspace check --tool codex || return $?
  command codex \"$@\"
}
gemini() {
  command aisw workspace check --tool gemini || return $?
  command gemini \"$@\"
}
__aisw_install_prompt_hook
";

// Fish cannot eval POSIX `export KEY=VALUE`. aisw detects Fish via FISH_VERSION
// and emits native `set -gx` / `set -e` lines, so the hook just evals them directly.
const FISH_HOOK: &str = "\
# Added by aisw - do not edit manually
set -gx AISW_SHELL_HOOK 1
set -gx AISW_SHELL fish
set -g AISW_LAST_PWD ''

function __aisw_prompt_check --on-variable PWD
  if test \"$PWD\" != \"$AISW_LAST_PWD\"
    set -g AISW_LAST_PWD \"$PWD\"
    command aisw workspace check --prompt
  end
end

function aisw
  if test \"$argv[1]\" = \"use\"; or begin; test \"$argv[1]\" = \"context\"; and test \"$argv[2]\" = \"use\"; end
    eval (command aisw $argv --emit-env 2>/dev/null)
    command aisw $argv
    __aisw_prompt_check
  else
    command aisw $argv
  end
end

function claude
  command aisw workspace check --tool claude
  or return $status
  command claude $argv
end

function codex
  command aisw workspace check --tool codex
  or return $status
  command codex $argv
end

function gemini
  command aisw workspace check --tool gemini
  or return $status
  command gemini $argv
end
";

const POWERSHELL_HOOK: &str = r#"
# Added by aisw - do not edit manually
$env:AISW_SHELL_HOOK = '1'
$env:AISW_SHELL = 'pwsh'
function global:__aisw_get_command_path([string]$Name) {
  $cmd = Get-Command $Name -All | Where-Object { $_.CommandType -eq 'Application' } | Select-Object -First 1
  if ($null -eq $cmd) {
    throw "Could not find application '$Name' on PATH."
  }
  $cmd.Source
}
function global:__aisw_bin {
  __aisw_get_command_path 'aisw'
}
function global:__aisw_prompt_check {
  $cwd = $PWD.Path
  if ($global:AISW_LAST_PWD -ne $cwd) {
    $global:AISW_LAST_PWD = $cwd
    & (__aisw_bin) workspace check --prompt
  }
}
if (-not (Test-Path Function:\global:__aisw_original_prompt)) {
  if (Test-Path Function:\prompt) {
    Copy-Item Function:\prompt Function:\global:__aisw_original_prompt
  }
}
function global:prompt {
  __aisw_prompt_check
  if (Test-Path Function:\global:__aisw_original_prompt) {
    & $function:__aisw_original_prompt
  } else {
    "PS $($executionContext.SessionState.Path.CurrentLocation)$('>' * ($nestedPromptLevel + 1)) "
  }
}
function global:aisw {
  param([Parameter(ValueFromRemainingArguments = $true)][string[]]$ArgsRest)
  $aiswBin = __aisw_bin
  if ($ArgsRest.Length -gt 0 -and ($ArgsRest[0] -eq 'use' -or ($ArgsRest.Length -gt 1 -and $ArgsRest[0] -eq 'context' -and $ArgsRest[1] -eq 'use'))) {
    $envScript = & $aiswBin @ArgsRest --emit-env 2>$null | Out-String
    if (-not [string]::IsNullOrWhiteSpace($envScript)) {
      Invoke-Expression $envScript
    }
    & $aiswBin @ArgsRest
    __aisw_prompt_check
  } else {
    & $aiswBin @ArgsRest
  }
}
function global:claude {
  param([Parameter(ValueFromRemainingArguments = $true)][string[]]$ArgsRest)
  & (__aisw_bin) workspace check --tool claude
  if ($LASTEXITCODE -ne 0) { return }
  & (__aisw_get_command_path 'claude') @ArgsRest
}
function global:codex {
  param([Parameter(ValueFromRemainingArguments = $true)][string[]]$ArgsRest)
  & (__aisw_bin) workspace check --tool codex
  if ($LASTEXITCODE -ne 0) { return }
  & (__aisw_get_command_path 'codex') @ArgsRest
}
function global:gemini {
  param([Parameter(ValueFromRemainingArguments = $true)][string[]]$ArgsRest)
  & (__aisw_bin) workspace check --tool gemini
  if ($LASTEXITCODE -ne 0) { return }
  & (__aisw_get_command_path 'gemini') @ArgsRest
}
"#;

pub fn run(args: ShellHookArgs) -> Result<()> {
    match args.shell {
        Shell::Bash | Shell::Zsh => print!("{}", BASH_ZSH_HOOK),
        Shell::Fish => print!("{}", FISH_HOOK),
        Shell::Pwsh => print!("{}", POWERSHELL_HOOK),
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
        assert!(BASH_ZSH_HOOK.contains("workspace check --tool claude"));
    }

    #[test]
    fn hook_intercepts_use_subcommand() {
        assert!(BASH_ZSH_HOOK.contains("\"use\""));
        assert!(BASH_ZSH_HOOK.contains("\"context\""));
        assert!(BASH_ZSH_HOOK.contains("--emit-env"));
    }

    #[test]
    fn fish_hook_returns_ok() {
        assert!(run(hook_args(Shell::Fish)).is_ok());
    }

    #[test]
    fn pwsh_hook_returns_ok() {
        assert!(run(hook_args(Shell::Pwsh)).is_ok());
    }

    #[test]
    fn fish_hook_contains_required_strings() {
        assert!(FISH_HOOK.contains("AISW_SHELL_HOOK"));
        assert!(FISH_HOOK.contains("set -gx AISW_SHELL_HOOK 1"));
        assert!(FISH_HOOK.contains("function aisw"));
        assert!(FISH_HOOK.contains("--emit-env"));
        assert!(FISH_HOOK.contains("set -gx"));
        assert!(FISH_HOOK.contains("workspace check --tool claude"));
    }

    #[test]
    fn fish_hook_intercepts_use_subcommand() {
        assert!(FISH_HOOK.contains("\"use\""));
        assert!(FISH_HOOK.contains("\"context\""));
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

    #[test]
    fn pwsh_hook_contains_required_strings() {
        assert!(POWERSHELL_HOOK.contains("$env:AISW_SHELL = 'pwsh'"));
        assert!(POWERSHELL_HOOK.contains("function global:aisw"));
        assert!(POWERSHELL_HOOK.contains("--emit-env"));
        assert!(POWERSHELL_HOOK.contains("workspace check --tool claude"));
    }
}

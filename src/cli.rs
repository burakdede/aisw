use clap::{Args, Parser, Subcommand, ValueEnum};

use crate::types::Tool;

#[derive(Parser, Debug)]
#[command(
    name = "aisw",
    about = "Manage multiple accounts for Claude Code, Codex CLI, and Gemini CLI",
    long_about = None,
    version,
    propagate_version = true,
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Add a new account profile for a tool
    Add(AddArgs),

    /// Switch the active account for a tool
    #[command(name = "use")]
    Use(UseArgs),

    /// Show all profiles, optionally filtered by tool
    List(ListArgs),

    /// Remove a stored profile
    Remove(RemoveArgs),

    /// Show current active profiles and credential status
    Status(StatusArgs),

    /// First-run setup: shell integration and credential import
    Init(InitArgs),

    /// Print the shell integration hook for the given shell
    #[command(name = "shell-hook")]
    ShellHook(ShellHookArgs),

    /// Manage credential backups
    Backup(BackupArgs),
}

#[derive(Args, Debug)]
pub struct AddArgs {
    /// Tool to add a profile for
    pub tool: Tool,

    /// Name for this profile (alphanumeric, hyphens, underscores, max 32 chars)
    pub profile_name: String,

    /// API key — skips the interactive auth method prompt
    #[arg(long, value_name = "KEY")]
    pub api_key: Option<String>,

    /// Human-readable label for this profile
    #[arg(long, value_name = "TEXT")]
    pub label: Option<String>,

    /// Switch to this profile immediately after adding
    #[arg(long)]
    pub set_active: bool,
}

#[derive(Args, Debug)]
pub struct UseArgs {
    /// Tool to switch
    pub tool: Tool,

    /// Profile to activate
    pub profile_name: String,

    /// Print shell export statements to stdout instead of applying them directly.
    /// Used internally by the shell hook — not intended for direct use.
    #[arg(long, hide = true)]
    pub emit_env: bool,
}

#[derive(Args, Debug)]
pub struct ListArgs {
    /// Filter output to a specific tool
    pub tool: Option<Tool>,

    /// Output as JSON
    #[arg(long)]
    pub json: bool,
}

#[derive(Args, Debug)]
pub struct RemoveArgs {
    /// Tool the profile belongs to
    pub tool: Tool,

    /// Profile to remove
    pub profile_name: String,

    /// Skip the confirmation prompt
    #[arg(long)]
    pub yes: bool,

    /// Allow removing the currently active profile
    #[arg(long)]
    pub force: bool,
}

#[derive(Args, Debug)]
pub struct StatusArgs {
    /// Output as JSON
    #[arg(long)]
    pub json: bool,
}

#[derive(Args, Debug)]
pub struct InitArgs {
    /// Answer yes to all confirmation prompts
    #[arg(long)]
    pub yes: bool,
}

#[derive(Args, Debug)]
pub struct ShellHookArgs {
    /// Shell to generate the hook for
    pub shell: Shell,
}

#[derive(ValueEnum, Clone, Debug, PartialEq)]
pub enum Shell {
    Bash,
    Zsh,
    Fish,
}

#[derive(Args, Debug)]
pub struct BackupArgs {
    #[command(subcommand)]
    pub command: BackupCommand,
}

#[derive(Subcommand, Debug)]
pub enum BackupCommand {
    /// List all credential backups
    List,

    /// Restore a backup by timestamp
    Restore {
        /// Timestamp of the backup to restore (from 'aisw backup list')
        timestamp: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<Cli, clap::Error> {
        Cli::try_parse_from(std::iter::once("aisw").chain(args.iter().copied()))
    }

    #[test]
    fn add_api_key() {
        let cli = parse(&["add", "claude", "work", "--api-key", "sk-ant-test"]).unwrap();
        let Command::Add(args) = cli.command else {
            panic!("wrong command")
        };
        assert_eq!(args.tool, Tool::Claude);
        assert_eq!(args.profile_name, "work");
        assert_eq!(args.api_key.as_deref(), Some("sk-ant-test"));
        assert!(!args.set_active);
    }

    #[test]
    fn add_with_label_and_set_active() {
        let cli = parse(&[
            "add",
            "codex",
            "personal",
            "--label",
            "my account",
            "--set-active",
        ])
        .unwrap();
        let Command::Add(args) = cli.command else {
            panic!("wrong command")
        };
        assert_eq!(args.tool, Tool::Codex);
        assert_eq!(args.label.as_deref(), Some("my account"));
        assert!(args.set_active);
    }

    #[test]
    fn use_command() {
        let cli = parse(&["use", "gemini", "work"]).unwrap();
        let Command::Use(args) = cli.command else {
            panic!("wrong command")
        };
        assert_eq!(args.tool, Tool::Gemini);
        assert_eq!(args.profile_name, "work");
        assert!(!args.emit_env);
    }

    #[test]
    fn use_emit_env_is_hidden_but_parseable() {
        let cli = parse(&["use", "claude", "work", "--emit-env"]).unwrap();
        let Command::Use(args) = cli.command else {
            panic!("wrong command")
        };
        assert!(args.emit_env);
    }

    #[test]
    fn list_no_filter() {
        let cli = parse(&["list"]).unwrap();
        let Command::List(args) = cli.command else {
            panic!("wrong command")
        };
        assert!(args.tool.is_none());
        assert!(!args.json);
    }

    #[test]
    fn list_with_tool_and_json() {
        let cli = parse(&["list", "codex", "--json"]).unwrap();
        let Command::List(args) = cli.command else {
            panic!("wrong command")
        };
        assert_eq!(args.tool, Some(Tool::Codex));
        assert!(args.json);
    }

    #[test]
    fn remove_flags() {
        let cli = parse(&["remove", "claude", "work", "--yes", "--force"]).unwrap();
        let Command::Remove(args) = cli.command else {
            panic!("wrong command")
        };
        assert_eq!(args.tool, Tool::Claude);
        assert!(args.yes);
        assert!(args.force);
    }

    #[test]
    fn status_json() {
        let cli = parse(&["status", "--json"]).unwrap();
        let Command::Status(args) = cli.command else {
            panic!("wrong command")
        };
        assert!(args.json);
    }

    #[test]
    fn init() {
        let cli = parse(&["init"]).unwrap();
        assert!(matches!(cli.command, Command::Init(_)));
    }

    #[test]
    fn init_yes_flag() {
        let cli = parse(&["init", "--yes"]).unwrap();
        let Command::Init(args) = cli.command else {
            panic!("wrong command")
        };
        assert!(args.yes);
    }

    #[test]
    fn shell_hook_variants() {
        for (input, expected) in [
            ("bash", Shell::Bash),
            ("zsh", Shell::Zsh),
            ("fish", Shell::Fish),
        ] {
            let cli = parse(&["shell-hook", input]).unwrap();
            let Command::ShellHook(args) = cli.command else {
                panic!("wrong command")
            };
            assert_eq!(args.shell, expected);
        }
    }

    #[test]
    fn backup_list() {
        let cli = parse(&["backup", "list"]).unwrap();
        let Command::Backup(args) = cli.command else {
            panic!("wrong command")
        };
        assert!(matches!(args.command, BackupCommand::List));
    }

    #[test]
    fn backup_restore() {
        let cli = parse(&["backup", "restore", "2026-03-25T10-00-00Z"]).unwrap();
        let Command::Backup(args) = cli.command else {
            panic!("wrong command")
        };
        let BackupCommand::Restore { timestamp } = args.command else {
            panic!("wrong subcommand")
        };
        assert_eq!(timestamp, "2026-03-25T10-00-00Z");
    }

    #[test]
    fn invalid_tool_name() {
        assert!(parse(&["add", "chatgpt", "work"]).is_err());
    }

    #[test]
    fn invalid_subcommand() {
        assert!(parse(&["switch", "claude", "work"]).is_err());
    }

    #[test]
    fn emit_env_absent_from_use_help() {
        let help = Cli::try_parse_from(["aisw", "use", "--help"])
            .unwrap_err()
            .to_string();
        assert!(
            !help.contains("emit-env"),
            "emit-env should be hidden from help"
        );
    }
}

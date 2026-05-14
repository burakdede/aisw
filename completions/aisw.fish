# Print an optspec for argparse to handle cmd's options that are independent of any subcommand.
function __fish_aisw_global_optspecs
	string join \n h/help V/version
end

function __fish_aisw_needs_command
	# Figure out if the current invocation already has a command.
	set -l cmd (commandline -opc)
	set -e cmd[1]
	argparse -s (__fish_aisw_global_optspecs) -- $cmd 2>/dev/null
	or return
	if set -q argv[1]
		# Also print the command, so this can be used to figure out what it is.
		echo $argv[1]
		return 1
	end
	return 0
end

function __fish_aisw_using_subcommand
	set -l cmd (__fish_aisw_needs_command)
	test -z "$cmd"
	and return 1
	contains -- $cmd[1] $argv
end

complete -c aisw -n "__fish_aisw_needs_command" -s h -l help -d 'Print help'
complete -c aisw -n "__fish_aisw_needs_command" -s V -l version -d 'Print version'
complete -c aisw -n "__fish_aisw_needs_command" -f -a "add" -d 'Add a new account profile for a tool'
complete -c aisw -n "__fish_aisw_needs_command" -f -a "context" -d 'Manage named multi-tool contexts'
complete -c aisw -n "__fish_aisw_needs_command" -f -a "use" -d 'Switch the active account for a tool'
complete -c aisw -n "__fish_aisw_needs_command" -f -a "list" -d 'Show all profiles, optionally filtered by tool'
complete -c aisw -n "__fish_aisw_needs_command" -f -a "remove" -d 'Remove a stored profile'
complete -c aisw -n "__fish_aisw_needs_command" -f -a "rename" -d 'Rename a stored profile'
complete -c aisw -n "__fish_aisw_needs_command" -f -a "status" -d 'Show current active profiles and credential status'
complete -c aisw -n "__fish_aisw_needs_command" -f -a "init" -d 'First-run setup: shell integration and credential import'
complete -c aisw -n "__fish_aisw_needs_command" -f -a "shell-hook" -d 'Print the shell integration hook for the given shell'
complete -c aisw -n "__fish_aisw_needs_command" -f -a "backup" -d 'Manage credential backups'
complete -c aisw -n "__fish_aisw_needs_command" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c aisw -n "__fish_aisw_using_subcommand add" -l api-key -d 'API key — skips the interactive auth method prompt' -r
complete -c aisw -n "__fish_aisw_using_subcommand add" -l label -d 'Human-readable label for this profile' -r
complete -c aisw -n "__fish_aisw_using_subcommand add" -l set-active -d 'Switch to this profile immediately after adding'
complete -c aisw -n "__fish_aisw_using_subcommand add" -l from-env -d 'Use the tool\'s standard environment variable instead of interactive prompts'
complete -c aisw -n "__fish_aisw_using_subcommand add" -l from-live -d 'Capture live credentials from the tool\'s current config without launching a login flow'
complete -c aisw -n "__fish_aisw_using_subcommand add" -l yes -d 'Overwrite an existing profile without prompting (only used with --from-live)'
complete -c aisw -n "__fish_aisw_using_subcommand add" -s h -l help -d 'Print help'
complete -c aisw -n "__fish_aisw_using_subcommand add" -s V -l version -d 'Print version'
complete -c aisw -n "__fish_aisw_using_subcommand context; and not __fish_seen_subcommand_from create list use set unset remove rename" -f -a "create" -d 'Create a named context that maps tools to profiles'
complete -c aisw -n "__fish_aisw_using_subcommand context; and not __fish_seen_subcommand_from create list use set unset remove rename" -f -a "list" -d 'Show saved contexts'
complete -c aisw -n "__fish_aisw_using_subcommand context; and not __fish_seen_subcommand_from create list use set unset remove rename" -f -a "use" -d 'Activate a saved context'
complete -c aisw -n "__fish_aisw_using_subcommand context; and not __fish_seen_subcommand_from create list use set unset remove rename" -f -a "set" -d 'Set or replace tool mappings in an existing context'
complete -c aisw -n "__fish_aisw_using_subcommand context; and not __fish_seen_subcommand_from create list use set unset remove rename" -f -a "unset" -d 'Remove tool mappings from an existing context'
complete -c aisw -n "__fish_aisw_using_subcommand context; and not __fish_seen_subcommand_from create list use set unset remove rename" -f -a "remove" -d 'Remove a saved context'
complete -c aisw -n "__fish_aisw_using_subcommand context; and not __fish_seen_subcommand_from create list use set unset remove rename" -f -a "rename" -d 'Rename a saved context'
complete -c aisw -n "__fish_aisw_using_subcommand context; and __fish_seen_subcommand_from create" -l claude -r -d 'Claude profile to include'
complete -c aisw -n "__fish_aisw_using_subcommand context; and __fish_seen_subcommand_from create" -l codex -r -d 'Codex profile to include'
complete -c aisw -n "__fish_aisw_using_subcommand context; and __fish_seen_subcommand_from create" -l gemini -r -d 'Gemini profile to include'
complete -c aisw -n "__fish_aisw_using_subcommand context; and __fish_seen_subcommand_from create" -s h -l help -d 'Print help'
complete -c aisw -n "__fish_aisw_using_subcommand context; and __fish_seen_subcommand_from create" -s V -l version -d 'Print version'
complete -c aisw -n "__fish_aisw_using_subcommand context; and __fish_seen_subcommand_from list" -l search -r -d 'Filter by context name or mapped profile text'
complete -c aisw -n "__fish_aisw_using_subcommand context; and __fish_seen_subcommand_from list" -l json -d 'Output as JSON'
complete -c aisw -n "__fish_aisw_using_subcommand context; and __fish_seen_subcommand_from list" -s h -l help -d 'Print help'
complete -c aisw -n "__fish_aisw_using_subcommand context; and __fish_seen_subcommand_from list" -s V -l version -d 'Print version'
complete -c aisw -n "__fish_aisw_using_subcommand context; and __fish_seen_subcommand_from use" -l state-mode -r -a "isolated shared" -d 'Choose isolated or shared state mode'
complete -c aisw -n "__fish_aisw_using_subcommand context; and __fish_seen_subcommand_from use" -l emit-env -d 'Print shell export statements to stdout'
complete -c aisw -n "__fish_aisw_using_subcommand context; and __fish_seen_subcommand_from use" -s h -l help -d 'Print help'
complete -c aisw -n "__fish_aisw_using_subcommand context; and __fish_seen_subcommand_from use" -s V -l version -d 'Print version'
complete -c aisw -n "__fish_aisw_using_subcommand context; and __fish_seen_subcommand_from set" -l claude -r -d 'Claude profile to set'
complete -c aisw -n "__fish_aisw_using_subcommand context; and __fish_seen_subcommand_from set" -l codex -r -d 'Codex profile to set'
complete -c aisw -n "__fish_aisw_using_subcommand context; and __fish_seen_subcommand_from set" -l gemini -r -d 'Gemini profile to set'
complete -c aisw -n "__fish_aisw_using_subcommand context; and __fish_seen_subcommand_from set" -s h -l help -d 'Print help'
complete -c aisw -n "__fish_aisw_using_subcommand context; and __fish_seen_subcommand_from set" -s V -l version -d 'Print version'
complete -c aisw -n "__fish_aisw_using_subcommand context; and __fish_seen_subcommand_from unset" -l claude -d 'Remove the Claude mapping'
complete -c aisw -n "__fish_aisw_using_subcommand context; and __fish_seen_subcommand_from unset" -l codex -d 'Remove the Codex mapping'
complete -c aisw -n "__fish_aisw_using_subcommand context; and __fish_seen_subcommand_from unset" -l gemini -d 'Remove the Gemini mapping'
complete -c aisw -n "__fish_aisw_using_subcommand context; and __fish_seen_subcommand_from unset" -s h -l help -d 'Print help'
complete -c aisw -n "__fish_aisw_using_subcommand context; and __fish_seen_subcommand_from unset" -s V -l version -d 'Print version'
complete -c aisw -n "__fish_aisw_using_subcommand context; and __fish_seen_subcommand_from remove" -l yes -d 'Skip the confirmation prompt'
complete -c aisw -n "__fish_aisw_using_subcommand context; and __fish_seen_subcommand_from remove" -s h -l help -d 'Print help'
complete -c aisw -n "__fish_aisw_using_subcommand context; and __fish_seen_subcommand_from remove" -s V -l version -d 'Print version'
complete -c aisw -n "__fish_aisw_using_subcommand context; and __fish_seen_subcommand_from rename" -s h -l help -d 'Print help'
complete -c aisw -n "__fish_aisw_using_subcommand context; and __fish_seen_subcommand_from rename" -s V -l version -d 'Print version'
complete -c aisw -n "__fish_aisw_using_subcommand use" -l state-mode -r -a "isolated shared" -d 'Choose isolated or shared state mode'
complete -c aisw -n "__fish_aisw_using_subcommand use" -l all -d 'Switch all tools to same-named profile in one command'
complete -c aisw -n "__fish_aisw_using_subcommand use" -l profile -r -d 'Profile name when using --all'
complete -c aisw -n "__fish_aisw_using_subcommand use" -l emit-env -d 'Print shell export statements to stdout instead of applying them directly. Used internally by the shell hook — not intended for direct use'
complete -c aisw -n "__fish_aisw_using_subcommand use" -s h -l help -d 'Print help'
complete -c aisw -n "__fish_aisw_using_subcommand use" -s V -l version -d 'Print version'
complete -c aisw -n "__fish_aisw_using_subcommand list" -l json -d 'Output as JSON'
complete -c aisw -n "__fish_aisw_using_subcommand list" -s h -l help -d 'Print help'
complete -c aisw -n "__fish_aisw_using_subcommand list" -s V -l version -d 'Print version'
complete -c aisw -n "__fish_aisw_using_subcommand remove" -l yes -d 'Skip the confirmation prompt'
complete -c aisw -n "__fish_aisw_using_subcommand remove" -l force -d 'Allow removing the currently active profile'
complete -c aisw -n "__fish_aisw_using_subcommand remove" -s h -l help -d 'Print help'
complete -c aisw -n "__fish_aisw_using_subcommand remove" -s V -l version -d 'Print version'
complete -c aisw -n "__fish_aisw_using_subcommand rename" -s h -l help -d 'Print help'
complete -c aisw -n "__fish_aisw_using_subcommand rename" -s V -l version -d 'Print version'
complete -c aisw -n "__fish_aisw_using_subcommand status" -l tool -r -a "claude codex gemini" -d 'Filter output to a specific tool'
complete -c aisw -n "__fish_aisw_using_subcommand status" -l search -r -d 'Filter by tool/profile/auth/backend text'
complete -c aisw -n "__fish_aisw_using_subcommand status" -l sort -r -a "name recent" -d 'Sort output rows'
complete -c aisw -n "__fish_aisw_using_subcommand status" -l active-only -d 'Show only tools with an active profile'
complete -c aisw -n "__fish_aisw_using_subcommand status" -l context -d 'Include derived context information'
complete -c aisw -n "__fish_aisw_using_subcommand status" -l json -d 'Output as JSON'
complete -c aisw -n "__fish_aisw_using_subcommand status" -s h -l help -d 'Print help'
complete -c aisw -n "__fish_aisw_using_subcommand status" -s V -l version -d 'Print version'
complete -c aisw -n "__fish_aisw_using_subcommand init" -l yes -d 'Answer yes to all confirmation prompts'
complete -c aisw -n "__fish_aisw_using_subcommand init" -s h -l help -d 'Print help'
complete -c aisw -n "__fish_aisw_using_subcommand init" -s V -l version -d 'Print version'
complete -c aisw -n "__fish_aisw_using_subcommand shell-hook" -s h -l help -d 'Print help'
complete -c aisw -n "__fish_aisw_using_subcommand shell-hook" -s V -l version -d 'Print version'
complete -c aisw -n "__fish_aisw_using_subcommand backup; and not __fish_seen_subcommand_from list restore help" -s h -l help -d 'Print help'
complete -c aisw -n "__fish_aisw_using_subcommand backup; and not __fish_seen_subcommand_from list restore help" -s V -l version -d 'Print version'
complete -c aisw -n "__fish_aisw_using_subcommand backup; and not __fish_seen_subcommand_from list restore help" -f -a "list" -d 'List all credential backups'
complete -c aisw -n "__fish_aisw_using_subcommand backup; and not __fish_seen_subcommand_from list restore help" -f -a "restore" -d 'Restore a backup by id'
complete -c aisw -n "__fish_aisw_using_subcommand backup; and not __fish_seen_subcommand_from list restore help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c aisw -n "__fish_aisw_using_subcommand backup; and __fish_seen_subcommand_from list" -s h -l help -d 'Print help'
complete -c aisw -n "__fish_aisw_using_subcommand backup; and __fish_seen_subcommand_from list" -s V -l version -d 'Print version'
complete -c aisw -n "__fish_aisw_using_subcommand backup; and __fish_seen_subcommand_from restore" -l yes -d 'Skip the confirmation prompt'
complete -c aisw -n "__fish_aisw_using_subcommand backup; and __fish_seen_subcommand_from restore" -s h -l help -d 'Print help'
complete -c aisw -n "__fish_aisw_using_subcommand backup; and __fish_seen_subcommand_from restore" -s V -l version -d 'Print version'
complete -c aisw -n "__fish_aisw_using_subcommand backup; and __fish_seen_subcommand_from help" -f -a "list" -d 'List all credential backups'
complete -c aisw -n "__fish_aisw_using_subcommand backup; and __fish_seen_subcommand_from help" -f -a "restore" -d 'Restore a backup by id'
complete -c aisw -n "__fish_aisw_using_subcommand backup; and __fish_seen_subcommand_from help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c aisw -n "__fish_aisw_using_subcommand help; and not __fish_seen_subcommand_from add context use list remove rename status init shell-hook backup help" -f -a "add" -d 'Add a new account profile for a tool'
complete -c aisw -n "__fish_aisw_using_subcommand help; and not __fish_seen_subcommand_from add context use list remove rename status init shell-hook backup help" -f -a "context" -d 'Manage named multi-tool contexts'
complete -c aisw -n "__fish_aisw_using_subcommand help; and not __fish_seen_subcommand_from add context use list remove rename status init shell-hook backup help" -f -a "use" -d 'Switch the active account for a tool'
complete -c aisw -n "__fish_aisw_using_subcommand help; and not __fish_seen_subcommand_from add context use list remove rename status init shell-hook backup help" -f -a "list" -d 'Show all profiles, optionally filtered by tool'
complete -c aisw -n "__fish_aisw_using_subcommand help; and not __fish_seen_subcommand_from add context use list remove rename status init shell-hook backup help" -f -a "remove" -d 'Remove a stored profile'
complete -c aisw -n "__fish_aisw_using_subcommand help; and not __fish_seen_subcommand_from add context use list remove rename status init shell-hook backup help" -f -a "rename" -d 'Rename a stored profile'
complete -c aisw -n "__fish_aisw_using_subcommand help; and not __fish_seen_subcommand_from add context use list remove rename status init shell-hook backup help" -f -a "status" -d 'Show current active profiles and credential status'
complete -c aisw -n "__fish_aisw_using_subcommand help; and not __fish_seen_subcommand_from add context use list remove rename status init shell-hook backup help" -f -a "init" -d 'First-run setup: shell integration and credential import'
complete -c aisw -n "__fish_aisw_using_subcommand help; and not __fish_seen_subcommand_from add context use list remove rename status init shell-hook backup help" -f -a "shell-hook" -d 'Print the shell integration hook for the given shell'
complete -c aisw -n "__fish_aisw_using_subcommand help; and not __fish_seen_subcommand_from add context use list remove rename status init shell-hook backup help" -f -a "backup" -d 'Manage credential backups'
complete -c aisw -n "__fish_aisw_using_subcommand help; and not __fish_seen_subcommand_from add context use list remove rename status init shell-hook backup help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c aisw -n "__fish_aisw_using_subcommand help; and __fish_seen_subcommand_from backup" -f -a "list" -d 'List all credential backups'
complete -c aisw -n "__fish_aisw_using_subcommand help; and __fish_seen_subcommand_from backup" -f -a "restore" -d 'Restore a backup by id'

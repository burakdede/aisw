use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

use crate::auth::secure_store;
use crate::backup::BackupManager;
use crate::cli::UninstallArgs;
use crate::commands::init::{rc_file, HOOK_MARKER};
use crate::config::{ConfigStore, CredentialBackend};
use crate::output;
use crate::runtime;
use crate::types::Tool;

const SHELLS: [&str; 4] = ["bash", "zsh", "fish", "pwsh"];

pub fn run(args: UninstallArgs, home: &Path, user_home: &Path) -> Result<()> {
    if runtime::is_non_interactive() && !args.yes && !args.dry_run {
        bail!(
            "uninstall requires confirmation.\n  \
             Re-run with --dry-run to preview changes, with --yes to apply them, or omit --non-interactive."
        );
    }

    let plan = build_plan(home, user_home)?;

    if !args.dry_run && !args.yes {
        eprint!("{}", confirmation_prompt(&plan, &args));
        let mut line = String::new();
        std::io::stdin()
            .read_line(&mut line)
            .context("could not read confirmation from stdin")?;
        if !matches!(line.trim(), "y" | "Y") {
            bail!("operation cancelled by user.");
        }
    }

    run_inner(args, home, user_home)
}

pub(crate) fn run_inner(args: UninstallArgs, home: &Path, user_home: &Path) -> Result<()> {
    let plan = build_plan(home, user_home)?;

    if args.dry_run {
        print_summary("Uninstall dry run", &plan, &args, true);
        output::print_blank_line();
        output::print_next_step("Review the plan above, then run 'aisw uninstall --yes' or add '--remove-data' if you also want to delete AISW_HOME.");
        return Ok(());
    }

    let mut removed_hooks = Vec::new();
    for rc in &plan.shell_hook_files {
        remove_hook_block(rc)?;
        removed_hooks.push(rc.display().to_string());
    }

    let mut purged_secrets = 0usize;
    let removed_data = if args.remove_data && home.exists() {
        ensure_safe_to_delete(home, user_home)?;
        // Deleting AISW_HOME alone would strand credentials in the OS keyring,
        // which is the opposite of what "remove my data" means. Purge them
        // first, while the config and backup index that name them still exist.
        purged_secrets = purge_managed_secrets(home);
        fs::remove_dir_all(home).with_context(|| format!("could not remove {}", home.display()))?;
        true
    } else {
        false
    };

    print_summary("Uninstall complete", &plan, &args, false);

    if !removed_hooks.is_empty() {
        output::print_blank_line();
        output::print_section("Removed shell integration from");
        for rc in removed_hooks {
            output::print_info(rc);
        }
    }

    output::print_blank_line();
    output::print_effects_header();
    if removed_data {
        output::print_effect(format!("Deleted {}.", home.display()));
        if purged_secrets > 0 {
            output::print_effect(format!(
                "Removed {purged_secrets} aisw-managed system keyring entr{}.",
                if purged_secrets == 1 { "y" } else { "ies" }
            ));
        }
    } else if plan.data_dir_exists {
        output::print_effect(format!("Kept {}.", home.display()));
    } else {
        output::print_effect(format!("No {} directory was present.", home.display()));
    }
    if plan.shell_hook_files.is_empty() {
        output::print_effect("No aisw-managed shell hook block was found.");
    } else {
        output::print_effect("Removed aisw-managed shell hook block(s).");
    }
    output::print_effect(
        "Did not modify upstream tool directories such as ~/.claude, ~/.codex, or ~/.gemini.",
    );
    output::print_blank_line();
    output::print_next_step(
        "Restart your shell or source your rc file. If you installed via Cargo, run 'cargo uninstall aisw' to remove the binary; otherwise remove the installed aisw binary manually.",
    );

    Ok(())
}

#[derive(Debug)]
struct Plan {
    shell_hook_files: Vec<PathBuf>,
    data_dir_exists: bool,
}

fn build_plan(home: &Path, user_home: &Path) -> Result<Plan> {
    let mut shell_hook_files = Vec::new();
    for shell in SHELLS {
        let Some(rc) = rc_file(user_home, shell) else {
            continue;
        };
        if rc.exists() && file_contains_hook(&rc)? {
            shell_hook_files.push(rc);
        }
    }

    Ok(Plan {
        shell_hook_files,
        data_dir_exists: home.exists(),
    })
}

/// Refuse to recursively delete a path that is clearly not an aisw home.
///
/// `AISW_HOME` is user-supplied, so a stray `AISW_HOME=$HOME` would otherwise
/// turn `uninstall --remove-data --yes` into `rm -rf ~`.
fn ensure_safe_to_delete(home: &Path, user_home: &Path) -> Result<()> {
    if home.parent().is_none() {
        bail!(
            "refusing to delete {} — AISW_HOME must not be a filesystem root.",
            home.display()
        );
    }
    if home == user_home {
        bail!(
            "refusing to delete {} — AISW_HOME must not be your home directory.\n  \
             Point AISW_HOME at a dedicated directory such as ~/.aisw and retry.",
            home.display()
        );
    }
    Ok(())
}

/// Delete every system-keyring secret aisw created under this home.
///
/// Best effort: a keyring that is locked or unavailable must not block the
/// filesystem cleanup, so failures are counted as "not purged" rather than
/// aborting the uninstall. Returns the number of entries removed.
fn purge_managed_secrets(home: &Path) -> usize {
    let mut purged = 0usize;

    let mut keyring_profiles = Vec::new();
    if let Ok(config) = ConfigStore::new(home).load() {
        for tool in Tool::ALL {
            for (name, meta) in config.profiles_for(tool) {
                if meta.credential_backend == CredentialBackend::SystemKeyring {
                    keyring_profiles.push((tool, name.clone()));
                }
            }
        }
    }

    for (tool, name) in &keyring_profiles {
        if secure_store::delete_profile_secret(*tool, name).is_ok() {
            purged += 1;
        }
    }

    // Backups only carry a keyring secret when their profile was keyring-backed,
    // so restrict the sweep to those profiles rather than probing every backup.
    if let Ok(backups) = BackupManager::new(home).list() {
        for entry in backups {
            let keyring_backed = keyring_profiles
                .iter()
                .any(|(tool, name)| *tool == entry.tool && *name == entry.profile);
            if keyring_backed
                && secure_store::delete_backup_secret(entry.tool, &entry.profile, &entry.backup_id)
                    .is_ok()
            {
                purged += 1;
            }
        }
    }

    purged
}

fn file_contains_hook(path: &Path) -> Result<bool> {
    let contents =
        fs::read_to_string(path).with_context(|| format!("could not read {}", path.display()))?;
    Ok(contents.contains(HOOK_MARKER))
}

fn confirmation_prompt(plan: &Plan, args: &UninstallArgs) -> String {
    let shell_text = if plan.shell_hook_files.is_empty() {
        "no shell hook files".to_owned()
    } else if plan.shell_hook_files.len() == 1 {
        format!(
            "shell integration from {}",
            plan.shell_hook_files[0].display()
        )
    } else {
        format!(
            "shell integration from {} files",
            plan.shell_hook_files.len()
        )
    };

    let data_text = if args.remove_data {
        "delete aisw-managed data"
    } else {
        "keep aisw-managed data"
    };

    format!(
        "Uninstall aisw by removing {shell_text} and {data_text}? [y/N] \n\
Tip: run 'aisw uninstall --dry-run' first to preview exactly what will change.\n> "
    )
}

fn print_summary(title: &str, plan: &Plan, args: &UninstallArgs, dry_run: bool) {
    output::print_title(title);
    output::print_kv("Shell hooks", plan.shell_hook_files.len().to_string());
    output::print_kv(
        "AISW_HOME",
        if args.remove_data {
            "remove"
        } else if plan.data_dir_exists {
            "keep"
        } else {
            "not present"
        },
    );
    output::print_kv(
        "Mode",
        if dry_run {
            "preview only"
        } else {
            "apply changes"
        },
    );
    output::print_blank_line();
    if plan.shell_hook_files.is_empty() {
        output::print_info("No aisw-managed shell hook block found.");
    } else {
        output::print_section("Shell hook files");
        for rc in &plan.shell_hook_files {
            output::print_info(rc.display().to_string());
        }
    }
    output::print_blank_line();
    output::print_section("Scope");
    output::print_info("Only aisw-managed shell hook blocks are removed from rc files.");
    output::print_info(
        "Upstream tool directories such as ~/.claude, ~/.codex, and ~/.gemini are not modified.",
    );
}

fn remove_hook_block(path: &Path) -> Result<()> {
    let contents =
        fs::read_to_string(path).with_context(|| format!("could not read {}", path.display()))?;
    let updated = strip_hook_block(&contents);
    if updated != contents {
        fs::write(path, updated).with_context(|| format!("could not write {}", path.display()))?;
    }
    Ok(())
}

fn strip_hook_block(contents: &str) -> String {
    let mut out: Vec<&str> = Vec::new();
    let lines: Vec<&str> = contents.split_inclusive('\n').collect();
    let mut idx = 0;

    while idx < lines.len() {
        let line = lines[idx];
        if line.trim_end_matches('\n') == HOOK_MARKER {
            if matches!(out.last(), Some(prev) if prev.trim().is_empty()) {
                out.pop();
            }

            idx += 1;
            if idx < lines.len() && lines[idx].contains("aisw shell-hook") {
                idx += 1;
            }
            continue;
        }

        out.push(line);
        idx += 1;
    }

    out.concat()
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn strip_hook_block_removes_marker_and_hook_line() {
        let text = "export PATH=/bin\n\n# Added by aisw\neval \"$(aisw shell-hook zsh)\"\n";
        assert_eq!(strip_hook_block(text), "export PATH=/bin\n");
    }

    #[test]
    fn strip_hook_block_leaves_unrelated_content() {
        let text = "# Added by something else\necho hi\n";
        assert_eq!(strip_hook_block(text), text);
    }

    #[test]
    fn run_inner_dry_run_preserves_files() {
        let tmp = tempdir().unwrap();
        let home = tmp.path().join("aisw");
        let user_home = tmp.path().join("home");
        fs::create_dir_all(&home).unwrap();
        fs::create_dir_all(&user_home).unwrap();
        let zshrc = user_home.join(".zshrc");
        fs::write(
            &zshrc,
            "\n# Added by aisw\neval \"$(aisw shell-hook zsh)\"\n",
        )
        .unwrap();

        run_inner(
            UninstallArgs {
                remove_data: true,
                dry_run: true,
                yes: false,
            },
            &home,
            &user_home,
        )
        .unwrap();

        assert!(home.exists());
        assert!(fs::read_to_string(zshrc)
            .unwrap()
            .contains("shell-hook zsh"));
    }

    #[test]
    fn confirmation_prompt_mentions_single_shell_file_and_keep_data() {
        let plan = Plan {
            shell_hook_files: vec![PathBuf::from("/tmp/.zshrc")],
            data_dir_exists: true,
        };
        let args = UninstallArgs {
            remove_data: false,
            dry_run: false,
            yes: false,
        };
        let prompt = confirmation_prompt(&plan, &args);
        assert!(prompt.contains("shell integration from /tmp/.zshrc"));
        assert!(prompt.contains("keep aisw-managed data"));
    }

    #[test]
    fn confirmation_prompt_mentions_multiple_shell_files_and_remove_data() {
        let plan = Plan {
            shell_hook_files: vec![PathBuf::from("/tmp/.zshrc"), PathBuf::from("/tmp/.bashrc")],
            data_dir_exists: true,
        };
        let args = UninstallArgs {
            remove_data: true,
            dry_run: false,
            yes: false,
        };
        let prompt = confirmation_prompt(&plan, &args);
        assert!(prompt.contains("shell integration from 2 files"));
        assert!(prompt.contains("delete aisw-managed data"));
    }

    #[test]
    fn strip_hook_block_removes_marker_only_line() {
        let text = "line1\n# Added by aisw\nline2\n";
        assert_eq!(strip_hook_block(text), "line1\nline2\n");
    }

    #[test]
    fn run_inner_apply_removes_hook_and_keeps_data_by_default() {
        let tmp = tempdir().unwrap();
        let home = tmp.path().join("aisw");
        let user_home = tmp.path().join("home");
        fs::create_dir_all(&home).unwrap();
        fs::create_dir_all(&user_home).unwrap();
        let zshrc = user_home.join(".zshrc");
        fs::write(
            &zshrc,
            "export PATH=/bin\n\n# Added by aisw\neval \"$(aisw shell-hook zsh)\"\n",
        )
        .unwrap();

        run_inner(
            UninstallArgs {
                remove_data: false,
                dry_run: false,
                yes: true,
            },
            &home,
            &user_home,
        )
        .unwrap();

        assert!(home.exists());
        let updated = fs::read_to_string(&zshrc).unwrap();
        assert!(!updated.contains("aisw shell-hook"));
        assert!(updated.contains("export PATH=/bin"));
    }

    #[test]
    fn run_inner_apply_remove_data_deletes_home() {
        let tmp = tempdir().unwrap();
        let home = tmp.path().join("aisw");
        let user_home = tmp.path().join("home");
        fs::create_dir_all(&home).unwrap();
        fs::create_dir_all(&user_home).unwrap();

        run_inner(
            UninstallArgs {
                remove_data: true,
                dry_run: false,
                yes: true,
            },
            &home,
            &user_home,
        )
        .unwrap();

        assert!(!home.exists());
    }
}

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

use crate::cli::UninstallArgs;
use crate::commands::init::{rc_file, HOOK_MARKER};
use crate::output;
use crate::runtime;

const SHELLS: [&str; 3] = ["bash", "zsh", "fish"];

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

    let removed_data = if args.remove_data && home.exists() {
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
        "Restart your shell or source your rc file, then remove the aisw binary manually if you no longer want it installed.",
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
        let rc = rc_file(user_home, shell);
        if rc.exists() && file_contains_hook(&rc)? {
            shell_hook_files.push(rc);
        }
    }

    Ok(Plan {
        shell_hook_files,
        data_dir_exists: home.exists(),
    })
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
}

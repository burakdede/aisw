use std::path::Path;

use anyhow::Result;

use crate::cli::ListArgs;
use crate::config::{AuthMethod, ConfigStore};
use crate::output;
use crate::types::Tool;

pub(crate) struct Row {
    tool: &'static str,
    profile: String,
    active: bool,
    auth_method: &'static str,
    label: Option<String>,
}

fn auth_display(method: AuthMethod) -> &'static str {
    match method {
        AuthMethod::OAuth => "oauth",
        AuthMethod::ApiKey => "api_key",
    }
}

pub(crate) fn collect_rows(args: &ListArgs, home: &Path) -> Result<Vec<Row>> {
    let config_store = ConfigStore::new(home);
    let config = config_store.load()?;

    let tools: &[Tool] = match args.tool {
        Some(t) => match t {
            Tool::Claude => &[Tool::Claude],
            Tool::Codex => &[Tool::Codex],
            Tool::Gemini => &[Tool::Gemini],
        },
        None => &[Tool::Claude, Tool::Codex, Tool::Gemini],
    };

    let mut rows = Vec::new();
    for &tool in tools {
        let profiles = match tool {
            Tool::Claude => &config.profiles.claude,
            Tool::Codex => &config.profiles.codex,
            Tool::Gemini => &config.profiles.gemini,
        };
        let active = match tool {
            Tool::Claude => config.active.claude.as_deref(),
            Tool::Codex => config.active.codex.as_deref(),
            Tool::Gemini => config.active.gemini.as_deref(),
        };

        let mut names: Vec<&str> = profiles.keys().map(String::as_str).collect();
        names.sort_unstable();

        for name in names {
            let meta = &profiles[name];
            rows.push(Row {
                tool: tool.binary_name(),
                profile: name.to_owned(),
                active: active == Some(name),
                auth_method: auth_display(meta.auth_method),
                label: meta.label.clone(),
            });
        }
    }
    Ok(rows)
}

pub fn run(args: ListArgs, home: &Path) -> Result<()> {
    let rows = collect_rows(&args, home)?;

    if args.json {
        print_json(&rows)?;
    } else {
        print_table(&rows);
    }

    Ok(())
}

fn print_table(rows: &[Row]) {
    if rows.is_empty() {
        output::print_title("Profiles");
        output::print_empty_state("No profiles found.");
        output::print_blank_line();
        output::print_next_step("Run 'aisw add <tool> <name>' to add one.");
        return;
    }

    output::print_title("Profiles");

    let mut current_tool = None;
    for row in rows {
        if current_tool != Some(row.tool) {
            if current_tool.is_some() {
                output::print_blank_line();
            }

            let tool = match row.tool {
                "claude" => Tool::Claude,
                "codex" => Tool::Codex,
                "gemini" => Tool::Gemini,
                _ => unreachable!(),
            };
            output::print_tool_section(tool);
            current_tool = Some(row.tool);
        }

        output::print_profile_section(&row.profile, row.active);
        output::print_kv("Active", if row.active { "yes" } else { "no" });
        output::print_kv("Auth", row.auth_method);
        if let Some(label) = row.label.as_deref() {
            output::print_kv("Label", label);
        }
        output::print_blank_line();
    }
}

fn print_json(rows: &[Row]) -> Result<()> {
    let json_rows: Vec<serde_json::Value> = rows
        .iter()
        .map(|r| {
            serde_json::json!({
                "tool":        r.tool,
                "profile":     r.profile,
                "active":      r.active,
                "auth_method": r.auth_method,
                "label":       r.label,
            })
        })
        .collect();
    println!("{}", serde_json::to_string_pretty(&json_rows)?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;
    use crate::auth;
    use crate::cli::ListArgs;
    use crate::config::ConfigStore;
    use crate::profile::ProfileStore;
    use crate::types::Tool;

    fn claude_key() -> &'static str {
        "sk-ant-api03-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"
    }

    fn list_args(tool: Option<Tool>, json: bool) -> ListArgs {
        ListArgs { tool, json }
    }

    #[test]
    fn empty_config_returns_no_rows() {
        let tmp = tempdir().unwrap();
        let rows = collect_rows(&list_args(None, false), tmp.path()).unwrap();
        assert!(rows.is_empty());
    }

    #[test]
    fn added_profile_appears_in_rows() {
        let tmp = tempdir().unwrap();
        let ps = ProfileStore::new(tmp.path());
        let cs = ConfigStore::new(tmp.path());
        auth::claude::add_api_key(&ps, &cs, "work", claude_key(), None).unwrap();

        let rows = collect_rows(&list_args(None, false), tmp.path()).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].tool, "claude");
        assert_eq!(rows[0].profile, "work");
        assert_eq!(rows[0].auth_method, "api_key");
        assert!(!rows[0].active);
    }

    #[test]
    fn active_profile_marked() {
        let tmp = tempdir().unwrap();
        let ps = ProfileStore::new(tmp.path());
        let cs = ConfigStore::new(tmp.path());
        auth::claude::add_api_key(&ps, &cs, "work", claude_key(), None).unwrap();
        cs.set_active(Tool::Claude, "work").unwrap();

        let rows = collect_rows(&list_args(None, false), tmp.path()).unwrap();
        assert!(rows[0].active);
    }

    #[test]
    fn tool_filter_excludes_other_tools() {
        let tmp = tempdir().unwrap();
        let ps = ProfileStore::new(tmp.path());
        let cs = ConfigStore::new(tmp.path());
        auth::claude::add_api_key(&ps, &cs, "work", claude_key(), None).unwrap();
        auth::codex::add_api_key(&ps, &cs, "main", "sk-codex-test-key-12345", None).unwrap();

        let rows = collect_rows(&list_args(Some(Tool::Claude), false), tmp.path()).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].tool, "claude");
    }

    #[test]
    fn label_stored_in_row() {
        let tmp = tempdir().unwrap();
        let ps = ProfileStore::new(tmp.path());
        let cs = ConfigStore::new(tmp.path());
        auth::claude::add_api_key(&ps, &cs, "work", claude_key(), Some("Work key".into())).unwrap();

        let rows = collect_rows(&list_args(None, false), tmp.path()).unwrap();
        assert_eq!(rows[0].label.as_deref(), Some("Work key"));
    }

    #[test]
    fn profiles_sorted_alphabetically() {
        let tmp = tempdir().unwrap();
        let ps = ProfileStore::new(tmp.path());
        let cs = ConfigStore::new(tmp.path());
        auth::claude::add_api_key(&ps, &cs, "zzz", claude_key(), None).unwrap();
        auth::claude::add_api_key(
            &ps,
            &cs,
            "aaa",
            "sk-ant-api03-BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB",
            None,
        )
        .unwrap();

        let rows = collect_rows(&list_args(Some(Tool::Claude), false), tmp.path()).unwrap();
        assert_eq!(rows[0].profile, "aaa");
        assert_eq!(rows[1].profile, "zzz");
    }
}

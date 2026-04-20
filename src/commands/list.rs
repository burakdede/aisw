use std::path::Path;

use anyhow::Result;
use chrono::{DateTime, Utc};
use console::style;

use crate::cli::ListArgs;
use crate::config::{AuthMethod, ConfigStore, CredentialBackend};
use crate::output;
use crate::types::Tool;

pub(crate) struct Row {
    pub(crate) tool: &'static str,
    pub(crate) profile: String,
    pub(crate) active: bool,
    pub(crate) auth_method: &'static str,
    #[allow(dead_code)]
    pub(crate) credential_backend: &'static str,
    pub(crate) label: Option<String>,
    pub(crate) added_at: DateTime<Utc>,
}

fn auth_display(method: AuthMethod) -> &'static str {
    match method {
        AuthMethod::OAuth => "oauth",
        AuthMethod::ApiKey => "api_key",
    }
}

fn backend_display(backend: CredentialBackend) -> &'static str {
    backend.display_name()
}

pub(crate) fn collect_rows(args: &ListArgs, home: &Path) -> Result<Vec<Row>> {
    let config_store = ConfigStore::new(home);
    let config = config_store.load()?;

    let selected_tool = args.tool.or(args.tool_filter);
    let tools: Vec<Tool> = match selected_tool {
        Some(t) => vec![t],
        None => Tool::ALL.to_vec(),
    };

    let mut rows = Vec::new();
    for tool in tools {
        let profiles = config.profiles_for(tool);
        let active = config.active_for(tool);

        let mut names: Vec<&str> = profiles.keys().map(String::as_str).collect();
        names.sort_unstable();

        for name in names {
            let meta = &profiles[name];
            rows.push(Row {
                tool: tool.binary_name(),
                profile: name.to_owned(),
                active: active == Some(name),
                auth_method: auth_display(meta.auth_method),
                credential_backend: backend_display(meta.credential_backend),
                label: meta.label.clone(),
                added_at: meta.added_at,
            });
        }
    }

    if args.active_only {
        rows.retain(|row| row.active);
    }

    if let Some(search) = args.search.as_deref() {
        let needle = search.trim().to_ascii_lowercase();
        if !needle.is_empty() {
            rows.retain(|row| {
                row.profile.to_ascii_lowercase().contains(&needle)
                    || row
                        .label
                        .as_deref()
                        .unwrap_or_default()
                        .to_ascii_lowercase()
                        .contains(&needle)
                    || row.tool.to_ascii_lowercase().contains(&needle)
            });
        }
    }

    match args.sort {
        Some(crate::cli::SortBy::Name) => {
            rows.sort_by(|a, b| a.tool.cmp(b.tool).then_with(|| a.profile.cmp(&b.profile)));
        }
        Some(crate::cli::SortBy::Recent) => {
            rows.sort_by(|a, b| {
                b.added_at
                    .cmp(&a.added_at)
                    .then_with(|| a.tool.cmp(b.tool))
                    .then_with(|| a.profile.cmp(&b.profile))
            });
        }
        None => {}
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
    const PROFILE_WIDTH: usize = 36;
    const LABEL_WIDTH: usize = 40;

    if rows.is_empty() {
        output::print_title("Profiles");
        output::print_empty_state("No profiles found.");
        output::print_blank_line();
        output::print_next_step("Run 'aisw add <tool> <name>' to add one.");
        return;
    }

    output::print_title("Profiles");

    let mut current_tool: Option<&str> = None;
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

        // Build the line: bullet + profile name + auth badge + label
        let bullet = if row.active {
            format!("{}", style("\u{25cf}").green())
        } else {
            format!("{}", style("\u{25cb}").dim())
        };

        let display_profile = output::ellipsize(&row.profile, PROFILE_WIDTH);
        let name_part = if row.active {
            format!("{}", style(&display_profile).bold())
        } else {
            format!("{}", style(&display_profile).dim())
        };

        let auth_badge = match row.auth_method {
            "oauth" => format!(" {}", style("[oauth]").cyan()),
            "api_key" => format!(" {}", style("[api-key]").yellow()),
            other => format!(" [{}]", other),
        };

        let label_part = match row.label.as_deref() {
            Some(l) => format!(
                "  {}",
                style(format!("({})", output::ellipsize(l, LABEL_WIDTH))).dim()
            ),
            None => String::new(),
        };

        let active_tag = if row.active {
            format!("  {}", style("[active]").green().bold())
        } else {
            String::new()
        };

        println!(
            "  {} {}{}{}{}",
            bullet, name_part, auth_badge, label_part, active_tag
        );
    }
}

fn print_json(rows: &[Row]) -> Result<()> {
    // Build grouped structure: { "claude": { "active": ..., "profiles": [...] }, ... }
    let mut map = serde_json::Map::new();

    for tool in Tool::ALL {
        let tool_name = tool.binary_name();
        let tool_rows: Vec<&Row> = rows.iter().filter(|r| r.tool == tool_name).collect();
        let active = tool_rows
            .iter()
            .find(|r| r.active)
            .map(|r| serde_json::Value::String(r.profile.clone()))
            .unwrap_or(serde_json::Value::Null);
        let profiles: Vec<serde_json::Value> = tool_rows
            .iter()
            .map(|r| {
                serde_json::json!({
                    "name": r.profile,
                    "auth": r.auth_method,
                    "label": r.label,
                })
            })
            .collect();
        map.insert(
            tool_name.to_owned(),
            serde_json::json!({
                "active": active,
                "profiles": profiles,
            }),
        );
    }

    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::Value::Object(map))?
    );
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
        ListArgs {
            tool,
            tool_filter: None,
            search: None,
            sort: None,
            active_only: false,
            json,
        }
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
        assert_eq!(rows[0].credential_backend, "file");
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

    #[test]
    fn search_filters_by_label_or_name() {
        let tmp = tempdir().unwrap();
        let ps = ProfileStore::new(tmp.path());
        let cs = ConfigStore::new(tmp.path());
        auth::claude::add_api_key(&ps, &cs, "work", claude_key(), Some("Billing".into())).unwrap();
        auth::claude::add_api_key(
            &ps,
            &cs,
            "personal",
            "sk-ant-api03-BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB",
            None,
        )
        .unwrap();

        let mut args = list_args(None, false);
        args.search = Some("bill".into());
        let rows = collect_rows(&args, tmp.path()).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].profile, "work");
    }

    #[test]
    fn active_only_filters_non_active_rows() {
        let tmp = tempdir().unwrap();
        let ps = ProfileStore::new(tmp.path());
        let cs = ConfigStore::new(tmp.path());
        auth::claude::add_api_key(&ps, &cs, "work", claude_key(), None).unwrap();
        auth::codex::add_api_key(&ps, &cs, "main", "sk-codex-test-key-12345", None).unwrap();
        cs.set_active(Tool::Claude, "work").unwrap();

        let mut args = list_args(None, false);
        args.active_only = true;
        let rows = collect_rows(&args, tmp.path()).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].tool, "claude");
    }
}

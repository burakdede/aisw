use std::path::Path;

use anyhow::Result;
use chrono::{DateTime, Local, Utc};
use console::style;

use crate::cli::UsageArgs;
use crate::config::ConfigStore;
use crate::error::AiswError;
use crate::output;
use crate::profile::ProfileStore;
use crate::types::Tool;
use crate::usage::{self, UsageReport, UsageWindow};

pub(crate) struct Row {
    pub(crate) tool: Tool,
    pub(crate) profile: String,
    pub(crate) active: bool,
    /// The vendor's report, or a per-profile reason it is unavailable.
    pub(crate) report: Result<UsageReport, String>,
}

pub fn run(args: UsageArgs, home: &Path) -> Result<()> {
    let rows = collect_rows(&args, home)?;
    if args.json {
        print_json(&rows)?;
    } else {
        print_text(&rows);
    }
    Ok(())
}

pub(crate) fn collect_rows(args: &UsageArgs, home: &Path) -> Result<Vec<Row>> {
    let config = ConfigStore::new(home).load()?;
    let profile_store = ProfileStore::new(home);

    let tools: Vec<Tool> = match args.tool {
        Some(tool) if !usage::supports_usage(tool) => anyhow::bail!(
            "{} does not expose a usage endpoint; usage is available for claude and codex",
            tool.display_name()
        ),
        Some(tool) => vec![tool],
        None => Tool::ALL
            .into_iter()
            .filter(|tool| usage::supports_usage(*tool))
            .collect(),
    };

    let mut rows = Vec::new();
    for tool in tools {
        let profiles = config.profiles_for(tool);
        let active = config.active_for(tool);

        let mut names: Vec<&str> = profiles.keys().map(String::as_str).collect();
        names.sort_unstable();
        if let Some(wanted) = args.profile.as_deref() {
            if !profiles.contains_key(wanted) {
                return Err(AiswError::ProfileNotFound {
                    tool,
                    name: wanted.to_owned(),
                }
                .into());
            }
            names.retain(|name| *name == wanted);
        }
        if args.active_only {
            names.retain(|name| Some(*name) == active);
        }

        for name in names {
            let report = usage::fetch_profile_usage(&profile_store, tool, name, &profiles[name])
                .map_err(|err| format!("{err:#}"));
            rows.push(Row {
                tool,
                profile: name.to_owned(),
                active: active == Some(name),
                report,
            });
        }
    }
    Ok(rows)
}

fn print_text(rows: &[Row]) {
    output::print_title("Usage");

    if rows.is_empty() {
        output::print_empty_state("No profiles found for Claude Code or Codex CLI.");
        output::print_blank_line();
        output::print_next_step("Run 'aisw add <tool> <name>' to add one.");
        return;
    }

    let now = Utc::now();
    let mut current_tool: Option<Tool> = None;
    for row in rows {
        if current_tool != Some(row.tool) {
            if current_tool.is_some() {
                output::print_blank_line();
            }
            output::print_tool_section(row.tool);
            current_tool = Some(row.tool);
        }

        let bullet = if row.active {
            style("\u{25cf}").green()
        } else {
            style("\u{25cb}").dim()
        };
        let name = if row.active {
            style(&row.profile).bold()
        } else {
            style(&row.profile).dim()
        };
        match &row.report {
            Ok(report) => {
                println!(
                    "  {bullet} {name}  {}",
                    style(describe_account(report)).dim()
                );
                if report.windows.is_empty() {
                    println!("      {}", style("no usage windows reported").dim());
                }
                for window in &report.windows {
                    println!("      {}", describe_window(window, now));
                }
            }
            Err(message) => {
                println!("  {bullet} {name}  {}", style(message).yellow());
            }
        }
    }
}

fn describe_account(report: &UsageReport) -> String {
    match (report.account.as_deref(), report.plan.as_deref()) {
        (Some(account), Some(plan)) => format!("{account} ({plan})"),
        (Some(account), None) => account.to_owned(),
        (None, Some(plan)) => plan.to_owned(),
        (None, None) => String::new(),
    }
}

fn describe_window(window: &UsageWindow, now: DateTime<Utc>) -> String {
    let mut line = format!("{:<12} {:>3.0}%", window.name, window.used_percent);
    if let Some(resets_at) = window.resets_at {
        let clock = reset_clock(resets_at, now);
        let remaining = countdown(resets_at - now);
        line.push_str(&format!("   resets {clock} ({remaining})"));
    }
    line
}

/// Local clock time, with the date only when the reset is not today.
fn reset_clock(resets_at: DateTime<Utc>, now: DateTime<Utc>) -> String {
    let local = resets_at.with_timezone(&Local);
    if local.date_naive() == now.with_timezone(&Local).date_naive() {
        local.format("%H:%M").to_string()
    } else {
        local.format("%b %-d %H:%M").to_string()
    }
}

fn countdown(remaining: chrono::Duration) -> String {
    let total_minutes = remaining.num_minutes().max(0);
    let days = total_minutes / (24 * 60);
    let hours = total_minutes % (24 * 60) / 60;
    let minutes = total_minutes % 60;
    if days > 0 {
        format!("in {days}d {hours}h")
    } else if hours > 0 {
        format!("in {hours}h {minutes}m")
    } else {
        format!("in {minutes}m")
    }
}

fn print_json(rows: &[Row]) -> Result<()> {
    // Same grouping as `list --json`: { "claude": { "active": ..., "profiles": [...] }, ... }
    let mut map = serde_json::Map::new();

    for tool in Tool::ALL
        .into_iter()
        .filter(|tool| usage::supports_usage(*tool))
    {
        let tool_rows: Vec<&Row> = rows.iter().filter(|row| row.tool == tool).collect();
        let active = tool_rows
            .iter()
            .find(|row| row.active)
            .map(|row| serde_json::Value::String(row.profile.clone()))
            .unwrap_or(serde_json::Value::Null);
        let profiles: Vec<serde_json::Value> = tool_rows
            .iter()
            .map(|row| {
                let report = row.report.as_ref().ok();
                serde_json::json!({
                    "name": row.profile,
                    "active": row.active,
                    "account": report.and_then(|report| report.account.clone()),
                    "plan": report.and_then(|report| report.plan.clone()),
                    "windows": report.map(|report| report.windows.clone()).unwrap_or_default(),
                    "error": row.report.as_ref().err(),
                })
            })
            .collect();
        map.insert(
            tool.binary_name().to_owned(),
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
    use super::*;

    #[test]
    fn countdown_uses_the_two_largest_units() {
        assert_eq!(
            countdown(chrono::Duration::minutes(2 * 24 * 60 + 12 * 60 + 7)),
            "in 2d 12h"
        );
        assert_eq!(
            countdown(chrono::Duration::minutes(2 * 60 + 24)),
            "in 2h 24m"
        );
        assert_eq!(countdown(chrono::Duration::minutes(9)), "in 9m");
        assert_eq!(countdown(chrono::Duration::minutes(-30)), "in 0m");
    }

    #[test]
    fn describe_account_joins_account_and_plan() {
        let report = |account: Option<&str>, plan: Option<&str>| UsageReport {
            account: account.map(str::to_owned),
            plan: plan.map(str::to_owned),
            windows: Vec::new(),
        };
        assert_eq!(
            describe_account(&report(Some("me@example.com"), Some("pro"))),
            "me@example.com (pro)"
        );
        assert_eq!(
            describe_account(&report(Some("me@example.com"), None)),
            "me@example.com"
        );
        assert_eq!(describe_account(&report(None, Some("pro"))), "pro");
        assert_eq!(describe_account(&report(None, None)), "");
    }

    #[test]
    fn describe_window_omits_reset_when_unknown() {
        let window = UsageWindow {
            name: "5h".to_owned(),
            used_percent: 14.0,
            resets_at: None,
        };
        assert_eq!(describe_window(&window, Utc::now()), "5h            14%");
    }
}

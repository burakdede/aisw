use std::borrow::Cow;
use std::ffi::OsString;

use anyhow::Error;
use console::style;

use crate::types::Tool;

const KV_LABEL_WIDTH: usize = 14;

pub fn configure_color(no_color_flag: bool) {
    let enabled = should_enable_color(no_color_flag, std::env::var_os("NO_COLOR"));
    console::set_colors_enabled(enabled);
    console::set_colors_enabled_stderr(enabled);
}

pub fn print_title(title: &str) {
    println!("{}", style(title).bold().cyan());
    println!("{}", style("─".repeat(title.chars().count().max(12))).dim());
    println!();
}

pub fn print_section(title: &str) {
    println!("{}", style(title).bold());
}

pub fn print_tool_section(tool: Tool) {
    print_section(tool.display_name());
}

pub fn print_profile_section(name: &str, active: bool) {
    if active {
        println!(
            "{} {}",
            style(name).bold(),
            style("[active]").green().bold()
        );
    } else {
        println!("{}", style(name).bold());
    }
}

pub fn print_kv(label: &str, value: impl AsRef<str>) {
    println!(
        "  {} {}",
        style(format!("{label:<KV_LABEL_WIDTH$}:",)).dim(),
        styled_value(label, value.as_ref())
    );
}

pub fn print_blank_line() {
    println!();
}

pub fn print_next_step(message: impl AsRef<str>) {
    println!("{}", style("Next").bold().cyan());
    println!("  {}", style(message.as_ref()).cyan());
}

pub fn print_fix(message: impl AsRef<str>) {
    println!("{}", style("Fix").bold().yellow());
    println!("  {}", style(message.as_ref()).yellow());
}

pub fn print_info(message: impl AsRef<str>) {
    println!("  {}", style(message.as_ref()).dim());
}

pub fn print_effects_header() {
    println!("{}", style("Effects").bold().green());
}

pub fn print_effect(message: impl AsRef<str>) {
    println!("  {}", style(message.as_ref()).green());
}

pub fn print_success(message: impl AsRef<str>) {
    println!("{}", style(message.as_ref()).green().bold());
}

pub fn print_warning(message: impl AsRef<str>) {
    println!("{}", style(message.as_ref()).yellow().bold());
}

pub fn print_empty_state(message: impl AsRef<str>) {
    println!("{}", style(message.as_ref()).dim());
}

pub fn print_warning_stderr(message: impl AsRef<str>) {
    eprintln!("{}", style(message.as_ref()).yellow().bold());
}

pub fn print_info_stderr(message: impl AsRef<str>) {
    eprintln!("  {}", style(message.as_ref()).dim());
}

pub fn print_error_chain(error: &Error) {
    let chain: Vec<String> = error.chain().map(|c| c.to_string()).collect();
    if let Some((first, rest)) = chain.split_first() {
        eprintln!("{} {}", style("Error:").red().bold(), style(first).red());
        for msg in rest {
            if msg.trim_start().starts_with("Run 'aisw ") {
                eprintln!("{} {}", style("Fix:").yellow().bold(), style(msg).yellow());
            } else {
                eprintln!("  {}", style(msg).dim());
            }
        }
    }
}

pub fn active_value(active: Option<&str>) -> &str {
    match active {
        Some(value) => value,
        None => "none",
    }
}

fn styled_value<'a>(label: &str, value: &'a str) -> console::StyledObject<Cow<'a, str>> {
    let value = Cow::Borrowed(value);

    match label {
        "Active" if value == "none" => style(value).yellow(),
        "Active" => style(value).green(),
        "Activation" if value == "active" => style(value).green(),
        "Activation" => style(value).yellow(),
        "Was active" if value == "yes" => style(value).yellow(),
        "Was active" => style(value).dim(),
        "Auth" => style(value).cyan(),
        "State" => style_state(value),
        _ => style(value),
    }
}

fn style_state<'a>(value: Cow<'a, str>) -> console::StyledObject<Cow<'a, str>> {
    let lower = value.to_ascii_lowercase();

    if lower.contains("credentials present") || lower.contains("ready") {
        style(value).green()
    } else if lower.contains("not found")
        || lower.contains("mismatch")
        || lower.contains("permissions too broad")
        || lower.contains("no active")
    {
        style(value).yellow()
    } else {
        style(value)
    }
}

fn should_enable_color(no_color_flag: bool, no_color_env: Option<OsString>) -> bool {
    !no_color_flag && no_color_env.is_none()
}

#[cfg(test)]
mod tests {
    use super::should_enable_color;

    #[test]
    fn color_enabled_by_default() {
        assert!(should_enable_color(false, None));
    }

    #[test]
    fn no_color_flag_disables_color() {
        assert!(!should_enable_color(true, None));
    }

    #[test]
    fn no_color_env_disables_color() {
        assert!(!should_enable_color(false, Some("1".into())));
    }
}

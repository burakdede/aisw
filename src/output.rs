use std::borrow::Cow;
use std::ffi::OsString;

use anyhow::Error;
use console::style;

use crate::runtime;
use crate::types::Tool;

const KV_LABEL_WIDTH: usize = 14;

pub fn configure(no_color_flag: bool, quiet: bool) {
    let _ = quiet;
    let enabled = should_enable_color(no_color_flag, std::env::var_os("NO_COLOR"));
    console::set_colors_enabled(enabled);
    console::set_colors_enabled_stderr(enabled);
}

pub fn print_title(title: &str) {
    if runtime::is_quiet() {
        return;
    }
    println!("{}", style(title).bold().cyan());
    println!("{}", style("─".repeat(title.chars().count().max(12))).dim());
    println!();
}

pub fn print_section(title: &str) {
    if runtime::is_quiet() {
        return;
    }
    println!("{}", style(title).bold());
}

pub fn print_tool_section(tool: Tool) {
    print_section(tool.display_name());
}

pub fn print_profile_section(name: &str, active: bool) {
    if runtime::is_quiet() {
        return;
    }
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
    if runtime::is_quiet() {
        return;
    }
    println!(
        "  {} {}",
        style(format!("{label:<KV_LABEL_WIDTH$}:",)).dim(),
        styled_value(label, value.as_ref())
    );
}

pub fn print_blank_line() {
    if runtime::is_quiet() {
        return;
    }
    println!();
}

pub fn print_next_step(message: impl AsRef<str>) {
    if runtime::is_quiet() {
        return;
    }
    println!("{}", style("Next").bold().cyan());
    println!("  {}", style(message.as_ref()).cyan());
}

pub fn print_fix(message: impl AsRef<str>) {
    if runtime::is_quiet() {
        return;
    }
    println!("{}", style("Fix").bold().yellow());
    println!("  {}", style(message.as_ref()).yellow());
}

pub fn print_info(message: impl AsRef<str>) {
    if runtime::is_quiet() {
        return;
    }
    println!("  {}", style(message.as_ref()).dim());
}

pub fn print_effects_header() {
    if runtime::is_quiet() {
        return;
    }
    println!("{}", style("Effects").bold().green());
}

pub fn print_effect(message: impl AsRef<str>) {
    if runtime::is_quiet() {
        return;
    }
    println!("  {}", style(message.as_ref()).green());
}

pub fn print_success(message: impl AsRef<str>) {
    if runtime::is_quiet() {
        return;
    }
    println!("{}", style(message.as_ref()).green().bold());
}

pub fn print_warning(message: impl AsRef<str>) {
    if runtime::is_quiet() {
        return;
    }
    println!("{}", style(message.as_ref()).yellow().bold());
}

pub fn print_empty_state(message: impl AsRef<str>) {
    if runtime::is_quiet() {
        return;
    }
    println!("{}", style(message.as_ref()).dim());
}

pub fn print_table_header(columns: &[(&str, usize)]) {
    if runtime::is_quiet() {
        return;
    }

    let mut line = String::new();
    for (idx, (label, width)) in columns.iter().enumerate() {
        if idx > 0 {
            line.push(' ');
        }

        if *width == 0 {
            line.push_str(label);
        } else {
            line.push_str(&format!("{label:<width$}"));
        }
    }

    println!("{}", style(line).bold().dim());
}

pub fn print_table_row(cells: &[(&str, usize)]) {
    if runtime::is_quiet() {
        return;
    }

    let mut line = String::new();
    for (idx, (value, width)) in cells.iter().enumerate() {
        if idx > 0 {
            line.push(' ');
        }

        if *width == 0 {
            line.push_str(value);
        } else {
            line.push_str(&format!("{value:<width$}"));
        }
    }

    println!("{line}");
}

pub fn print_warning_stderr(message: impl AsRef<str>) {
    if runtime::is_quiet() {
        return;
    }
    eprintln!("{}", style(message.as_ref()).yellow().bold());
}

pub fn print_info_stderr(message: impl AsRef<str>) {
    if runtime::is_quiet() {
        return;
    }
    eprintln!("  {}", style(message.as_ref()).dim());
}

pub fn print_error_chain(error: &Error) {
    let chain: Vec<String> = error
        .chain()
        .map(|c| redact_sensitive_text(&c.to_string()))
        .collect();
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

fn redact_sensitive_text(text: &str) -> String {
    let mut redacted = text.to_owned();

    for (prefix, terminator) in [
        ("\"apiKey\":\"", '"'),
        ("\"token\":\"", '"'),
        ("ANTHROPIC_API_KEY=", '\n'),
        ("OPENAI_API_KEY=", '\n'),
        ("GEMINI_API_KEY=", '\n'),
    ] {
        redacted = redact_after_prefix(&redacted, prefix, terminator);
    }

    for prefix in ["sk-ant-", "sk-codex-", "sk-proj-", "sk-", "AIza"] {
        redacted = redact_prefixed_token(&redacted, prefix);
    }

    redacted
}

fn redact_after_prefix(text: &str, prefix: &str, terminator: char) -> String {
    let mut redacted = String::with_capacity(text.len());
    let mut cursor = 0;

    while let Some(offset) = text[cursor..].find(prefix) {
        let start = cursor + offset;
        let value_start = start + prefix.len();

        redacted.push_str(&text[cursor..value_start]);

        let value_end = if terminator == '\n' {
            text[value_start..]
                .find('\n')
                .map(|end| value_start + end)
                .unwrap_or(text.len())
        } else {
            text[value_start..]
                .find(terminator)
                .map(|end| value_start + end)
                .unwrap_or(text.len())
        };

        if value_end > value_start {
            redacted.push_str("[REDACTED]");
        }

        cursor = value_end;
    }

    redacted.push_str(&text[cursor..]);
    redacted
}

fn redact_prefixed_token(text: &str, prefix: &str) -> String {
    let mut redacted = String::with_capacity(text.len());
    let mut cursor = 0;

    while let Some(offset) = text[cursor..].find(prefix) {
        let start = cursor + offset;
        redacted.push_str(&text[cursor..start]);
        redacted.push_str("[REDACTED]");

        let token_end = text[start + prefix.len()..]
            .char_indices()
            .find_map(|(idx, ch)| (!is_secret_char(ch)).then_some(start + prefix.len() + idx))
            .unwrap_or(text.len());

        cursor = token_end;
    }

    redacted.push_str(&text[cursor..]);
    redacted
}

fn is_secret_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-')
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
    use super::{redact_sensitive_text, should_enable_color};

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

    #[test]
    fn redacts_json_secret_values() {
        let text = r#"auth file contents: {"token":"sk-codex-test-key-12345","apiKey":"sk-ant-api03-AAAAAAAAAA"}"#;
        let redacted = redact_sensitive_text(text);
        assert!(!redacted.contains("sk-codex-test-key-12345"));
        assert!(!redacted.contains("sk-ant-api03-AAAAAAAAAA"));
        assert!(redacted.contains(r#""token":"[REDACTED]""#));
        assert!(redacted.contains(r#""apiKey":"[REDACTED]""#));
    }

    #[test]
    fn redacts_env_style_secret_values() {
        let text = "export OPENAI_API_KEY=sk-proj-secret123\nGEMINI_API_KEY=AIzaSecret123";
        let redacted = redact_sensitive_text(text);
        assert!(!redacted.contains("sk-proj-secret123"));
        assert!(!redacted.contains("AIzaSecret123"));
        assert!(redacted.contains("OPENAI_API_KEY=[REDACTED]"));
        assert!(redacted.contains("GEMINI_API_KEY=[REDACTED]"));
    }

    #[test]
    fn redacts_bare_secret_tokens() {
        let text = "failure while handling sk-ant-api03-AAAAAAAAAAAAAAAA and AIzaToken123";
        let redacted = redact_sensitive_text(text);
        assert!(!redacted.contains("sk-ant-api03-AAAAAAAAAAAAAAAA"));
        assert!(!redacted.contains("AIzaToken123"));
        assert_eq!(redacted.matches("[REDACTED]").count(), 2);
    }
}

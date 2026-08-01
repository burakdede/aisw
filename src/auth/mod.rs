pub mod antigravity;
pub(crate) mod child_guard;
pub mod claude;
pub mod codex;
pub(crate) mod files;
pub mod gemini;
pub(crate) mod identity;
pub(crate) mod macos_keychain;
pub(crate) mod secure_backend;
pub(crate) mod secure_store;
pub(crate) mod system_keyring;
pub(crate) mod test_overrides;
pub mod token_expiry;

use anyhow::{bail, Result};

/// Reject an API key that is empty or contains control characters.
///
/// Control characters are refused because stored credentials are later
/// materialized into formats where they change meaning rather than being
/// escaped. Gemini writes `GEMINI_API_KEY=<key>` into a `.env` file the CLI
/// sources, so a key containing a newline injects arbitrary additional
/// environment variables (for example `GOOGLE_CLOUD_PROJECT`). A real key from
/// any of these providers is a single line of printable ASCII, so nothing
/// legitimate is rejected here.
pub(crate) fn validate_api_key_charset(key: &str, tool_label: &str, help: &str) -> Result<()> {
    if key.trim().is_empty() {
        bail!("{tool_label} API key must not be empty.\n  {help}");
    }
    if let Some(bad) = key.chars().find(|ch| ch.is_control()) {
        bail!(
            "{tool_label} API key contains an invalid control character ({}).\n  \
             Keys must be a single line — check for a stray newline from copy/paste \
             or from piping a file into --api-key.",
            match bad {
                '\n' => "newline".to_owned(),
                '\r' => "carriage return".to_owned(),
                '\t' => "tab".to_owned(),
                other => format!("U+{:04X}", other as u32),
            }
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::validate_api_key_charset;

    #[test]
    fn accepts_a_normal_key() {
        assert!(validate_api_key_charset("sk-ant-api03-AAAA", "Claude", "help").is_ok());
    }

    #[test]
    fn rejects_empty_and_whitespace_keys() {
        assert!(validate_api_key_charset("", "Claude", "help").is_err());
        assert!(validate_api_key_charset("   ", "Claude", "help").is_err());
    }

    /// A newline in the key would inject extra `KEY=value` lines into Gemini's
    /// generated `.env` file.
    #[test]
    fn rejects_control_characters() {
        for key in [
            "AIzaValid\nGOOGLE_CLOUD_PROJECT=attacker",
            "AIzaValid\rmore",
            "AIzaValid\tmore",
            "AIzaValid\u{0}more",
        ] {
            let err = validate_api_key_charset(key, "Gemini", "help").unwrap_err();
            assert!(
                err.to_string().contains("control character"),
                "expected rejection for {key:?}, got: {err}"
            );
        }
    }

    #[test]
    fn error_names_the_offending_character() {
        let err = validate_api_key_charset("abc\ndef", "Gemini", "help").unwrap_err();
        assert!(err.to_string().contains("newline"), "got: {err}");
    }
}

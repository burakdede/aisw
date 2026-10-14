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

/// Trim ASCII whitespace from the edges of a stored credential payload.
///
/// Agent credentials are JSON documents, so surrounding whitespace means
/// nothing to any reader — but it is not harmless. `security(1)` prints a
/// Keychain value containing a newline as hex rather than text, and Claude Code
/// reads its own item with that command and expects JSON, so a single stray
/// newline in a value aisw writes logs the user out of every session (#250).
///
/// Applying this on both read and persist is deliberate: a profile captured
/// before that fix carries the newline on disk, and trimming on read lets the
/// next switch repair the live item without a migration step. Trimming is safe
/// here, unlike at the `security` transport layer, precisely because the
/// payload is known to be JSON.
pub(crate) fn trim_credential_payload(bytes: &[u8]) -> &[u8] {
    bytes.trim_ascii()
}

/// Keep credential JSON readable by upstream security CLI callers on macOS.
pub(crate) fn normalize_keychain_json(bytes: &[u8]) -> Option<Vec<u8>> {
    use std::fmt::Write;

    let bytes = trim_credential_payload(bytes);
    let value: serde_json::Value = serde_json::from_slice(bytes).ok()?;
    if !value.is_object() {
        return None;
    }
    if bytes.is_ascii() && !bytes.iter().any(u8::is_ascii_control) {
        return Some(bytes.to_vec());
    }
    let json = serde_json::to_string(&value).ok()?;
    let mut ascii = String::with_capacity(json.len());
    for character in json.chars() {
        if character.is_ascii() && !character.is_control() {
            ascii.push(character);
        } else {
            for unit in character.encode_utf16(&mut [0; 2]) {
                write!(&mut ascii, "\\u{unit:04x}").ok()?;
            }
        }
    }
    Some(ascii.into_bytes())
}

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
    use super::{trim_credential_payload, validate_api_key_charset};

    #[test]
    fn trims_the_whitespace_that_makes_security_switch_to_hex() {
        assert_eq!(trim_credential_payload(b"{\"a\":1}\n"), b"{\"a\":1}");
        assert_eq!(
            trim_credential_payload(b"\n\t {\"a\":1} \r\n"),
            b"{\"a\":1}"
        );
        assert_eq!(trim_credential_payload(b"{\"a\":1}"), b"{\"a\":1}");
    }

    /// Only the edges. Whitespace inside a JSON string is part of the value —
    /// an access token is opaque and must survive byte-for-byte.
    #[test]
    fn leaves_whitespace_inside_the_payload_alone() {
        let payload = b"{\"token\":\"a b\\nc\"}";
        assert_eq!(trim_credential_payload(payload), payload);
    }

    #[test]
    fn trimming_is_idempotent_and_handles_degenerate_input() {
        let once = trim_credential_payload(b"  {}  ");
        assert_eq!(trim_credential_payload(once), once);
        assert_eq!(trim_credential_payload(b""), b"");
        assert_eq!(trim_credential_payload(b"   "), b"");
        assert_eq!(trim_credential_payload(b"\x00{}\x00"), b"\x00{}\x00");
    }

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

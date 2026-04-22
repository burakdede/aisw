use anyhow::{bail, Result};
use serde_json::Value;

use super::{claude, codex, gemini, secure_store};
use crate::config::{AuthMethod, Config, ConfigStore, CredentialBackend};
use crate::profile::ProfileStore;
use crate::types::Tool;

const CLAUDE_CREDENTIALS_FILE: &str = ".credentials.json";
const CLAUDE_OAUTH_ACCOUNT_FILE: &str = "oauth-account.json";
const CODEX_AUTH_FILE: &str = "auth.json";
const GEMINI_OAUTH_FILES: &[&str] = &["settings.json", "oauth_creds.json"];

pub fn ensure_unique_oauth_identity(
    profile_store: &ProfileStore,
    config_store: &ConfigStore,
    tool: Tool,
    pending_name: &str,
    pending_backend: CredentialBackend,
) -> Result<()> {
    let Some(identity) =
        resolve_oauth_identity(profile_store, tool, pending_name, pending_backend)?
    else {
        return Ok(());
    };

    let config = config_store.load()?;
    for existing_name in oauth_profile_names(&config, tool) {
        if existing_name == pending_name {
            continue;
        }

        let backend = config.profiles_for(tool)[existing_name].credential_backend;
        let Some(existing_identity) =
            resolve_oauth_identity(profile_store, tool, existing_name, backend)?
        else {
            continue;
        };

        if existing_identity == identity {
            bail!(
                "A {} OAuth profile for {} already exists as '{}'.\n  Use that profile or remove it before creating another alias for the same account.",
                tool,
                identity,
                existing_name
            );
        }
    }

    Ok(())
}

pub fn existing_oauth_profile_for_json_bytes(
    profile_store: &ProfileStore,
    config_store: &ConfigStore,
    tool: Tool,
    bytes: &[u8],
) -> Result<Option<String>> {
    let Some(identity) = resolve_identity_from_json_bytes(bytes)? else {
        return Ok(None);
    };

    let config = config_store.load()?;
    for existing_name in oauth_profile_names(&config, tool) {
        let backend = config.profiles_for(tool)[existing_name].credential_backend;
        let Some(existing_identity) =
            resolve_oauth_identity(profile_store, tool, existing_name, backend)?
        else {
            continue;
        };

        if existing_identity == identity {
            return Ok(Some(existing_name.to_owned()));
        }
    }

    Ok(None)
}

/// Treat API-key profiles as duplicates only on exact secret match.
///
/// This is intentionally narrower than "same account" because vendor auth docs
/// treat API keys as independently issued credentials, and users may
/// intentionally keep multiple keys for one account/project with different
/// operational purposes. We therefore minimize false positives and only treat a
/// profile as duplicate when the stored secret is byte-for-byte equal.
///
/// References:
/// - Anthropic Claude Code setup / API-key auth
/// - OpenAI developer quickstart / Create and export an API key
/// - Google AI Studio / Gemini API key setup
pub fn existing_api_key_profile_for_secret(
    profile_store: &ProfileStore,
    config_store: &ConfigStore,
    tool: Tool,
    secret: &str,
) -> Result<Option<String>> {
    let config = config_store.load()?;
    for existing_name in api_key_profile_names(&config, tool) {
        let existing_secret = read_api_key_for_profile(profile_store, tool, existing_name)?;
        if existing_secret == secret {
            return Ok(Some(existing_name.to_owned()));
        }
    }

    Ok(None)
}

fn oauth_profile_names(config: &Config, tool: Tool) -> Vec<&str> {
    let profiles = config.profiles_for(tool);

    profiles
        .iter()
        .filter_map(|(name, meta)| (meta.auth_method == AuthMethod::OAuth).then_some(name.as_str()))
        .collect()
}

fn api_key_profile_names(config: &Config, tool: Tool) -> Vec<&str> {
    let profiles = config.profiles_for(tool);

    profiles
        .iter()
        .filter_map(|(name, meta)| {
            (meta.auth_method == AuthMethod::ApiKey).then_some(name.as_str())
        })
        .collect()
}

fn read_api_key_for_profile(
    profile_store: &ProfileStore,
    tool: Tool,
    profile_name: &str,
) -> Result<String> {
    match tool {
        Tool::Claude => claude::read_api_key(profile_store, profile_name),
        Tool::Codex => codex::read_api_key(profile_store, profile_name),
        Tool::Gemini => gemini::read_api_key(profile_store, profile_name),
    }
}

fn resolve_oauth_identity(
    profile_store: &ProfileStore,
    tool: Tool,
    profile_name: &str,
    backend: CredentialBackend,
) -> Result<Option<String>> {
    if backend == CredentialBackend::SystemKeyring {
        let bytes = match tool {
            Tool::Claude | Tool::Codex => secure_store::read_profile_secret(tool, profile_name)?,
            Tool::Gemini => None,
        };
        return match bytes {
            Some(bytes) => resolve_identity_from_json_bytes(&bytes),
            None => Ok(None),
        };
    }

    let mut candidates = Vec::new();
    match tool {
        Tool::Claude => {
            candidates.push(CLAUDE_CREDENTIALS_FILE);
            candidates.push(CLAUDE_OAUTH_ACCOUNT_FILE);
        }
        Tool::Codex => candidates.push(CODEX_AUTH_FILE),
        Tool::Gemini => candidates.extend_from_slice(GEMINI_OAUTH_FILES),
    }

    for filename in candidates {
        let path = profile_store.profile_dir(tool, profile_name).join(filename);
        if !path.exists() {
            continue;
        }

        let bytes = profile_store.read_file(tool, profile_name, filename)?;
        if let Some(identity) = resolve_identity_from_json_bytes(&bytes)? {
            return Ok(Some(identity));
        }
    }

    Ok(None)
}

pub(crate) fn resolve_identity_from_json_bytes(bytes: &[u8]) -> Result<Option<String>> {
    let value: Value = match serde_json::from_slice(bytes) {
        Ok(value) => value,
        Err(_) => return Ok(None),
    };

    Ok(resolve_identity_from_value(&value))
}

fn resolve_identity_from_value(value: &Value) -> Option<String> {
    find_email(value)
        .or_else(|| find_subject(value))
        .map(normalize_identity)
}

fn find_email(value: &Value) -> Option<String> {
    walk_json(value, &|key, raw| {
        let trimmed = raw.trim();
        if matches!(
            key,
            Some("email" | "email_address" | "emailAddress" | "mail")
        ) && looks_like_email(trimmed)
        {
            return Some(trimmed.to_owned());
        }

        if looks_like_jwt(trimmed) {
            return decode_jwt_payload(trimmed)
                .ok()
                .flatten()
                .and_then(|payload| find_email(&payload));
        }

        None
    })
}

fn find_subject(value: &Value) -> Option<String> {
    walk_json(value, &|key, raw| {
        let trimmed = raw.trim();
        if matches!(
            key,
            Some("sub" | "subject" | "account_id" | "accountId" | "user_id" | "userId")
        ) && !trimmed.is_empty()
        {
            return Some(trimmed.to_owned());
        }

        if looks_like_jwt(trimmed) {
            return decode_jwt_payload(trimmed)
                .ok()
                .flatten()
                .and_then(|payload| find_subject(&payload));
        }

        None
    })
}

fn walk_json(
    value: &Value,
    visit: &dyn Fn(Option<&str>, &str) -> Option<String>,
) -> Option<String> {
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                if let Value::String(raw) = child {
                    if let Some(found) = visit(Some(key.as_str()), raw) {
                        return Some(found);
                    }
                }
                if let Some(found) = walk_json(child, visit) {
                    return Some(found);
                }
            }
            None
        }
        Value::Array(items) => {
            for item in items {
                if let Some(found) = walk_json(item, visit) {
                    return Some(found);
                }
            }
            None
        }
        Value::String(raw) => visit(None, raw),
        _ => None,
    }
}

fn looks_like_email(value: &str) -> bool {
    let mut parts = value.split('@');
    let local = parts.next().unwrap_or_default();
    let domain = parts.next().unwrap_or_default();
    !local.is_empty() && domain.contains('.') && parts.next().is_none()
}

fn normalize_identity(value: String) -> String {
    value.trim().to_ascii_lowercase()
}

fn looks_like_jwt(value: &str) -> bool {
    let mut parts = value.split('.');
    let first = parts.next().unwrap_or_default();
    let second = parts.next().unwrap_or_default();
    let third = parts.next().unwrap_or_default();
    !first.is_empty() && !second.is_empty() && !third.is_empty() && parts.next().is_none()
}

fn decode_jwt_payload(token: &str) -> Result<Option<Value>> {
    Ok(crate::util::jwt::decode_jwt_payload(token))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_direct_email_identity() {
        let value: Value =
            serde_json::from_str(r#"{"account":{"email":"Burak@Example.com"}}"#).unwrap();
        assert_eq!(
            resolve_identity_from_value(&value),
            Some("burak@example.com".to_owned())
        );
    }

    #[test]
    fn resolves_camel_case_email_identity() {
        let value: Value =
            serde_json::from_str(r#"{"oauthAccount":{"emailAddress":"Burak@Example.com"}}"#)
                .unwrap();
        assert_eq!(
            resolve_identity_from_value(&value),
            Some("burak@example.com".to_owned())
        );
    }

    #[test]
    fn resolves_subject_from_jwt_payload() {
        let token = "eyJhbGciOiJub25lIn0.eyJzdWIiOiJVU0VSLTEyMyJ9.sig";
        let value: Value = serde_json::from_str(&format!(r#"{{"id_token":"{}"}}"#, token)).unwrap();
        assert_eq!(
            resolve_identity_from_value(&value),
            Some("user-123".to_owned())
        );
    }

    #[test]
    fn ignores_non_json_payloads() {
        assert_eq!(resolve_identity_from_json_bytes(b"not-json").unwrap(), None);
    }

    #[test]
    fn resolves_subject_from_padded_jwt_payload() {
        use base64::engine::general_purpose::URL_SAFE;
        use base64::Engine;
        let payload = URL_SAFE.encode(br#"{"sub":"USER-1234"}"#);
        assert!(
            payload.ends_with('='),
            "test fixture should exercise padded base64url decoding"
        );
        let token = format!("eyJhbGciOiJub25lIn0.{payload}.sig");
        let value: Value = serde_json::from_str(&format!(r#"{{"id_token":"{}"}}"#, token)).unwrap();
        assert_eq!(
            resolve_identity_from_value(&value),
            Some("user-1234".to_owned())
        );
    }

    #[test]
    fn invalid_base64url_payload_is_ignored() {
        let value: Value =
            serde_json::from_str(r#"{"id_token":"eyJhbGciOiJub25lIn0.bad$payload.sig"}"#).unwrap();
        assert_eq!(resolve_identity_from_value(&value), None);
    }
}

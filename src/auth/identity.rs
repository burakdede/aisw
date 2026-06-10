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

#[derive(Clone, Debug, PartialEq, Eq)]
enum OAuthIdentity {
    Claude {
        email: Option<String>,
        organization_uuid: Option<String>,
        fallback: Option<String>,
    },
    Codex {
        email: Option<String>,
        account_id: Option<String>,
        fallback: Option<String>,
    },
    Generic(String),
}

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

        if existing_identity.matches(&identity) {
            bail!(
                "A {} OAuth profile for {} already exists as '{}'.\n  Use that profile or remove it before creating another alias for the same account.",
                tool,
                identity.display(),
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
    existing_oauth_profile_for_identity(
        profile_store,
        config_store,
        tool,
        resolve_identity_from_json_bytes_for_tool(tool, bytes)?,
    )
}

pub fn existing_claude_oauth_profile_for_live_state(
    profile_store: &ProfileStore,
    config_store: &ConfigStore,
    credential_bytes: &[u8],
    account_bytes: Option<&[u8]>,
) -> Result<Option<String>> {
    let identity = resolve_claude_identity(Some(credential_bytes), account_bytes)?;
    existing_oauth_profile_for_identity(profile_store, config_store, Tool::Claude, identity)
}

fn existing_oauth_profile_for_identity(
    profile_store: &ProfileStore,
    config_store: &ConfigStore,
    tool: Tool,
    identity: Option<OAuthIdentity>,
) -> Result<Option<String>> {
    let Some(identity) = identity else {
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

        if existing_identity.matches(&identity) {
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
        let existing_secret =
            read_api_key_for_profile(profile_store, &config, tool, existing_name)?;
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
    config: &Config,
    tool: Tool,
    profile_name: &str,
) -> Result<String> {
    let backend = config.profiles_for(tool)[profile_name].credential_backend;
    match tool {
        Tool::Claude => claude::read_api_key_with_backend(profile_store, profile_name, backend),
        Tool::Codex => codex::read_api_key_with_backend(profile_store, profile_name, backend),
        Tool::Gemini => gemini::read_api_key(profile_store, profile_name),
    }
}

fn resolve_oauth_identity(
    profile_store: &ProfileStore,
    tool: Tool,
    profile_name: &str,
    backend: CredentialBackend,
) -> Result<Option<OAuthIdentity>> {
    match tool {
        Tool::Claude => {
            let credential_bytes = if backend == CredentialBackend::SystemKeyring {
                secure_store::read_profile_secret(tool, profile_name)?
            } else {
                read_optional_profile_file(
                    profile_store,
                    tool,
                    profile_name,
                    CLAUDE_CREDENTIALS_FILE,
                )?
            };
            let account_bytes = read_optional_profile_file(
                profile_store,
                tool,
                profile_name,
                CLAUDE_OAUTH_ACCOUNT_FILE,
            )?;
            resolve_claude_identity(credential_bytes.as_deref(), account_bytes.as_deref())
        }
        Tool::Codex => resolve_identity_from_optional_profile_files(
            profile_store,
            tool,
            profile_name,
            backend,
            &[CODEX_AUTH_FILE],
        ),
        Tool::Gemini => resolve_identity_from_optional_profile_files(
            profile_store,
            tool,
            profile_name,
            backend,
            GEMINI_OAUTH_FILES,
        ),
    }
}

fn resolve_identity_from_optional_profile_files(
    profile_store: &ProfileStore,
    tool: Tool,
    profile_name: &str,
    backend: CredentialBackend,
    filenames: &[&str],
) -> Result<Option<OAuthIdentity>> {
    if backend == CredentialBackend::SystemKeyring {
        let bytes = match tool {
            Tool::Claude | Tool::Codex => secure_store::read_profile_secret(tool, profile_name)?,
            Tool::Gemini => None,
        };
        return match bytes {
            Some(bytes) => resolve_identity_from_json_bytes_for_tool(tool, &bytes),
            None => Ok(None),
        };
    }

    for filename in filenames {
        let path = profile_store.profile_dir(tool, profile_name).join(filename);
        if !path.exists() {
            continue;
        }

        let bytes = profile_store.read_file(tool, profile_name, filename)?;
        if let Some(identity) = resolve_identity_from_json_bytes_for_tool(tool, &bytes)? {
            return Ok(Some(identity));
        }
    }

    Ok(None)
}

fn read_optional_profile_file(
    profile_store: &ProfileStore,
    tool: Tool,
    profile_name: &str,
    filename: &str,
) -> Result<Option<Vec<u8>>> {
    let path = profile_store.profile_dir(tool, profile_name).join(filename);
    if !path.exists() {
        return Ok(None);
    }

    profile_store
        .read_file(tool, profile_name, filename)
        .map(Some)
}

#[cfg(test)]
pub(crate) fn resolve_identity_from_json_bytes(bytes: &[u8]) -> Result<Option<String>> {
    Ok(
        resolve_identity_from_json_bytes_for_tool(Tool::Codex, bytes)?.map(
            |identity| match identity {
                OAuthIdentity::Claude { .. } => unreachable!(),
                OAuthIdentity::Codex { .. } => unreachable!(),
                OAuthIdentity::Generic(identity) => identity,
            },
        ),
    )
}

fn resolve_identity_from_json_bytes_for_tool(
    tool: Tool,
    bytes: &[u8],
) -> Result<Option<OAuthIdentity>> {
    let value: Value = match serde_json::from_slice(bytes) {
        Ok(value) => value,
        Err(_) => return Ok(None),
    };

    Ok(resolve_identity_from_value(tool, &value))
}

fn resolve_claude_identity(
    credential_bytes: Option<&[u8]>,
    account_bytes: Option<&[u8]>,
) -> Result<Option<OAuthIdentity>> {
    let credential_value = credential_bytes
        .map(serde_json::from_slice::<Value>)
        .transpose()
        .ok()
        .flatten();
    let account_value = account_bytes
        .map(serde_json::from_slice::<Value>)
        .transpose()
        .ok()
        .flatten();

    let email = account_value
        .as_ref()
        .and_then(find_email)
        .or_else(|| credential_value.as_ref().and_then(find_email));
    let organization_uuid = account_value
        .as_ref()
        .and_then(find_claude_organization_uuid);
    let fallback = credential_value
        .as_ref()
        .and_then(find_subject)
        .or_else(|| account_value.as_ref().and_then(find_subject))
        .map(normalize_identity);

    if email.is_none() && organization_uuid.is_none() && fallback.is_none() {
        return Ok(None);
    }

    Ok(Some(OAuthIdentity::Claude {
        email: email.map(normalize_identity),
        organization_uuid: organization_uuid.map(normalize_identity),
        fallback,
    }))
}

fn resolve_identity_from_value(tool: Tool, value: &Value) -> Option<OAuthIdentity> {
    match tool {
        Tool::Claude => {
            let email = find_email(value).map(normalize_identity);
            let organization_uuid = find_claude_organization_uuid(value).map(normalize_identity);
            let fallback = find_subject(value).map(normalize_identity);
            if email.is_none() && organization_uuid.is_none() && fallback.is_none() {
                return None;
            }
            Some(OAuthIdentity::Claude {
                email,
                organization_uuid,
                fallback,
            })
        }
        Tool::Codex => {
            let email = find_codex_email(value)
                .or_else(|| find_email(value))
                .map(normalize_identity);
            let account_id = find_codex_account_id(value).map(normalize_identity);
            let fallback = find_codex_subject(value).map(normalize_identity);

            if email.is_none() && account_id.is_none() && fallback.is_none() {
                return None;
            }

            Some(OAuthIdentity::Codex {
                email,
                account_id,
                fallback,
            })
        }
        Tool::Gemini => find_email(value)
            .or_else(|| find_subject(value))
            .map(normalize_identity)
            .map(OAuthIdentity::Generic),
    }
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

fn find_codex_email(value: &Value) -> Option<String> {
    walk_json(value, &|key, raw| {
        let trimmed = raw.trim();
        if matches!(key, Some("primaryEmail")) && looks_like_email(trimmed) {
            return Some(trimmed.to_owned());
        }
        None
    })
}

fn find_codex_subject(value: &Value) -> Option<String> {
    walk_json(value, &|key, raw| {
        let trimmed = raw.trim();
        if matches!(key, Some("sub" | "subject" | "user_id" | "userId")) && !trimmed.is_empty() {
            return Some(trimmed.to_owned());
        }

        if looks_like_jwt(trimmed) {
            return decode_jwt_payload(trimmed)
                .ok()
                .flatten()
                .and_then(|payload| find_codex_subject(&payload));
        }

        None
    })
}

fn find_codex_account_id(value: &Value) -> Option<String> {
    walk_json(value, &|key, raw| {
        let trimmed = raw.trim();
        if matches!(key, Some("account_id" | "accountId")) && !trimmed.is_empty() {
            return Some(trimmed.to_owned());
        }

        None
    })
}

fn find_claude_organization_uuid(value: &Value) -> Option<String> {
    walk_json(value, &|key, raw| {
        matches!(key, Some("organizationUuid"))
            .then(|| raw.trim())
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
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

impl OAuthIdentity {
    fn matches(&self, other: &Self) -> bool {
        match (self, other) {
            (
                OAuthIdentity::Claude {
                    email: left_email,
                    organization_uuid: left_org,
                    fallback: left_fallback,
                },
                OAuthIdentity::Claude {
                    email: right_email,
                    organization_uuid: right_org,
                    fallback: right_fallback,
                },
            ) => {
                if let (Some(left_email), Some(right_email)) = (left_email, right_email) {
                    if left_email != right_email {
                        return false;
                    }

                    return match (left_org, right_org) {
                        (Some(left_org), Some(right_org)) => left_org == right_org,
                        _ => true,
                    };
                }

                left_fallback.is_some() && left_fallback == right_fallback
            }
            (
                OAuthIdentity::Codex {
                    email: left_email,
                    account_id: left_account_id,
                    fallback: left_fallback,
                },
                OAuthIdentity::Codex {
                    email: right_email,
                    account_id: right_account_id,
                    fallback: right_fallback,
                },
            ) => {
                if let (Some(left_email), Some(right_email)) = (left_email, right_email) {
                    if left_email != right_email {
                        return false;
                    }

                    return match (left_account_id, right_account_id) {
                        (Some(left_account_id), Some(right_account_id)) => {
                            left_account_id == right_account_id
                        }
                        _ => true,
                    };
                }

                left_fallback.is_some() && left_fallback == right_fallback
            }
            (OAuthIdentity::Generic(left), OAuthIdentity::Generic(right)) => left == right,
            _ => false,
        }
    }

    fn display(&self) -> &str {
        match self {
            OAuthIdentity::Claude {
                email, fallback, ..
            } => email
                .as_deref()
                .or(fallback.as_deref())
                .unwrap_or("unknown account"),
            OAuthIdentity::Codex {
                email, fallback, ..
            } => email
                .as_deref()
                .or(fallback.as_deref())
                .unwrap_or("unknown account"),
            OAuthIdentity::Generic(identity) => identity,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_direct_email_identity() {
        let value: Value =
            serde_json::from_str(r#"{"account":{"email":"Burak@Example.com"}}"#).unwrap();
        assert_eq!(
            resolve_identity_from_value(Tool::Codex, &value),
            Some(OAuthIdentity::Codex {
                email: Some("burak@example.com".to_owned()),
                account_id: None,
                fallback: None,
            })
        );
    }

    #[test]
    fn resolves_camel_case_email_identity() {
        let value: Value =
            serde_json::from_str(r#"{"oauthAccount":{"emailAddress":"Burak@Example.com"}}"#)
                .unwrap();
        assert_eq!(
            resolve_identity_from_value(Tool::Claude, &value),
            Some(OAuthIdentity::Claude {
                email: Some("burak@example.com".to_owned()),
                organization_uuid: None,
                fallback: None,
            })
        );
    }

    #[test]
    fn resolves_subject_from_jwt_payload() {
        let token = "eyJhbGciOiJub25lIn0.eyJzdWIiOiJVU0VSLTEyMyJ9.sig";
        let value: Value = serde_json::from_str(&format!(r#"{{"id_token":"{}"}}"#, token)).unwrap();
        assert_eq!(
            resolve_identity_from_value(Tool::Codex, &value),
            Some(OAuthIdentity::Codex {
                email: None,
                account_id: None,
                fallback: Some("user-123".to_owned()),
            })
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
            resolve_identity_from_value(Tool::Codex, &value),
            Some(OAuthIdentity::Codex {
                email: None,
                account_id: None,
                fallback: Some("user-1234".to_owned()),
            })
        );
    }

    #[test]
    fn invalid_base64url_payload_is_ignored() {
        let value: Value =
            serde_json::from_str(r#"{"id_token":"eyJhbGciOiJub25lIn0.bad$payload.sig"}"#).unwrap();
        assert_eq!(resolve_identity_from_value(Tool::Codex, &value), None);
    }

    #[test]
    fn resolves_codex_identity_with_account_id() {
        let value: Value = serde_json::from_str(
            r#"{"primaryEmail":"Burak@Example.com","tokens":{"account_id":"acc-workspace"}}"#,
        )
        .unwrap();
        assert_eq!(
            resolve_identity_from_value(Tool::Codex, &value),
            Some(OAuthIdentity::Codex {
                email: Some("burak@example.com".to_owned()),
                account_id: Some("acc-workspace".to_owned()),
                fallback: None,
            })
        );
    }

    #[test]
    fn codex_identity_distinguishes_same_email_and_different_account_id() {
        let left = OAuthIdentity::Codex {
            email: Some("work@example.com".to_owned()),
            account_id: Some("acc-a".to_owned()),
            fallback: None,
        };
        let right = OAuthIdentity::Codex {
            email: Some("work@example.com".to_owned()),
            account_id: Some("acc-b".to_owned()),
            fallback: None,
        };
        assert!(!left.matches(&right));
    }

    #[test]
    fn codex_identity_matches_same_email_and_same_account_id() {
        let left = OAuthIdentity::Codex {
            email: Some("work@example.com".to_owned()),
            account_id: Some("acc-a".to_owned()),
            fallback: None,
        };
        let right = OAuthIdentity::Codex {
            email: Some("work@example.com".to_owned()),
            account_id: Some("acc-a".to_owned()),
            fallback: None,
        };
        assert!(left.matches(&right));
    }

    #[test]
    fn resolves_claude_identity_with_org_uuid() {
        let identity = resolve_claude_identity(
            Some(br#"{"account":{"email":"Work@Example.com"}}"#),
            Some(br#"{"emailAddress":"Work@Example.com","organizationUuid":"ORG-123"}"#),
        )
        .unwrap();

        assert_eq!(
            identity,
            Some(OAuthIdentity::Claude {
                email: Some("work@example.com".to_owned()),
                organization_uuid: Some("org-123".to_owned()),
                fallback: None,
            })
        );
    }

    #[test]
    fn claude_identity_matches_same_email_and_same_org() {
        let left = OAuthIdentity::Claude {
            email: Some("work@example.com".to_owned()),
            organization_uuid: Some("org-123".to_owned()),
            fallback: None,
        };
        let right = OAuthIdentity::Claude {
            email: Some("work@example.com".to_owned()),
            organization_uuid: Some("org-123".to_owned()),
            fallback: None,
        };
        assert!(left.matches(&right));
    }

    #[test]
    fn claude_identity_allows_same_email_with_different_org() {
        let left = OAuthIdentity::Claude {
            email: Some("work@example.com".to_owned()),
            organization_uuid: Some("org-123".to_owned()),
            fallback: None,
        };
        let right = OAuthIdentity::Claude {
            email: Some("work@example.com".to_owned()),
            organization_uuid: Some("org-456".to_owned()),
            fallback: None,
        };
        assert!(!left.matches(&right));
    }

    #[test]
    fn claude_identity_preserves_email_only_matching_when_org_is_missing() {
        let left = OAuthIdentity::Claude {
            email: Some("work@example.com".to_owned()),
            organization_uuid: Some("org-123".to_owned()),
            fallback: None,
        };
        let right = OAuthIdentity::Claude {
            email: Some("work@example.com".to_owned()),
            organization_uuid: None,
            fallback: None,
        };
        assert!(left.matches(&right));
    }
}

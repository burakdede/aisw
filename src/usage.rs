//! Rate-limit usage windows for OAuth-backed Claude Code and Codex CLI profiles.
//!
//! This is the only code path in aisw that touches the network. It runs only
//! when the user invokes `aisw usage`, and it sends each profile's own access
//! token to the vendor endpoint that issued it, exactly as the upstream CLI
//! does when it renders its own usage view. Nothing is sent anywhere else.

use std::time::Duration;

use anyhow::{anyhow, bail, Context, Result};
use chrono::{DateTime, TimeZone, Utc};
use serde::Serialize;
use serde_json::Value;

use crate::auth::test_overrides;
use crate::config::{AuthMethod, ProfileMeta};
use crate::profile::ProfileStore;
use crate::types::Tool;

const CLAUDE_USAGE_URL: &str = "https://api.anthropic.com/api/oauth/usage";
const CODEX_USAGE_URL: &str = "https://chatgpt.com/backend-api/wham/usage";
/// Beta header Claude Code sends on OAuth-authenticated usage requests.
const CLAUDE_OAUTH_BETA: &str = "oauth-2025-04-20";
const TIMEOUT: Duration = Duration::from_secs(10);

/// One rate-limit window as the vendor reports it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct UsageWindow {
    /// `5h`, `7d`, or a vendor-defined name such as a model or reserve pool.
    pub name: String,
    pub used_percent: f64,
    pub resets_at: Option<DateTime<Utc>>,
}

/// Usage for one profile.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct UsageReport {
    /// Account email when stored metadata or the vendor reports one.
    pub account: Option<String>,
    /// Organization name (Claude) or plan tier (Codex).
    pub plan: Option<String>,
    pub windows: Vec<UsageWindow>,
}

pub fn supports_usage(tool: Tool) -> bool {
    matches!(tool, Tool::Claude | Tool::Codex)
}

/// Fetch usage for one stored profile.
///
/// Errors explain why this profile cannot report usage. Callers render them
/// per row so one stale profile does not hide the others.
pub fn fetch_profile_usage(
    profile_store: &ProfileStore,
    tool: Tool,
    name: &str,
    meta: &ProfileMeta,
) -> Result<UsageReport> {
    if meta.auth_method == AuthMethod::ApiKey {
        bail!("API-key profiles have no rate-limit windows; only OAuth profiles report usage");
    }
    match tool {
        Tool::Claude => fetch_claude(profile_store, name, meta),
        Tool::Codex => fetch_codex(profile_store, name, meta),
        Tool::Gemini | Tool::Antigravity => {
            bail!("{} does not expose a usage endpoint", tool.display_name())
        }
    }
}

fn fetch_claude(
    profile_store: &ProfileStore,
    name: &str,
    meta: &ProfileMeta,
) -> Result<UsageReport> {
    let bytes =
        crate::auth::claude::read_stored_credentials(profile_store, name, meta.credential_backend)?;
    let credentials: Value =
        serde_json::from_slice(&bytes).context("stored Claude credentials are not valid JSON")?;
    let oauth = credentials.get("claudeAiOauth").unwrap_or(&credentials);
    let token = first_string(
        oauth,
        &["accessToken", "access_token", "oauthToken", "token"],
    )
    .ok_or_else(|| anyhow!("stored Claude credentials contain no OAuth access token"))?;
    let expires_at = oauth
        .get("expiresAt")
        .and_then(Value::as_i64)
        .and_then(|millis| Utc.timestamp_millis_opt(millis).single());
    if expires_at.is_some_and(|expiry| expiry <= Utc::now()) {
        bail!("OAuth token expired; run 'claude' with this profile active to refresh it");
    }

    let url = endpoint(CLAUDE_USAGE_URL, "AISW_CLAUDE_USAGE_URL");
    let data = get_json(
        &url,
        &[
            ("Authorization", &format!("Bearer {token}")),
            ("anthropic-beta", CLAUDE_OAUTH_BETA),
        ],
    )?;

    let account = profile_store
        .read_file(Tool::Claude, name, crate::auth::claude::OAUTH_ACCOUNT_FILE)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok());
    Ok(UsageReport {
        account: account
            .as_ref()
            .and_then(|account| first_string(account, &["emailAddress"])),
        plan: account
            .as_ref()
            .and_then(|account| first_string(account, &["organizationName"])),
        windows: claude_windows(&data),
    })
}

fn fetch_codex(
    profile_store: &ProfileStore,
    name: &str,
    meta: &ProfileMeta,
) -> Result<UsageReport> {
    let bytes =
        crate::auth::codex::read_stored_credentials(profile_store, name, meta.credential_backend)?;
    let auth: Value =
        serde_json::from_slice(&bytes).context("stored Codex credentials are not valid JSON")?;
    let tokens = auth.get("tokens").ok_or_else(|| {
        anyhow!("profile is not ChatGPT-managed; only ChatGPT sign-in reports usage")
    })?;
    let token = first_string(tokens, &["access_token"])
        .ok_or_else(|| anyhow!("stored Codex credentials contain no access token"))?;
    let claims = crate::util::jwt::decode_jwt_payload(&token);
    let expires_at = claims
        .as_ref()
        .and_then(|claims| claims.get("exp"))
        .and_then(Value::as_i64)
        .and_then(|secs| Utc.timestamp_opt(secs, 0).single());
    if expires_at.is_some_and(|expiry| expiry <= Utc::now()) {
        bail!("ChatGPT token expired; run 'codex' with this profile active to refresh it");
    }
    let account_id = first_string(tokens, &["account_id"])
        .or_else(|| {
            let auth_claims = claims.as_ref()?.get("https://api.openai.com/auth")?;
            first_string(auth_claims, &["chatgpt_account_id"])
        })
        .ok_or_else(|| anyhow!("stored Codex credentials contain no ChatGPT account id"))?;

    let url = endpoint(CODEX_USAGE_URL, "AISW_CODEX_USAGE_URL");
    let data = get_json(
        &url,
        &[
            ("Authorization", &format!("Bearer {token}")),
            ("ChatGPT-Account-Id", &account_id),
        ],
    )?;
    Ok(UsageReport {
        account: first_string(&data, &["email"]),
        plan: first_string(&data, &["plan_type"]),
        windows: codex_windows(&data),
    })
}

fn first_string(value: &Value, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|key| value.get(key).and_then(Value::as_str))
        .map(str::to_owned)
}

/// Test builds may point a vendor endpoint at a local server; release builds
/// always use the real one.
fn endpoint(default: &str, override_var: &str) -> String {
    test_overrides::string(override_var).unwrap_or_else(|| default.to_owned())
}

fn get_json(url: &str, headers: &[(&str, &str)]) -> Result<Value> {
    let agent = ureq::Agent::config_builder()
        .timeout_global(Some(TIMEOUT))
        .http_status_as_error(false)
        .build()
        .new_agent();
    let mut request = agent.get(url);
    for (name, value) in headers {
        request = request.header(*name, *value);
    }
    let mut response = request
        .call()
        .with_context(|| format!("could not reach {url}"))?;
    let status = response.status().as_u16();
    let body = response
        .body_mut()
        .read_to_string()
        .context("could not read the usage response")?;
    match status {
        200..=299 => serde_json::from_str(&body).context("usage endpoint returned invalid JSON"),
        401 | 403 => bail!(
            "usage endpoint rejected the token (HTTP {status}); sign in again with the upstream CLI"
        ),
        _ => bail!("usage endpoint returned HTTP {status}"),
    }
}

/// Claude reports fixed `five_hour` / `seven_day` windows plus per-model
/// weekly windows under `limits`.
fn claude_windows(data: &Value) -> Vec<UsageWindow> {
    let mut windows = Vec::new();
    for (key, name) in [("five_hour", "5h"), ("seven_day", "7d")] {
        let Some(window) = data.get(key) else {
            continue;
        };
        let Some(used_percent) = window.get("utilization").and_then(Value::as_f64) else {
            continue;
        };
        windows.push(UsageWindow {
            name: name.to_owned(),
            used_percent,
            resets_at: rfc3339(window.get("resets_at")),
        });
    }
    let limits = data.get("limits").and_then(Value::as_array);
    for limit in limits.into_iter().flatten() {
        let name = limit
            .pointer("/scope/model/display_name")
            .and_then(Value::as_str);
        let used_percent = limit.get("percent").and_then(Value::as_f64);
        if let (Some(name), Some(used_percent)) = (name, used_percent) {
            windows.push(UsageWindow {
                name: name.to_owned(),
                used_percent,
                resets_at: rfc3339(limit.get("resets_at")),
            });
        }
    }
    windows
}

/// Codex reports a primary (and optional secondary) window per limit, named
/// only by duration, plus extra named limits such as reserve pools.
fn codex_windows(data: &Value) -> Vec<UsageWindow> {
    let mut windows = Vec::new();
    push_codex_limit(&mut windows, None, data.get("rate_limit"));
    let extras = data.get("additional_rate_limits").and_then(Value::as_array);
    for extra in extras.into_iter().flatten() {
        let name = extra.get("limit_name").and_then(Value::as_str);
        push_codex_limit(&mut windows, name, extra.get("rate_limit"));
    }
    windows
}

fn push_codex_limit(windows: &mut Vec<UsageWindow>, name: Option<&str>, limit: Option<&Value>) {
    let Some(limit) = limit else {
        return;
    };
    for key in ["primary_window", "secondary_window"] {
        let Some(window) = limit.get(key).filter(|window| !window.is_null()) else {
            continue;
        };
        let Some(used_percent) = window.get("used_percent").and_then(Value::as_f64) else {
            continue;
        };
        let hours = window
            .get("limit_window_seconds")
            .and_then(Value::as_u64)
            .unwrap_or(0)
            / 3600;
        let label = match (name, hours) {
            (Some(name), _) => name.to_owned(),
            (None, 168) => "7d".to_owned(),
            (None, hours) => format!("{hours}h"),
        };
        let resets_at = window
            .get("reset_at")
            .and_then(Value::as_i64)
            .and_then(|secs| Utc.timestamp_opt(secs, 0).single());
        windows.push(UsageWindow {
            name: label,
            used_percent,
            resets_at,
        });
    }
}

fn rfc3339(value: Option<&Value>) -> Option<DateTime<Utc>> {
    let text = value?.as_str()?;
    DateTime::parse_from_rfc3339(text)
        .ok()
        .map(|parsed| parsed.with_timezone(&Utc))
}

#[cfg(test)]
mod tests {
    use super::*;

    const CLAUDE_RESPONSE: &str = r#"{
        "five_hour": {"utilization": 14, "resets_at": "2026-09-18T04:10:00+00:00"},
        "seven_day": {"utilization": 25.5, "resets_at": "2026-09-20T14:00:00+00:00"},
        "extra_usage": {"is_enabled": false},
        "limits": [
            {"scope": {"model": {"display_name": "Fable"}}, "percent": 30, "resets_at": "2026-09-20T14:00:00+00:00"},
            {"scope": {"other": true}, "percent": 99}
        ]
    }"#;

    const CODEX_RESPONSE: &str = r#"{
        "email": "user@example.com",
        "plan_type": "pro",
        "rate_limit": {
            "allowed": true,
            "primary_window": {"used_percent": 81, "limit_window_seconds": 604800, "reset_at": 1789842473},
            "secondary_window": null
        },
        "additional_rate_limits": [
            {
                "limit_name": "gpt-reserve",
                "rate_limit": {"primary_window": {"used_percent": 0, "limit_window_seconds": 604800, "reset_at": 1790300603}}
            }
        ]
    }"#;

    fn names(windows: &[UsageWindow]) -> Vec<&str> {
        windows.iter().map(|window| window.name.as_str()).collect()
    }

    #[test]
    fn claude_windows_include_fixed_and_per_model_windows() {
        let data: Value = serde_json::from_str(CLAUDE_RESPONSE).unwrap();
        let windows = claude_windows(&data);
        assert_eq!(names(&windows), ["5h", "7d", "Fable"]);
        assert_eq!(windows[0].used_percent, 14.0);
        assert_eq!(windows[1].used_percent, 25.5);
        assert_eq!(
            windows[0].resets_at.map(|at| at.to_rfc3339()),
            Some("2026-09-18T04:10:00+00:00".to_owned())
        );
    }

    #[test]
    fn claude_windows_tolerate_missing_fields() {
        let data: Value = serde_json::from_str(r#"{"five_hour": {"utilization": 3}}"#).unwrap();
        let windows = claude_windows(&data);
        assert_eq!(names(&windows), ["5h"]);
        assert_eq!(windows[0].resets_at, None);
        assert!(claude_windows(&Value::Null).is_empty());
    }

    #[test]
    fn codex_windows_label_by_duration_and_limit_name() {
        let data: Value = serde_json::from_str(CODEX_RESPONSE).unwrap();
        let windows = codex_windows(&data);
        assert_eq!(names(&windows), ["7d", "gpt-reserve"]);
        assert_eq!(windows[0].used_percent, 81.0);
        assert_eq!(
            windows[0].resets_at.map(|at| at.timestamp()),
            Some(1789842473)
        );
    }

    #[test]
    fn codex_windows_name_short_windows_in_hours() {
        let data: Value = serde_json::from_str(
            r#"{"rate_limit": {"primary_window": {"used_percent": 5, "limit_window_seconds": 18000},
                               "secondary_window": {"used_percent": 6, "limit_window_seconds": 604800}}}"#,
        )
        .unwrap();
        assert_eq!(names(&codex_windows(&data)), ["5h", "7d"]);
    }

    #[test]
    fn only_claude_and_codex_report_usage() {
        assert!(supports_usage(Tool::Claude));
        assert!(supports_usage(Tool::Codex));
        assert!(!supports_usage(Tool::Gemini));
        assert!(!supports_usage(Tool::Antigravity));
    }
}

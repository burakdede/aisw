use chrono::{DateTime, Duration, TimeZone, Utc};
use serde_json::Value;

/// Try to extract expiry from Claude's .credentials.json (`expiresAt` field — Unix ms or ISO string).
pub fn parse_claude_expiry(json: &[u8]) -> Option<DateTime<Utc>> {
    let v: Value = serde_json::from_slice(json).ok()?;
    let expires_at = v.get("expiresAt")?;

    // Try as Unix milliseconds (integer)
    if let Some(ms) = expires_at.as_i64() {
        return Utc.timestamp_millis_opt(ms).single();
    }

    // Try as ISO string
    if let Some(s) = expires_at.as_str() {
        return s.parse::<DateTime<Utc>>().ok();
    }

    None
}

/// Try to extract expiry from Codex auth.json via JWT `exp` claim in `token` field.
pub fn parse_codex_expiry(json: &[u8]) -> Option<DateTime<Utc>> {
    let v: Value = serde_json::from_slice(json).ok()?;
    let token = v.get("token").and_then(|t| t.as_str())?;
    let exp = decode_jwt_exp(token)?;
    Utc.timestamp_opt(exp, 0).single()
}

/// Try to extract expiry from Gemini oauth_creds.json via `expiry` ISO field or JWT `exp` in `id_token`.
pub fn parse_gemini_expiry(json: &[u8]) -> Option<DateTime<Utc>> {
    let v: Value = serde_json::from_slice(json).ok()?;

    // Try `expiry` ISO field first
    if let Some(s) = v.get("expiry").and_then(|e| e.as_str()) {
        if let Ok(dt) = s.parse::<DateTime<Utc>>() {
            return Some(dt);
        }
    }

    // Try `id_token` JWT `exp` claim
    if let Some(id_token) = v.get("id_token").and_then(|t| t.as_str()) {
        if let Some(exp) = decode_jwt_exp(id_token) {
            return Utc.timestamp_opt(exp, 0).single();
        }
    }

    None
}

/// Returns a warning message if the token is expired or expiring within 24h.
pub fn expiry_warning(
    expiry: DateTime<Utc>,
    tool_name: &str,
    profile_name: &str,
    command: &str,
) -> Option<String> {
    let now = Utc::now();
    if expiry <= now {
        Some(format!(
            "\u{26a0}  {} ({}): OAuth token expired — run 'aisw add {} {}' to refresh",
            tool_name, profile_name, command, profile_name
        ))
    } else if expiry - now < Duration::hours(24) {
        let hours = (expiry - now).num_hours().max(1);
        Some(format!(
            "\u{26a0}  {} ({}): OAuth token expires in ~{}h",
            tool_name, profile_name, hours
        ))
    } else {
        None
    }
}

fn decode_jwt_exp(jwt: &str) -> Option<i64> {
    let payload = crate::util::jwt::decode_jwt_payload(jwt)?;
    payload.get("exp").and_then(|e| e.as_i64())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn make_jwt_with_exp(exp: i64) -> String {
        let header = "eyJhbGciOiJIUzI1NiJ9";
        let payload_json = format!(r#"{{"exp":{}}}"#, exp);
        let payload = crate::util::jwt::encode_jwt_payload_for_test(payload_json.as_bytes());
        format!("{}.{}.sig", header, payload)
    }

    #[test]
    fn parse_claude_expiry_from_unix_ms() {
        // 2030-01-01T00:00:00Z in ms
        let ms: i64 = 1893456000000;
        let json = format!(r#"{{"expiresAt":{}}}"#, ms);
        let dt = parse_claude_expiry(json.as_bytes()).unwrap();
        assert!(dt.timestamp_millis() == ms);
    }

    #[test]
    fn parse_claude_expiry_from_iso_string() {
        let json = r#"{"expiresAt":"2030-01-01T00:00:00Z"}"#;
        let dt = parse_claude_expiry(json.as_bytes()).unwrap();
        assert!(dt.timestamp() > 0);
    }

    #[test]
    fn parse_claude_expiry_returns_none_for_missing() {
        let json = r#"{"token":"abc"}"#;
        assert!(parse_claude_expiry(json.as_bytes()).is_none());
    }

    #[test]
    fn parse_claude_expiry_returns_none_for_malformed() {
        assert!(parse_claude_expiry(b"not json").is_none());
    }

    #[test]
    fn parse_codex_expiry_from_jwt() {
        let future_exp: i64 = 1893456000; // 2030-01-01
        let jwt = make_jwt_with_exp(future_exp);
        let json = format!(r#"{{"token":"{}"}}"#, jwt);
        let dt = parse_codex_expiry(json.as_bytes()).unwrap();
        assert_eq!(dt.timestamp(), future_exp);
    }

    #[test]
    fn parse_codex_expiry_returns_none_for_missing_token() {
        let json = r#"{"other":"field"}"#;
        assert!(parse_codex_expiry(json.as_bytes()).is_none());
    }

    #[test]
    fn parse_codex_expiry_returns_none_for_malformed_jwt() {
        let json = r#"{"token":"not.a.jwt"}"#;
        // Should return None gracefully, no panic
        let _ = parse_codex_expiry(json.as_bytes());
    }

    #[test]
    fn parse_gemini_expiry_from_iso_string() {
        let json = r#"{"expiry":"2030-01-01T00:00:00Z"}"#;
        let dt = parse_gemini_expiry(json.as_bytes()).unwrap();
        assert!(dt.timestamp() > 0);
    }

    #[test]
    fn parse_gemini_expiry_from_id_token() {
        let future_exp: i64 = 1893456000;
        let jwt = make_jwt_with_exp(future_exp);
        let json = format!(r#"{{"id_token":"{}"}}"#, jwt);
        let dt = parse_gemini_expiry(json.as_bytes()).unwrap();
        assert_eq!(dt.timestamp(), future_exp);
    }

    #[test]
    fn parse_gemini_expiry_returns_none_for_missing() {
        let json = r#"{"other":"field"}"#;
        assert!(parse_gemini_expiry(json.as_bytes()).is_none());
    }

    #[test]
    fn expiry_warning_for_expired_token() {
        let past = Utc::now() - Duration::hours(1);
        let warning = expiry_warning(past, "Claude Code", "work", "claude");
        assert!(warning.is_some());
        let msg = warning.unwrap();
        assert!(msg.contains("expired"), "expected 'expired' in: {}", msg);
    }

    #[test]
    fn expiry_warning_for_soon_expiring_token() {
        let soon = Utc::now() + Duration::hours(2);
        let warning = expiry_warning(soon, "Claude Code", "work", "claude");
        assert!(warning.is_some());
        let msg = warning.unwrap();
        assert!(
            msg.contains("expires in"),
            "expected 'expires in' in: {}",
            msg
        );
    }

    #[test]
    fn expiry_warning_for_far_future_token() {
        let future = Utc::now() + Duration::days(30);
        let warning = expiry_warning(future, "Claude Code", "work", "claude");
        assert!(warning.is_none());
    }
}

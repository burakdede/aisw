//! `aisw usage` against loopback vendor endpoints.
//!
//! No test touches the real network: every request goes to a local server via
//! the debug-only `AISW_CLAUDE_USAGE_URL` / `AISW_CODEX_USAGE_URL` overrides,
//! and the expired-token test proves expiry is caught before any request.

mod common;

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use common::TestEnv;
use predicates::str::contains;

const CLAUDE_RESPONSE: &str = r#"{
    "five_hour": {"utilization": 14, "resets_at": "2026-09-18T04:10:00+00:00"},
    "seven_day": {"utilization": 25, "resets_at": "2026-09-20T14:00:00+00:00"},
    "limits": [
        {"scope": {"model": {"display_name": "Fable"}}, "percent": 30, "resets_at": "2026-09-20T14:00:00+00:00"}
    ]
}"#;

const CODEX_RESPONSE: &str = r#"{
    "email": "user@example.com",
    "plan_type": "pro",
    "rate_limit": {
        "primary_window": {"used_percent": 81, "limit_window_seconds": 604800, "reset_at": 1789842473},
        "secondary_window": null
    },
    "additional_rate_limits": [
        {"limit_name": "gpt-reserve", "rate_limit": {"primary_window": {"used_percent": 0, "limit_window_seconds": 604800, "reset_at": 1790300603}}}
    ]
}"#;

/// Unix milliseconds / seconds far in the future and in the past.
const FUTURE_MS: i64 = 4_102_444_800_000;
const FUTURE_SECS: i64 = 4_102_444_800;
const PAST_MS: i64 = 1_600_000_000_000;
const PAST_SECS: i64 = 1_600_000_000;

/// A loopback HTTP server that answers every request with one canned
/// response and records the request heads it received.
struct FakeEndpoint {
    url: String,
    requests: Arc<Mutex<Vec<String>>>,
}

impl FakeEndpoint {
    fn serve(status: u16, body: &'static str) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/usage", listener.local_addr().unwrap());
        let requests = Arc::new(Mutex::new(Vec::new()));
        let seen = Arc::clone(&requests);
        thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else {
                    break;
                };
                seen.lock().unwrap().push(read_request_head(&mut stream));
                let response = format!(
                    "HTTP/1.1 {status} OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = stream.write_all(response.as_bytes());
            }
        });
        Self { url, requests }
    }

    fn requests(&self) -> Vec<String> {
        self.requests.lock().unwrap().clone()
    }
}

fn read_request_head(stream: &mut TcpStream) -> String {
    let mut raw = Vec::new();
    let mut chunk = [0u8; 1024];
    while !raw.windows(4).any(|window| window == b"\r\n\r\n") {
        match stream.read(&mut chunk) {
            Ok(0) | Err(_) => break,
            Ok(n) => raw.extend_from_slice(&chunk[..n]),
        }
    }
    String::from_utf8_lossy(&raw).to_ascii_lowercase()
}

fn write_secret(path: &std::path::Path, contents: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, contents).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).unwrap();
    }
}

/// Register profiles in config.json: (tool, name, auth_method, active).
fn write_profiles(env: &TestEnv, profiles: &[(&str, &str, &str, bool)]) {
    let mut config = serde_json::json!({
        "version": 2,
        "active": {"claude": null, "codex": null, "gemini": null, "antigravity": null},
        "profiles": {"claude": {}, "codex": {}, "gemini": {}, "antigravity": {}},
        "settings": {
            "backup_on_switch": true,
            "max_backups": 10,
            "claude": {"state_mode": "isolated"},
            "codex": {"state_mode": "isolated"}
        }
    });
    for (tool, name, auth_method, active) in profiles {
        config["profiles"][*tool][*name] = serde_json::json!({
            "added_at": "2026-07-16T00:00:00Z",
            "auth_method": auth_method,
            "credential_backend": "file",
            "label": null
        });
        if *active {
            config["active"][*tool] = serde_json::json!(name);
        }
    }
    std::fs::write(
        env.aisw_home.join("config.json"),
        serde_json::to_string_pretty(&config).unwrap(),
    )
    .unwrap();
}

fn seed_claude_oauth(env: &TestEnv, name: &str, expires_at_ms: i64) {
    let dir = env.aisw_home.join("profiles").join("claude").join(name);
    write_secret(
        &dir.join(".credentials.json"),
        &format!(
            r#"{{"claudeAiOauth":{{"accessToken":"claude-token-{name}","refreshToken":"refresh","expiresAt":{expires_at_ms}}}}}"#
        ),
    );
    write_secret(
        &dir.join("oauth-account.json"),
        r#"{"emailAddress":"me@example.com","organizationName":"Example Org"}"#,
    );
}

fn chatgpt_jwt(exp: i64) -> String {
    let payload = format!(
        r#"{{"exp":{exp},"https://api.openai.com/auth":{{"chatgpt_account_id":"acc-123"}}}}"#
    );
    format!(
        "eyJhbGciOiJIUzI1NiJ9.{}.sig",
        URL_SAFE_NO_PAD.encode(payload.as_bytes())
    )
}

fn seed_codex_chatgpt(env: &TestEnv, name: &str, exp: i64) {
    let dir = env.aisw_home.join("profiles").join("codex").join(name);
    write_secret(
        &dir.join("auth.json"),
        &format!(
            r#"{{"auth_mode":"chatgpt","tokens":{{"access_token":"{}","account_id":"acc-123","refresh_token":"refresh","id_token":"id"}},"last_refresh":"2026-07-16T00:00:00Z"}}"#,
            chatgpt_jwt(exp)
        ),
    );
    write_secret(
        &dir.join("config.toml"),
        "cli_auth_credentials_store = \"file\"\n",
    );
}

fn run(env: &TestEnv, args: &[&str], endpoints: &[(&str, &FakeEndpoint)]) -> std::process::Output {
    let mut cmd = env.cmd();
    cmd.args(args);
    for (var, endpoint) in endpoints {
        cmd.env(var, &endpoint.url);
    }
    cmd.output().unwrap()
}

fn stdout_of(output: &std::process::Output) -> String {
    assert!(
        output.status.success(),
        "command failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

#[test]
fn claude_oauth_profile_reports_vendor_windows() {
    let env = TestEnv::new();
    seed_claude_oauth(&env, "work", FUTURE_MS);
    write_profiles(&env, &[("claude", "work", "o_auth", true)]);
    let endpoint = FakeEndpoint::serve(200, CLAUDE_RESPONSE);

    let output = run(
        &env,
        &["usage", "claude"],
        &[("AISW_CLAUDE_USAGE_URL", &endpoint)],
    );
    let stdout = stdout_of(&output);

    for expected in [
        "Claude Code",
        "work",
        "me@example.com (Example Org)",
        "5h",
        "14%",
        "7d",
        "25%",
        "Fable",
        "30%",
    ] {
        assert!(
            stdout.contains(expected),
            "missing {expected:?} in:\n{stdout}"
        );
    }
    let requests = endpoint.requests();
    assert_eq!(
        requests.len(),
        1,
        "expected one usage request, saw {requests:?}"
    );
    assert!(requests[0].contains("authorization: bearer claude-token-work"));
    assert!(requests[0].contains("anthropic-beta: oauth-2025-04-20"));
}

#[test]
fn codex_chatgpt_profile_sends_account_header_and_reports_windows() {
    let env = TestEnv::new();
    seed_codex_chatgpt(&env, "work", FUTURE_SECS);
    write_profiles(&env, &[("codex", "work", "o_auth", true)]);
    let endpoint = FakeEndpoint::serve(200, CODEX_RESPONSE);

    let output = run(
        &env,
        &["usage", "codex"],
        &[("AISW_CODEX_USAGE_URL", &endpoint)],
    );
    let stdout = stdout_of(&output);

    for expected in [
        "Codex CLI",
        "user@example.com (pro)",
        "7d",
        "81%",
        "gpt-reserve",
        "0%",
    ] {
        assert!(
            stdout.contains(expected),
            "missing {expected:?} in:\n{stdout}"
        );
    }
    let requests = endpoint.requests();
    assert_eq!(requests.len(), 1);
    assert!(requests[0].contains("authorization: bearer eyj"));
    assert!(requests[0].contains("chatgpt-account-id: acc-123"));
}

#[test]
fn json_output_reports_windows_and_per_profile_errors() {
    let env = TestEnv::new();
    env.add_fake_tool("codex", "codex 1.0.0");
    seed_claude_oauth(&env, "work", FUTURE_MS);
    write_profiles(&env, &[("claude", "work", "o_auth", true)]);
    env.cmd()
        .args([
            "add",
            "codex",
            "keyed",
            "--api-key",
            "sk-codex-test-key-12345",
        ])
        .assert()
        .success();
    let endpoint = FakeEndpoint::serve(200, CLAUDE_RESPONSE);

    let output = run(
        &env,
        &["usage", "--json"],
        &[("AISW_CLAUDE_USAGE_URL", &endpoint)],
    );
    assert!(
        output.stderr.is_empty(),
        "stderr should be empty in --json mode"
    );
    let json: serde_json::Value = serde_json::from_str(&stdout_of(&output)).unwrap();

    assert_eq!(json["claude"]["active"], "work");
    let claude = &json["claude"]["profiles"][0];
    assert_eq!(claude["name"], "work");
    assert_eq!(claude["active"], true);
    assert_eq!(claude["account"], "me@example.com");
    assert_eq!(claude["plan"], "Example Org");
    assert!(claude["error"].is_null());
    assert_eq!(claude["windows"][0]["name"], "5h");
    assert_eq!(claude["windows"][0]["used_percent"], 14.0);
    assert_eq!(claude["windows"][0]["resets_at"], "2026-09-18T04:10:00Z");
    assert_eq!(claude["windows"][2]["name"], "Fable");

    let codex = &json["codex"]["profiles"][0];
    assert_eq!(codex["name"], "keyed");
    assert_eq!(codex["windows"], serde_json::json!([]));
    assert!(codex["error"].as_str().unwrap().contains("API-key"));
}

#[test]
fn expired_tokens_are_reported_without_any_request() {
    let env = TestEnv::new();
    seed_claude_oauth(&env, "stale", PAST_MS);
    seed_codex_chatgpt(&env, "stale", PAST_SECS);
    write_profiles(
        &env,
        &[
            ("claude", "stale", "o_auth", true),
            ("codex", "stale", "o_auth", true),
        ],
    );
    let endpoint = FakeEndpoint::serve(200, CLAUDE_RESPONSE);

    let output = run(
        &env,
        &["usage"],
        &[
            ("AISW_CLAUDE_USAGE_URL", &endpoint),
            ("AISW_CODEX_USAGE_URL", &endpoint),
        ],
    );
    let stdout = stdout_of(&output);

    assert!(stdout.contains("OAuth token expired"), "{stdout}");
    assert!(stdout.contains("ChatGPT token expired"), "{stdout}");
    assert!(
        endpoint.requests().is_empty(),
        "expired tokens must not be sent"
    );
}

#[test]
fn rejected_token_is_reported_per_profile_and_exits_zero() {
    let env = TestEnv::new();
    seed_claude_oauth(&env, "work", FUTURE_MS);
    write_profiles(&env, &[("claude", "work", "o_auth", true)]);
    let endpoint = FakeEndpoint::serve(401, r#"{"error":"unauthorized"}"#);

    let output = run(
        &env,
        &["usage", "claude"],
        &[("AISW_CLAUDE_USAGE_URL", &endpoint)],
    );
    let stdout = stdout_of(&output);

    assert!(stdout.contains("HTTP 401"), "{stdout}");
    assert!(stdout.contains("sign in again"), "{stdout}");
}

#[test]
fn active_only_queries_just_the_active_profile() {
    let env = TestEnv::new();
    seed_claude_oauth(&env, "work", FUTURE_MS);
    seed_claude_oauth(&env, "personal", FUTURE_MS);
    write_profiles(
        &env,
        &[
            ("claude", "work", "o_auth", true),
            ("claude", "personal", "o_auth", false),
        ],
    );
    let endpoint = FakeEndpoint::serve(200, CLAUDE_RESPONSE);

    let output = run(
        &env,
        &["usage", "claude", "--active-only"],
        &[("AISW_CLAUDE_USAGE_URL", &endpoint)],
    );
    let stdout = stdout_of(&output);

    assert!(stdout.contains("work"), "{stdout}");
    assert!(!stdout.contains("personal"), "{stdout}");
    assert_eq!(endpoint.requests().len(), 1);
}

#[test]
fn unsupported_tool_is_an_error() {
    TestEnv::new()
        .cmd()
        .args(["usage", "gemini"])
        .assert()
        .failure()
        .stderr(contains("does not expose a usage endpoint"));
}

#[test]
fn unknown_profile_exits_with_profile_not_found() {
    let env = TestEnv::new();
    write_profiles(&env, &[("claude", "work", "o_auth", true)]);

    env.cmd().args(["usage", "claude", "nope"]).assert().code(2);

    let output = env.output(&["usage", "claude", "nope", "--json"]);
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["ok"], false);
    assert_eq!(json["error"]["kind"], "profile_not_found");
}

#[test]
fn no_profiles_prints_an_empty_state() {
    let env = TestEnv::new();
    let stdout = stdout_of(&env.output(&["usage"]));
    assert!(stdout.contains("No profiles found"), "{stdout}");
}

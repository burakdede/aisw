use std::env;

use anyhow::Result;

use crate::auth;
use crate::config::AuthMethod;
use crate::profile::ProfileStore;
use crate::types::Tool;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionActivation {
    Effective,
    CurrentShellNotUsingProfile,
    NotApplicable,
}

pub fn assess_current_session(
    tool: Tool,
    auth_method: AuthMethod,
    profile_store: &ProfileStore,
    profile_name: &str,
) -> Result<SessionActivation> {
    match tool {
        Tool::Claude => assess_shell_env(
            auth_method,
            "CLAUDE_CONFIG_DIR",
            || {
                profile_store
                    .profile_dir(Tool::Claude, profile_name)
                    .display()
                    .to_string()
            },
            || auth::claude::read_api_key(profile_store, profile_name),
        ),
        Tool::Codex => assess_shell_env(
            auth_method,
            "CODEX_HOME",
            || {
                profile_store
                    .profile_dir(Tool::Codex, profile_name)
                    .display()
                    .to_string()
            },
            || auth::codex::read_api_key(profile_store, profile_name),
        ),
        Tool::Gemini => Ok(SessionActivation::NotApplicable),
    }
}

fn assess_shell_env<OAuth, ApiKey>(
    auth_method: AuthMethod,
    oauth_var: &str,
    oauth_expected: OAuth,
    api_key_expected: ApiKey,
) -> Result<SessionActivation>
where
    OAuth: FnOnce() -> String,
    ApiKey: FnOnce() -> Result<String>,
{
    let (var_name, expected_value) = match auth_method {
        AuthMethod::OAuth => (oauth_var, oauth_expected()),
        AuthMethod::ApiKey => (api_key_env_var(oauth_var), api_key_expected()?),
    };

    let current = env::var(var_name).ok();
    if current.as_deref() == Some(expected_value.as_str()) {
        Ok(SessionActivation::Effective)
    } else {
        Ok(SessionActivation::CurrentShellNotUsingProfile)
    }
}

fn api_key_env_var(oauth_var: &str) -> &'static str {
    match oauth_var {
        "CLAUDE_CONFIG_DIR" => "ANTHROPIC_API_KEY",
        "CODEX_HOME" => "OPENAI_API_KEY",
        _ => unreachable!(),
    }
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;
    use crate::config::ConfigStore;

    #[test]
    fn gemini_session_activation_not_applicable() {
        let tmp = tempdir().unwrap();
        let profile_store = ProfileStore::new(tmp.path());

        let status =
            assess_current_session(Tool::Gemini, AuthMethod::ApiKey, &profile_store, "work")
                .unwrap();

        assert_eq!(status, SessionActivation::NotApplicable);
    }

    #[test]
    fn claude_api_key_effective_when_env_matches() {
        let tmp = tempdir().unwrap();
        let profile_store = ProfileStore::new(tmp.path());
        let config_store = ConfigStore::new(tmp.path());
        auth::claude::add_api_key(
            &profile_store,
            &config_store,
            "work",
            "sk-ant-api03-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
            None,
        )
        .unwrap();

        unsafe {
            env::set_var(
                "ANTHROPIC_API_KEY",
                "sk-ant-api03-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
            );
        }
        let status =
            assess_current_session(Tool::Claude, AuthMethod::ApiKey, &profile_store, "work")
                .unwrap();
        unsafe {
            env::remove_var("ANTHROPIC_API_KEY");
        }

        assert_eq!(status, SessionActivation::Effective);
    }
}

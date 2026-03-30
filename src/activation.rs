use anyhow::Result;

use crate::auth;
use crate::config::AuthMethod;
use crate::profile::ProfileStore;
use crate::types::Tool;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiveActivation {
    Applied,
    NotApplied,
}

pub fn assess_live_state(
    tool: Tool,
    auth_method: AuthMethod,
    profile_store: &ProfileStore,
    profile_name: &str,
    user_home: &std::path::Path,
) -> Result<LiveActivation> {
    let applied = match tool {
        Tool::Claude => {
            auth::claude::live_credentials_match(profile_store, profile_name, user_home)?
        }
        Tool::Codex => auth::codex::live_files_match(profile_store, profile_name, user_home)?,
        Tool::Gemini => match auth_method {
            AuthMethod::ApiKey => auth::gemini::live_env_matches(
                profile_store,
                profile_name,
                &user_home.join(".gemini").join(".env"),
            )?,
            AuthMethod::OAuth => auth::gemini::live_token_cache_matches(
                profile_store,
                profile_name,
                &user_home.join(".gemini"),
            )?,
        },
    };

    if applied {
        Ok(LiveActivation::Applied)
    } else {
        Ok(LiveActivation::NotApplied)
    }
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;
    use crate::config::ConfigStore;

    const CLAUDE_KEY: &str = "sk-ant-api03-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
    const CODEX_KEY: &str = "sk-codex-test-key-12345";
    const GEMINI_KEY: &str = "AIzatest1234567890ABCDEF";

    #[test]
    fn gemini_api_key_live_state_is_not_applied_without_live_env() {
        let tmp = tempdir().unwrap();
        let profile_store = ProfileStore::new(tmp.path());
        let config_store = ConfigStore::new(tmp.path());
        auth::gemini::add_api_key(&profile_store, &config_store, "work", GEMINI_KEY, None).unwrap();

        let status = assess_live_state(
            Tool::Gemini,
            AuthMethod::ApiKey,
            &profile_store,
            "work",
            tmp.path(),
        )
        .unwrap();

        assert_eq!(status, LiveActivation::NotApplied);
    }

    #[test]
    fn claude_live_state_is_applied_when_live_credentials_match() {
        let tmp = tempdir().unwrap();
        let profile_store = ProfileStore::new(tmp.path());
        let config_store = ConfigStore::new(tmp.path());
        auth::claude::add_api_key(&profile_store, &config_store, "work", CLAUDE_KEY, None).unwrap();
        auth::claude::apply_live_credentials(&profile_store, "work", tmp.path()).unwrap();

        let status = assess_live_state(
            Tool::Claude,
            AuthMethod::ApiKey,
            &profile_store,
            "work",
            tmp.path(),
        )
        .unwrap();

        assert_eq!(status, LiveActivation::Applied);
    }

    #[test]
    fn claude_live_state_is_not_applied_when_live_credentials_are_missing() {
        let tmp = tempdir().unwrap();
        let profile_store = ProfileStore::new(tmp.path());
        let config_store = ConfigStore::new(tmp.path());
        auth::claude::add_api_key(&profile_store, &config_store, "work", CLAUDE_KEY, None).unwrap();

        let status = assess_live_state(
            Tool::Claude,
            AuthMethod::ApiKey,
            &profile_store,
            "work",
            tmp.path(),
        )
        .unwrap();

        assert_eq!(status, LiveActivation::NotApplied);
    }

    #[test]
    fn codex_live_state_is_applied_when_live_files_match() {
        let tmp = tempdir().unwrap();
        let profile_store = ProfileStore::new(tmp.path());
        let config_store = ConfigStore::new(tmp.path());
        auth::codex::add_api_key(&profile_store, &config_store, "work", CODEX_KEY, None).unwrap();
        auth::codex::apply_live_files(&profile_store, "work", tmp.path()).unwrap();

        let status = assess_live_state(
            Tool::Codex,
            AuthMethod::ApiKey,
            &profile_store,
            "work",
            tmp.path(),
        )
        .unwrap();

        assert_eq!(status, LiveActivation::Applied);
    }

    #[test]
    fn codex_live_state_is_not_applied_when_live_files_are_missing() {
        let tmp = tempdir().unwrap();
        let profile_store = ProfileStore::new(tmp.path());
        let config_store = ConfigStore::new(tmp.path());
        auth::codex::add_api_key(&profile_store, &config_store, "work", CODEX_KEY, None).unwrap();

        let status = assess_live_state(
            Tool::Codex,
            AuthMethod::ApiKey,
            &profile_store,
            "work",
            tmp.path(),
        )
        .unwrap();

        assert_eq!(status, LiveActivation::NotApplied);
    }

    #[test]
    fn gemini_oauth_live_state_is_applied_when_cache_matches() {
        let tmp = tempdir().unwrap();
        let profile_store = ProfileStore::new(tmp.path());
        profile_store.create(Tool::Gemini, "work").unwrap();
        profile_store
            .write_file(
                Tool::Gemini,
                "work",
                "oauth_creds.json",
                br#"{"token":"tok"}"#,
            )
            .unwrap();
        profile_store
            .write_file(
                Tool::Gemini,
                "work",
                "settings.json",
                br#"{"account":"work"}"#,
            )
            .unwrap();
        auth::gemini::apply_token_cache(&profile_store, "work", &tmp.path().join(".gemini"))
            .unwrap();

        let status = assess_live_state(
            Tool::Gemini,
            AuthMethod::OAuth,
            &profile_store,
            "work",
            tmp.path(),
        )
        .unwrap();

        assert_eq!(status, LiveActivation::Applied);
    }

    #[test]
    fn gemini_oauth_live_state_is_not_applied_when_cache_is_missing() {
        let tmp = tempdir().unwrap();
        let profile_store = ProfileStore::new(tmp.path());
        profile_store.create(Tool::Gemini, "work").unwrap();
        profile_store
            .write_file(
                Tool::Gemini,
                "work",
                "oauth_creds.json",
                br#"{"token":"tok"}"#,
            )
            .unwrap();

        let status = assess_live_state(
            Tool::Gemini,
            AuthMethod::OAuth,
            &profile_store,
            "work",
            tmp.path(),
        )
        .unwrap();

        assert_eq!(status, LiveActivation::NotApplied);
    }
}

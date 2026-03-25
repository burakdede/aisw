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

    #[test]
    fn gemini_api_key_live_state_is_not_applied_without_live_env() {
        let tmp = tempdir().unwrap();
        let profile_store = ProfileStore::new(tmp.path());
        let config_store = ConfigStore::new(tmp.path());
        auth::gemini::add_api_key(
            &profile_store,
            &config_store,
            "work",
            "AIzatest1234567890ABCDEF",
            None,
        )
        .unwrap();

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
}

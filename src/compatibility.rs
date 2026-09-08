use crate::types::Tool;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Verified,
    OlderThanVerified,
    NewerThanVerified,
    Unknown,
}

impl Status {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Verified => "verified",
            Self::OlderThanVerified => "older_than_verified",
            Self::NewerThanVerified => "newer_than_verified",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Version(u64, u64, u64);

pub fn baseline(tool: Tool) -> &'static str {
    match tool {
        Tool::Claude => "v2.1.263",
        Tool::Codex => "rust-v0.153.4",
        Tool::Gemini => "v0.58.0",
        Tool::Antigravity => "1.1.27",
    }
}

pub fn assess(tool: Tool, detected: Option<&str>) -> Status {
    let Some(detected) = detected.and_then(parse_version) else {
        return Status::Unknown;
    };
    let expected = parse_version(baseline(tool)).expect("compatibility baselines are valid");
    match detected.cmp(&expected) {
        std::cmp::Ordering::Less => Status::OlderThanVerified,
        std::cmp::Ordering::Equal => Status::Verified,
        std::cmp::Ordering::Greater => Status::NewerThanVerified,
    }
}

fn parse_version(raw: &str) -> Option<Version> {
    raw.split(|ch: char| !ch.is_ascii_alphanumeric() && ch != '.')
        .filter_map(|part| part.strip_prefix('v').or(Some(part)))
        .find_map(|part| {
            let mut components = part.split('.');
            let version = Version(
                components.next()?.parse().ok()?,
                components.next()?.parse().ok()?,
                components.next()?.parse().ok()?,
            );
            components.next().is_none().then_some(version)
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_each_audited_release() {
        for (tool, version) in [
            (Tool::Claude, "claude 2.1.263"),
            (Tool::Codex, "codex-cli 0.153.4"),
            (Tool::Gemini, "gemini 0.58.0"),
            (Tool::Antigravity, "agy 1.1.27"),
        ] {
            assert_eq!(assess(tool, Some(version)), Status::Verified);
        }
    }

    #[test]
    fn classifies_versions_relative_to_the_audit() {
        assert_eq!(
            assess(Tool::Claude, Some("claude 2.1.262")),
            Status::OlderThanVerified
        );
        assert_eq!(
            assess(Tool::Codex, Some("codex-cli 0.153.5")),
            Status::NewerThanVerified
        );
    }

    #[test]
    fn treats_missing_and_unparseable_versions_as_unknown() {
        assert_eq!(assess(Tool::Gemini, None), Status::Unknown);
        assert_eq!(
            assess(Tool::Gemini, Some("gemini development")),
            Status::Unknown
        );
    }
}

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
        Tool::Claude => "v2.1.266",
        Tool::Codex => "rust-v0.153.4",
        Tool::Gemini => "v0.59.0",
        Tool::Antigravity => "1.1.28",
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
    for (start, ch) in raw.char_indices() {
        let version_start = if ch == 'v'
            && raw
                .get(start + ch.len_utf8()..)
                .and_then(|rest| rest.chars().next())
                .is_some_and(|next| next.is_ascii_digit())
        {
            start + ch.len_utf8()
        } else if ch.is_ascii_digit() {
            start
        } else {
            continue;
        };

        let end = raw[version_start..]
            .char_indices()
            .find_map(|(offset, ch)| {
                (!ch.is_ascii_digit() && ch != '.').then_some(version_start + offset)
            })
            .unwrap_or(raw.len());
        let suffix = &raw[end..];
        if suffix.starts_with('-') || suffix.starts_with('+') {
            continue;
        }
        if suffix
            .chars()
            .next()
            .is_some_and(|next| next.is_ascii_alphanumeric() || next == '.')
        {
            continue;
        }

        let mut components = raw[version_start..end].split('.');
        let (Some(major), Some(minor), Some(patch)) =
            (components.next(), components.next(), components.next())
        else {
            continue;
        };
        if components.next().is_some() {
            continue;
        }
        let (Ok(major), Ok(minor), Ok(patch)) = (
            major.parse::<u64>(),
            minor.parse::<u64>(),
            patch.parse::<u64>(),
        ) else {
            continue;
        };
        return Some(Version(major, minor, patch));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_each_audited_release() {
        for (tool, version) in [
            (Tool::Claude, "claude 2.1.266"),
            (Tool::Codex, "codex-cli 0.153.4"),
            (Tool::Gemini, "gemini 0.59.0"),
            (Tool::Antigravity, "agy 1.1.28"),
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

    #[test]
    fn treats_prerelease_and_build_metadata_as_unknown() {
        assert_eq!(
            assess(Tool::Codex, Some("codex-cli 0.153.4-beta.1")),
            Status::Unknown
        );
        assert_eq!(
            assess(Tool::Codex, Some("codex-cli 0.153.4+build.1")),
            Status::Unknown
        );
    }
}

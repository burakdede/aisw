use std::path::PathBuf;

fn read_repo_file(path: &str) -> String {
    std::fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(path))
        .unwrap_or_else(|error| panic!("could not read {path}: {error}"))
}

#[test]
fn automation_docs_publish_the_machine_interface_contract() {
    let docs = read_repo_file("docs/automation.md");
    let website = read_repo_file("website/src/content/docs/automation.md");

    for content in [&docs, &website] {
        for expected in [
            "### Machine interface versions",
            "`cli_api_version`",
            "`json_schema_version`",
            "`progress_schema_version`",
            "The current value for each is `1`.",
            "ignore unknown fields",
            "Treat an unknown version as unsupported",
            "top-level `command` identifies the parsed operation",
            "Backup ids sort lexicographically, newest first.",
            "sort_by(.backup_id) | first | .backup_id",
        ] {
            assert!(content.contains(expected), "missing {expected}");
        }
    }
}

#[test]
fn profile_onboarding_lists_every_supported_tool() {
    for path in [
        "docs/adding-profiles.md",
        "website/src/content/docs/adding-profiles.md",
    ] {
        let content = read_repo_file(path);
        assert!(
            content.contains("`<tool>` is one of: `claude`, `codex`, `gemini`, `antigravity`."),
            "profile onboarding is missing a supported tool: {path}"
        );
    }
}

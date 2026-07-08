use anyhow::Result;
use serde::Serialize;

use crate::cli::CapabilitiesArgs;
use crate::types::Tool;

#[derive(Serialize)]
struct CapabilitiesPayload {
    version: &'static str,
    cli_api_version: u32,
    json_schema_version: u32,
    progress_schema_version: u32,
    features: FeatureFlags,
    tools: ToolCapabilitiesSet,
}

#[derive(Serialize)]
struct FeatureFlags {
    api_key_stdin: bool,
    mutation_json: bool,
    progress_json: bool,
    non_prompting_init: bool,
    detect_live_init: bool,
    verify: bool,
    repair: bool,
    contexts: bool,
    workspace_bindings: bool,
    project_bindings_alias: bool,
}

#[derive(Serialize)]
struct ToolCapabilitiesSet {
    claude: ToolCapabilities,
    codex: ToolCapabilities,
    gemini: ToolCapabilities,
}

#[derive(Serialize)]
struct ToolCapabilities {
    auth_methods: Vec<&'static str>,
    state_modes: Vec<&'static str>,
    credential_backends: Vec<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    fail_closed_keyring_identity: Option<bool>,
}

pub fn run(args: CapabilitiesArgs) -> Result<()> {
    let payload = CapabilitiesPayload {
        version: env!("CARGO_PKG_VERSION"),
        cli_api_version: 1,
        json_schema_version: 1,
        progress_schema_version: 1,
        features: FeatureFlags {
            api_key_stdin: true,
            mutation_json: true,
            progress_json: true,
            non_prompting_init: true,
            detect_live_init: true,
            verify: true,
            repair: false,
            contexts: true,
            workspace_bindings: true,
            project_bindings_alias: false,
        },
        tools: ToolCapabilitiesSet {
            claude: tool_capabilities(Tool::Claude),
            codex: tool_capabilities(Tool::Codex),
            gemini: tool_capabilities(Tool::Gemini),
        },
    };

    if args.json {
        println!("{}", serde_json::to_string_pretty(&payload)?);
    } else {
        println!("machine features");
        println!("  api_key_stdin: true");
        println!("  mutation_json: true");
        println!("  progress_json: true");
    }

    Ok(())
}

fn tool_capabilities(tool: Tool) -> ToolCapabilities {
    match tool {
        Tool::Claude => ToolCapabilities {
            auth_methods: vec!["oauth", "api_key", "from_env", "from_live"],
            state_modes: vec!["isolated", "shared"],
            credential_backends: vec!["file", "system_keyring"],
            fail_closed_keyring_identity: None,
        },
        Tool::Codex => ToolCapabilities {
            auth_methods: vec!["oauth", "api_key", "from_env", "from_live"],
            state_modes: vec!["isolated", "shared"],
            credential_backends: vec!["file", "system_keyring"],
            fail_closed_keyring_identity: Some(true),
        },
        Tool::Gemini => ToolCapabilities {
            auth_methods: vec!["oauth", "api_key", "from_env", "from_live"],
            state_modes: vec!["isolated"],
            credential_backends: vec!["file"],
            fail_closed_keyring_identity: None,
        },
    }
}

use anyhow::Result;
use serde::Serialize;

use crate::cli::VersionArgs;

#[derive(Serialize)]
struct VersionPayload {
    version: &'static str,
    cli_api_version: u32,
    json_schema_version: u32,
    progress_schema_version: u32,
}

pub fn run(args: VersionArgs) -> Result<()> {
    let payload = VersionPayload {
        version: env!("CARGO_PKG_VERSION"),
        cli_api_version: 1,
        json_schema_version: 1,
        progress_schema_version: 1,
    };

    if args.json {
        println!("{}", serde_json::to_string_pretty(&payload)?);
    } else {
        println!("{}", payload.version);
    }

    Ok(())
}

use anyhow::Error;
use serde::Serialize;

use crate::error::AiswError;

#[derive(Debug, Clone, Serialize)]
pub struct MachineRemediation {
    pub kind: &'static str,
    pub command: String,
    pub safe: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct MachineError {
    pub kind: String,
    pub message: String,
    pub exit_code: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remediation: Option<MachineRemediation>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MachineEnvelope<T: Serialize> {
    pub ok: bool,
    pub command: &'static str,
    pub result: T,
}

#[derive(Debug, Clone, Serialize)]
pub struct MachineFailureEnvelope {
    pub ok: bool,
    pub command: String,
    pub error: MachineError,
}

pub fn print_success<T: Serialize>(command: &'static str, result: T) -> anyhow::Result<()> {
    println!(
        "{}",
        serde_json::to_string_pretty(&MachineEnvelope {
            ok: true,
            command,
            result,
        })?
    );
    Ok(())
}

pub fn print_failure(command: Option<&str>, err: &Error, exit_code: i32) {
    let failure = MachineFailureEnvelope {
        ok: false,
        command: command.unwrap_or("unknown").to_owned(),
        error: machine_error(err, exit_code),
    };

    if let Ok(json) = serde_json::to_string_pretty(&failure) {
        println!("{json}");
    } else {
        println!(
            "{{\"ok\":false,\"command\":\"{}\",\"error\":{{\"kind\":\"internal_error\",\"message\":\"failed to serialize machine error\",\"exit_code\":{}}}}}",
            command.unwrap_or("unknown"),
            exit_code
        );
    }
}

pub fn parse_error(command: Option<&str>, err: &clap::Error) -> MachineFailureEnvelope {
    let kind = match err.kind() {
        clap::error::ErrorKind::UnknownArgument => "unsupported_flag",
        clap::error::ErrorKind::InvalidSubcommand => "unsupported_command",
        _ => "validation_error",
    };

    MachineFailureEnvelope {
        ok: false,
        command: command.unwrap_or("unknown").to_owned(),
        error: MachineError {
            kind: kind.to_owned(),
            message: err.to_string().trim().to_owned(),
            exit_code: err.exit_code(),
            remediation: None,
        },
    }
}

fn machine_error(err: &Error, exit_code: i32) -> MachineError {
    if let Some(typed) = err.downcast_ref::<AiswError>() {
        return MachineError {
            kind: typed.code().to_owned(),
            message: typed.to_string(),
            exit_code,
            remediation: typed.remediation(),
        };
    }

    MachineError {
        kind: "validation_error".to_owned(),
        message: err.to_string(),
        exit_code,
        remediation: None,
    }
}

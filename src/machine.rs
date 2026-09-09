use anyhow::Error;
use serde::Serialize;

use crate::runtime;

use crate::error::AiswError;
use crate::output::redact_sensitive_text;

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

#[derive(Debug, Clone, Serialize)]
pub struct ProgressEvent<T: Serialize> {
    #[serde(rename = "type")]
    pub event_type: &'static str,
    pub seq: u64,
    pub command: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phase: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub safe_to_cancel: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ok: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<T>,
}

#[derive(Debug, Clone)]
pub struct ProgressReporter {
    command: &'static str,
    tool: Option<&'static str>,
    profile: Option<String>,
    seq: u64,
}

impl ProgressReporter {
    pub fn new(
        command: &'static str,
        tool: Option<&'static str>,
        profile: Option<String>,
    ) -> Option<Self> {
        runtime::is_progress_json().then_some(Self {
            command,
            tool,
            profile,
            seq: 0,
        })
    }

    pub fn started(&mut self) -> anyhow::Result<()> {
        self.emit::<serde_json::Value>("started", None, None, None, None, None)
    }

    pub fn info(&mut self, phase: &'static str, message: impl Into<String>) -> anyhow::Result<()> {
        self.emit::<serde_json::Value>("info", Some(phase), None, Some(message.into()), None, None)
    }

    pub fn waiting_for_user(
        &mut self,
        phase: &'static str,
        message: impl Into<String>,
        safe_to_cancel: bool,
    ) -> anyhow::Result<()> {
        self.emit::<serde_json::Value>(
            "waiting_for_user",
            Some(phase),
            Some(safe_to_cancel),
            Some(message.into()),
            None,
            None,
        )
    }

    pub fn result<T: Serialize>(&mut self, ok: bool, result: T) -> anyhow::Result<()> {
        self.emit("result", None, None, None, Some(ok), Some(result))
    }

    fn emit<T: Serialize>(
        &mut self,
        event_type: &'static str,
        phase: Option<&'static str>,
        safe_to_cancel: Option<bool>,
        message: Option<String>,
        ok: Option<bool>,
        result: Option<T>,
    ) -> anyhow::Result<()> {
        self.seq += 1;
        println!(
            "{}",
            serialize_json(&ProgressEvent {
                event_type,
                seq: self.seq,
                command: self.command,
                tool: self.tool,
                profile: self.profile.clone(),
                phase,
                safe_to_cancel,
                message,
                ok,
                result,
            })?
        );
        Ok(())
    }
}

pub fn print_success<T: Serialize>(command: &'static str, result: T) -> anyhow::Result<()> {
    println!(
        "{}",
        serialize_json(&MachineEnvelope {
            ok: true,
            command,
            result
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

    if let Ok(json) = serialize_json(&failure) {
        println!("{json}");
    } else {
        println!(
            "{{\"ok\":false,\"command\":\"{}\",\"error\":{{\"kind\":\"internal_error\",\"message\":\"failed to serialize machine error\",\"exit_code\":{}}}}}",
            command.unwrap_or("unknown"),
            exit_code
        );
    }
}

pub fn serialize_json<T: Serialize>(value: &T) -> anyhow::Result<String> {
    if runtime::is_progress_json() {
        Ok(serde_json::to_string(value)?)
    } else {
        Ok(serde_json::to_string_pretty(value)?)
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
            message: redact_sensitive_text(err.to_string().trim()),
            exit_code: err.exit_code(),
            remediation: None,
        },
    }
}

fn machine_error(err: &Error, exit_code: i32) -> MachineError {
    if let Some(typed) = err.downcast_ref::<AiswError>() {
        return MachineError {
            kind: typed.code().to_owned(),
            message: redact_sensitive_text(&typed.to_string()),
            exit_code,
            remediation: typed.remediation(),
        };
    }

    MachineError {
        kind: "validation_error".to_owned(),
        message: redact_sensitive_text(&err.to_string()),
        exit_code,
        remediation: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Tool;

    #[test]
    fn typed_machine_errors_redact_secrets() {
        let error = anyhow::Error::new(AiswError::AuthFailed {
            tool: Tool::Claude,
            reason: "provider rejected sk-ant-secret-value".to_owned(),
        });

        let machine = machine_error(&error, 1);

        assert_eq!(machine.kind, "auth_failed");
        assert!(!machine.message.contains("sk-ant-secret-value"));
        assert!(machine.message.contains("[REDACTED]"));
    }

    #[test]
    fn generic_machine_errors_redact_secrets() {
        let error = anyhow::anyhow!("command failed with token=AIza-secret-value");

        let machine = machine_error(&error, 1);

        assert_eq!(machine.kind, "validation_error");
        assert!(!machine.message.contains("AIza-secret-value"));
        assert!(machine.message.contains("[REDACTED]"));
    }

    #[test]
    fn parse_machine_errors_redact_secrets() {
        let error = crate::cli::parse_from(
            [
                "aisw",
                "add",
                "claude",
                "work",
                "--credential-backend",
                "sk-ant-secret-value",
            ],
            true,
        )
        .expect_err("the invalid credential backend should produce a parse error");

        let failure = parse_error(None, &error);

        assert!(!failure.error.message.contains("sk-ant-secret-value"));
        assert!(failure.error.message.contains("[REDACTED]"));
    }
}

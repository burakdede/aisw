use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum Tool {
    Claude,
    Codex,
    Gemini,
    Antigravity,
}

impl Tool {
    pub const ALL: [Tool; 4] = [Tool::Claude, Tool::Codex, Tool::Gemini, Tool::Antigravity];

    pub fn binary_name(&self) -> &'static str {
        match self {
            Tool::Claude => "claude",
            Tool::Codex => "codex",
            Tool::Gemini => "gemini",
            Tool::Antigravity => "agy",
        }
    }

    pub fn dir_name(&self) -> &'static str {
        match self {
            Tool::Claude => "claude",
            Tool::Codex => "codex",
            Tool::Gemini => "gemini",
            Tool::Antigravity => "antigravity",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            Tool::Claude => "Claude Code",
            Tool::Codex => "Codex CLI",
            Tool::Gemini => "Gemini CLI",
            Tool::Antigravity => "Antigravity CLI",
        }
    }

    pub fn supports_state_mode(self) -> bool {
        matches!(self, Tool::Claude | Tool::Codex)
    }
}

impl std::fmt::Display for Tool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.binary_name())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, clap::ValueEnum, Default)]
#[serde(rename_all = "snake_case")]
pub enum StateMode {
    #[default]
    Isolated,
    Shared,
}

impl StateMode {
    pub fn display_name(self) -> &'static str {
        match self {
            Self::Isolated => "isolated",
            Self::Shared => "shared",
        }
    }
}

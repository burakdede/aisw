#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, clap::ValueEnum)]
pub enum Tool {
    Claude,
    Codex,
    Gemini,
}

impl Tool {
    pub fn binary_name(&self) -> &'static str {
        match self {
            Tool::Claude => "claude",
            Tool::Codex => "codex",
            Tool::Gemini => "gemini",
        }
    }

    pub fn dir_name(&self) -> &'static str {
        match self {
            Tool::Claude => "claude",
            Tool::Codex => "codex",
            Tool::Gemini => "gemini",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            Tool::Claude => "Claude Code",
            Tool::Codex => "Codex CLI",
            Tool::Gemini => "Gemini CLI",
        }
    }
}

impl std::fmt::Display for Tool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.binary_name())
    }
}

use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Provider {
    HuggingFace,
    OpenAI,
    Anthropic,
    Gemini,
}

impl Provider {
    pub const ALL: [Self; 4] = [
        Self::HuggingFace,
        Self::OpenAI,
        Self::Anthropic,
        Self::Gemini,
    ];

    pub const fn display_name(self) -> &'static str {
        match self {
            Self::HuggingFace => "Hugging Face",
            Self::OpenAI => "OpenAI",
            Self::Anthropic => "Anthropic",
            Self::Gemini => "Gemini",
        }
    }

    pub const fn cli_name(self) -> &'static str {
        match self {
            Self::HuggingFace => "hf",
            Self::OpenAI => "openai",
            Self::Anthropic => "anthropic",
            Self::Gemini => "gemini",
        }
    }

    pub const fn env_var(self) -> &'static str {
        match self {
            Self::HuggingFace => "HF_TOKEN",
            Self::OpenAI => "OPENAI_API_KEY",
            Self::Anthropic => "ANTHROPIC_API_KEY",
            Self::Gemini => "GEMINI_API_KEY",
        }
    }

    pub const fn keychain_account(self) -> &'static str {
        match self {
            Self::HuggingFace => "huggingface",
            Self::OpenAI => "openai",
            Self::Anthropic => "anthropic",
            Self::Gemini => "gemini",
        }
    }
}

impl fmt::Display for Provider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.display_name())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeySource {
    Env,
    Keychain,
}

impl fmt::Display for KeySource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Env => f.write_str(".env/env"),
            Self::Keychain => f.write_str("keychain"),
        }
    }
}

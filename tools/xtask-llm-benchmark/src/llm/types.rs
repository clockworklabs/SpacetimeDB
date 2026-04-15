use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum Vendor {
    OpenAi,
    Anthropic,
    Google, // Gemini
    Xai,    // Grok
    DeepSeek,
    Meta, // Llama
}

impl Vendor {
    /// canonical, lowercase slug for file keys / merges
    pub fn slug(&self) -> &'static str {
        match self {
            Vendor::OpenAi => "openai",
            Vendor::Anthropic => "anthropic",
            Vendor::Google => "google",
            Vendor::Xai => "xai",
            Vendor::DeepSeek => "deepseek",
            Vendor::Meta => "meta",
        }
    }

    /// display/capitalized name (UI)
    pub fn display_name(&self) -> &'static str {
        match self {
            Vendor::OpenAi => "OpenAI",
            Vendor::Anthropic => "Anthropic",
            Vendor::Google => "Google",
            Vendor::Xai => "xAI",
            Vendor::DeepSeek => "DeepSeek",
            Vendor::Meta => "Meta",
        }
    }

    /// parse common user inputs (case-insensitive; accepts synonyms)
    pub fn parse(input: &str) -> Option<Self> {
        let s = input.trim().to_ascii_lowercase();
        Some(match s.as_str() {
            "openai" | "oai" => Vendor::OpenAi,
            "anthropic" | "claude" => Vendor::Anthropic,
            "google" | "gemini" => Vendor::Google,
            "xai" | "grok" => Vendor::Xai,
            "deepseek" => Vendor::DeepSeek,
            "meta" | "llama" => Vendor::Meta,
            _ => return None,
        })
    }
}

impl fmt::Display for Vendor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.slug())
    }
}

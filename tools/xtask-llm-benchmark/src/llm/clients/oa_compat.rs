//! Shared types for OpenAI-compatible chat completions responses.
//! Used by DeepSeek, Meta/OpenRouter, and OpenRouter clients.

use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct OACompatResp {
    #[serde(default)]
    pub choices: Vec<Choice>,
    #[serde(default)]
    pub usage: Option<UsageInfo>,
    /// OpenRouter returns an `error` object when the upstream provider fails.
    /// Absent (None) for direct vendor APIs.
    #[serde(default)]
    pub error: Option<OAError>,
}

#[derive(Debug, Deserialize)]
pub struct OAError {
    pub message: String,
}

#[derive(Debug, Deserialize)]
pub struct Choice {
    pub message: MsgOut,
}

#[derive(Debug, Deserialize)]
pub struct MsgOut {
    pub content: String,
}

#[derive(Debug, Deserialize)]
pub struct UsageInfo {
    #[serde(default)]
    pub prompt_tokens: Option<u32>,
    #[serde(default)]
    pub completion_tokens: Option<u32>,
}

impl OACompatResp {
    pub fn first_text(self) -> Option<String> {
        self.choices.into_iter().next().map(|c| c.message.content)
    }
}

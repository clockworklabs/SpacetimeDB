//! Shared types for OpenAI-compatible chat completions responses.
//! Used by DeepSeek, Meta/OpenRouter, and OpenRouter clients.

use anyhow::{anyhow, bail, Result};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct OACompatResp {
    pub id: Option<String>,
    pub provider: Option<String>,
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
    pub code: Option<serde_json::Value>,
    pub message: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct Choice {
    pub message: Option<MsgOut>,
    pub finish_reason: Option<String>,
    pub error: Option<OAError>,
}

#[derive(Debug, Deserialize)]
pub struct MsgOut {
    pub content: Option<String>,
    pub refusal: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UsageInfo {
    #[serde(default)]
    pub prompt_tokens: Option<u32>,
    #[serde(default)]
    pub completion_tokens: Option<u32>,
}

impl OACompatResp {
    pub fn first_text(self) -> Result<String> {
        let choice = self.choices.first();
        let finish = choice.and_then(|c| c.finish_reason.as_deref());
        // Keep diagnostic fields, not prompts, generated content, or raw response bodies.
        let diagnostic = format!(
            "id={}, provider={}, finish_reason={}, refusal={}",
            diagnostic_field(self.id.as_deref()),
            diagnostic_field(self.provider.as_deref()),
            diagnostic_field(finish),
            choice
                .and_then(|c| c.message.as_ref())
                .is_some_and(|m| m.refusal.is_some()),
        );
        if let Some(error) = self.error.as_ref().or_else(|| choice.and_then(|c| c.error.as_ref())) {
            bail!(
                "upstream error ({diagnostic}, code={}): {}",
                diagnostic_field(error.code.as_ref().map(|v| v.to_string()).as_deref()),
                diagnostic_field(error.message.as_deref()),
            );
        }
        if matches!(finish, Some("error" | "content_filter")) {
            bail!("unsuccessful completion ({diagnostic})");
        }
        self.choices
            .into_iter()
            .next()
            .and_then(|c| c.message)
            .and_then(|m| m.content)
            .filter(|text| !text.trim().is_empty())
            .ok_or_else(|| anyhow!("no nonempty completion content ({diagnostic})"))
    }
}

fn diagnostic_field(value: Option<&str>) -> String {
    value
        .unwrap_or("unknown")
        .chars()
        .filter(|c| !c.is_control())
        .take(512)
        .collect()
}

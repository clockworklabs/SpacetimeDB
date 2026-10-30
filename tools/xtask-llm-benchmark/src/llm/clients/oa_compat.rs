//! Shared types for OpenAI-compatible chat completions responses.
//! Used by DeepSeek, Meta/OpenRouter, and OpenRouter clients.

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};

/// Retain provider evidence in local artifacts, never in console error messages.
#[derive(Serialize)]
pub(crate) struct ProviderRefusal {
    pub generation_id: Option<String>,
    pub provider: Option<String>,
    pub finish_reason: Option<String>,
    pub native_finish_reason: Option<String>,
    pub refusal: Option<String>,
    pub content: Option<String>,
}

impl std::fmt::Display for ProviderRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "provider refusal (id={}, provider={}, finish_reason={}, native_finish_reason={}, refusal=true)",
            diagnostic_field(self.generation_id.as_deref()),
            diagnostic_field(self.provider.as_deref()),
            diagnostic_field(self.finish_reason.as_deref()),
            diagnostic_field(self.native_finish_reason.as_deref())
        )
    }
}

impl std::fmt::Debug for ProviderRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(self, f)
    }
}

impl std::error::Error for ProviderRefusal {}

#[derive(Serialize)]
pub(crate) struct ProviderResponseError {
    diagnostic: String,
    pub error: OAError,
    pub content: Option<String>,
    pub refusal: Option<String>,
    pub native_finish_reason: Option<String>,
}

impl std::fmt::Display for ProviderResponseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "upstream error ({}, code={})",
            self.diagnostic,
            self.error
                .code
                .as_ref()
                .and_then(serde_json::Value::as_u64)
                .map(|code| code.to_string())
                .unwrap_or_else(|| "unknown".into())
        )
    }
}

impl std::fmt::Debug for ProviderResponseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(self, f)
    }
}

impl std::error::Error for ProviderResponseError {}

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

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OAError {
    pub code: Option<serde_json::Value>,
    pub message: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct Choice {
    pub message: Option<MsgOut>,
    pub finish_reason: Option<String>,
    pub native_finish_reason: Option<String>,
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
        let error = self.error.as_ref().or_else(|| choice.and_then(|c| c.error.as_ref()));
        if error.is_some() || finish == Some("error") {
            return Err(ProviderResponseError {
                diagnostic,
                error: error.cloned().unwrap_or_else(|| OAError {
                    code: None,
                    message: Some("unsuccessful completion".into()),
                }),
                content: choice.and_then(|c| c.message.as_ref()).and_then(|m| m.content.clone()),
                refusal: choice.and_then(|c| c.message.as_ref()).and_then(|m| m.refusal.clone()),
                native_finish_reason: choice.and_then(|c| c.native_finish_reason.clone()),
            }
            .into());
        }
        if let Some(choice) = choice
            && (finish == Some("content_filter")
                || choice.native_finish_reason.as_deref() == Some("refusal")
                || choice
                    .message
                    .as_ref()
                    .is_some_and(|m| m.refusal.as_deref().is_some_and(|text| !text.trim().is_empty())))
        {
            return Err(ProviderRefusal {
                generation_id: self.id.clone(),
                provider: self.provider.clone(),
                finish_reason: choice.finish_reason.clone(),
                native_finish_reason: choice.native_finish_reason.clone(),
                refusal: choice.message.as_ref().and_then(|m| m.refusal.clone()),
                content: choice.message.as_ref().and_then(|m| m.content.clone()),
            }
            .into());
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_refusal_field_does_not_reject_valid_content() {
        let response: OACompatResp = serde_json::from_value(serde_json::json!({
            "choices": [{"finish_reason": "stop", "message": {"content": "valid code", "refusal": ""}}]
        }))
        .unwrap();
        assert_eq!(response.first_text().unwrap(), "valid code");
    }

    #[test]
    fn refusals_preserve_details_without_logging_text_or_scoring_partial_code() {
        for (finish, native, refusal) in [
            ("content_filter", "refusal", Some("full refusal details")),
            ("stop", "refusal", None),
            ("stop", "stop", Some("full refusal details")),
        ] {
            let response: OACompatResp = serde_json::from_value(serde_json::json!({
                "id": "generation-1", "provider": "test-provider",
                "choices": [{"finish_reason": finish, "native_finish_reason": native,
                    "message": {"content": "partial code", "refusal": refusal}}]
            }))
            .unwrap();
            let error = response.first_text().unwrap_err().context("model test");
            let detail = error
                .downcast_ref::<ProviderRefusal>()
                .expect("typed refusal survives context");
            assert_eq!(detail.generation_id.as_deref(), Some("generation-1"));
            assert_eq!(detail.provider.as_deref(), Some("test-provider"));
            assert_eq!(detail.native_finish_reason.as_deref(), Some(native));
            assert_eq!(detail.refusal.as_deref(), refusal);
            assert_eq!(detail.content.as_deref(), Some("partial code"));
            let logged = format!("{error:?} {error:#}");
            assert!(!logged.contains("full refusal details"));
            assert!(!logged.contains("partial code"));
        }
    }
}

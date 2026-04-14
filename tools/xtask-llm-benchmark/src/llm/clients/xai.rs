use super::http::HttpClient;
use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};

use crate::llm::prompt::BuiltPrompt;
use crate::llm::segmentation::{
    deterministic_trim_prefix, non_context_reserve_tokens_env, xai_ctx_limit_tokens, Segment,
};
use crate::llm::types::Vendor;

#[derive(Clone)]
pub struct XaiGrokClient {
    base: String, // e.g. https://api.x.ai/v1
    api_key: String,
    http: HttpClient,
}

impl XaiGrokClient {
    pub fn new(http: HttpClient, base: String, api_key: String) -> Self {
        Self { base, api_key, http }
    }

    /// Uses BuiltPrompt (system, static_prefix, segments) and maps to xAI /chat/completions.
    pub async fn generate(&self, model: &str, prompt: &BuiltPrompt) -> Result<String> {
        let url = format!("{}/v1/chat/completions", self.base.trim_end_matches('/'));

        // Never trim system or dynamic segments
        let system = prompt.system.clone();
        let segs: Vec<Segment<'_>> = prompt.segments.clone();

        // Trim ONLY the static prefix by: ctx_limit - headroom - tokens(system + segments)
        let mut static_prefix = prompt.static_prefix.clone().unwrap_or_default();

        let ctx_limit = xai_ctx_limit_tokens(model);
        let reserve = non_context_reserve_tokens_env(Vendor::Xai);
        let allowance = ctx_limit.saturating_sub(reserve);
        static_prefix = deterministic_trim_prefix(&static_prefix, allowance);

        #[derive(Serialize)]
        struct Req<'a> {
            model: &'a str,
            messages: Vec<Msg<'a>>,
            temperature: f32,
        }

        #[derive(Serialize)]
        struct Msg<'a> {
            role: &'a str, // "system" | "user" | "assistant"
            content: &'a str,
        }

        // Build messages in provider-preferred order
        let mut messages: Vec<Msg> = Vec::new();

        if let Some(sys) = system.as_deref() {
            messages.push(Msg {
                role: "system",
                content: sys,
            });
        }
        if !static_prefix.is_empty() {
            messages.push(Msg {
                role: "user",
                content: &static_prefix,
            });
        }
        for s in &segs {
            messages.push(Msg {
                role: s.role,
                content: &s.text,
            });
        }

        let req = Req {
            model,
            messages,
            temperature: 0.0,
        };

        let auth = HttpClient::bearer(&self.api_key);
        let body = self.http.post_json(&url, &[auth], &req).await?;
        let resp: GrokChatResp = serde_json::from_str(&body).context("parse grok resp")?;
        resp.into_first_text().ok_or_else(|| anyhow!("no content from Grok"))
    }
}

#[derive(Debug, Deserialize)]
struct GrokChatResp {
    choices: Vec<GrokChoice>,
}
#[derive(Debug, Deserialize)]
struct GrokChoice {
    message: GrokMsgOut,
}
#[derive(Debug, Deserialize)]
struct GrokMsgOut {
    content: String,
}

impl GrokChatResp {
    fn into_first_text(self) -> Option<String> {
        self.choices.into_iter().next().map(|c| c.message.content)
    }
}

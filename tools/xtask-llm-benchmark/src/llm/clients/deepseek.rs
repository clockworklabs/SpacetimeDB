use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};

use super::http::HttpClient;
use crate::llm::prompt::BuiltPrompt;
use crate::llm::segmentation::{
    deepseek_ctx_limit_tokens, deterministic_trim_prefix, non_context_reserve_tokens_env, Segment,
};
use crate::llm::types::Vendor;

#[derive(Clone)]
pub struct DeepSeekClient {
    base: String, // e.g. https://api.deepseek.com/v1
    api_key: String,
    http: HttpClient,
}

impl DeepSeekClient {
    pub fn new(http: HttpClient, base: String, api_key: String) -> Self {
        Self { base, api_key, http }
    }

    pub async fn generate(&self, model: &str, prompt: &BuiltPrompt) -> Result<String> {
        let url = format!("{}/chat/completions", self.base.trim_end_matches('/'));

        let system = prompt.system.clone();
        let segs: Vec<Segment<'_>> = prompt.segments.clone();

        let mut static_prefix = prompt.static_prefix.clone().unwrap_or_default();

        let ctx_limit = deepseek_ctx_limit_tokens(model);
        let reserve = non_context_reserve_tokens_env(Vendor::DeepSeek);
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
            role: &'a str,
            content: &'a str,
        }

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
        let resp: OACompatResp = serde_json::from_str(&body).context("parse deepseek resp")?;
        resp.first_text().ok_or_else(|| anyhow!("no content from DeepSeek"))
    }
}

#[derive(Debug, Deserialize)]
struct OACompatResp {
    choices: Vec<Choice>,
}
#[derive(Debug, Deserialize)]
struct Choice {
    message: MsgOut,
}
#[derive(Debug, Deserialize)]
struct MsgOut {
    content: String,
}
impl OACompatResp {
    fn first_text(self) -> Option<String> {
        self.choices.into_iter().next().map(|c| c.message.content)
    }
}

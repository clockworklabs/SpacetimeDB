use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};

use super::http::HttpClient;
use crate::llm::prompt::BuiltPrompt;
use crate::llm::segmentation::{
    deterministic_trim_prefix, meta_ctx_limit_tokens, non_context_reserve_tokens_env, Segment,
};
use crate::llm::types::Vendor;

#[derive(Clone)]
pub struct MetaLlamaClient {
    /// e.g. https://openrouter.ai/api/v1
    base: String,
    api_key: String,
    http: HttpClient,
}

impl MetaLlamaClient {
    pub fn new(http: HttpClient, base: String, api_key: String) -> Self {
        Self { base, api_key, http }
    }

    pub async fn generate(&self, model: &str, prompt: &BuiltPrompt) -> Result<String> {
        let url = format!("{}/chat/completions", self.base.trim_end_matches('/'));

        // Build input like other clients
        let system = prompt.system.clone();
        let segs: Vec<Segment<'_>> = prompt.segments.clone();

        // Trim static prefix against vendor/model allowance
        let mut static_prefix = prompt.static_prefix.clone().unwrap_or_default();
        let ctx_limit = meta_ctx_limit_tokens(model);
        let reserve = non_context_reserve_tokens_env(Vendor::Meta);
        let allowance = ctx_limit.saturating_sub(reserve);
        static_prefix = deterministic_trim_prefix(&static_prefix, allowance);

        // OpenAI-compatible schema
        #[derive(Serialize)]
        struct Req<'a> {
            model: &'a str,
            messages: Vec<Msg<'a>>,
            temperature: f32,
            #[serde(skip_serializing_if = "Option::is_none")]
            max_tokens: Option<u32>,
        }

        #[derive(Serialize)]
        struct Msg<'a> {
            role: &'a str,
            content: &'a str,
        }

        let mut messages: Vec<Msg> = Vec::new();

        if let Some(sys) = system.as_deref() {
            if !sys.is_empty() {
                messages.push(Msg {
                    role: "system",
                    content: sys,
                });
            }
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

        // Normalize to OpenRouter slugs when a friendly ID is passed
        let wire_model = normalize_meta_model(model);

        let req = Req {
            model: wire_model,
            messages,
            temperature: 0.0,
            max_tokens: None,
        };

        // Auth only; optional OpenRouter headers can live in HttpClient if desired
        let auth = HttpClient::bearer(&self.api_key);
        let body = self
            .http
            .post_json(&url, &[auth], &req)
            .await
            .with_context(|| format!("OpenRouter (Meta) POST {}", url))?;

        let resp: OACompatResp = serde_json::from_str(&body).context("parse OpenRouter (Meta) response")?;
        resp.first_text()
            .ok_or_else(|| anyhow!("no content from Meta/OpenRouter"))
    }
}

// Map friendly names → OpenRouter slugs. Extend as needed.
fn normalize_meta_model(id: &str) -> &str {
    match id {
        // OpenRouter slugs
        "meta-llama/llama-3.1-405b-instruct" => "meta-llama/llama-3.1-405b-instruct",
        "meta-llama/llama-3.1-70b-instruct" => "meta-llama/llama-3.1-70b-instruct",
        "meta-llama/llama-3.1-8b-instruct" => "meta-llama/llama-3.1-8b-instruct",

        // Friendly aliases -> slugs
        "llama-3.1-405b-instruct" | "llama3.1-405b-instruct" | "llama-3.1-405b" => "meta-llama/llama-3.1-405b-instruct",
        "llama-3.1-70b-instruct" | "llama3.1-70b-instruct" | "llama-3.1-70b" => "meta-llama/llama-3.1-70b-instruct",
        "llama-3.1-8b-instruct" | "llama3.1-8b-instruct" | "llama-3.1-8b" => "meta-llama/llama-3.1-8b-instruct",

        other => other,
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

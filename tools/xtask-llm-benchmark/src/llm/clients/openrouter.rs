use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};

use super::http::HttpClient;
use crate::llm::prompt::BuiltPrompt;
use crate::llm::segmentation::{deterministic_trim_prefix, non_context_reserve_tokens_env, Segment};
use crate::llm::types::{LlmOutput, Vendor};

const OPENROUTER_BASE: &str = "https://openrouter.ai/api/v1";

/// Default context limit for OpenRouter models (conservative).
/// Per-model overrides can be added to `openrouter_ctx_limit_tokens`.
const DEFAULT_CTX_LIMIT: usize = 128_000;

#[derive(Clone)]
pub struct OpenRouterClient {
    base: String,
    api_key: String,
    http: HttpClient,
}

impl OpenRouterClient {
    pub fn new(http: HttpClient, api_key: String) -> Self {
        Self {
            base: OPENROUTER_BASE.to_string(),
            api_key,
            http,
        }
    }

    pub fn with_base(http: HttpClient, base: String, api_key: String) -> Self {
        Self { base, api_key, http }
    }

    pub async fn generate(&self, model: &str, prompt: &BuiltPrompt) -> Result<LlmOutput> {
        let url = format!("{}/chat/completions", self.base.trim_end_matches('/'));

        let system = prompt.system.clone();
        let segs: Vec<Segment<'_>> = prompt.segments.clone();

        // Trim static prefix against model's context allowance
        let mut static_prefix = prompt.static_prefix.clone().unwrap_or_default();
        let ctx_limit = openrouter_ctx_limit_tokens(model);
        // Use a generic reserve since we don't know the vendor ahead of time.
        // OpenRouter routes to the right vendor, so this is a safe conservative default.
        let reserve = non_context_reserve_tokens_env(Vendor::OpenRouter);
        let allowance = ctx_limit.saturating_sub(reserve);
        static_prefix = deterministic_trim_prefix(&static_prefix, allowance);

        // OpenAI-compatible chat completions schema
        #[derive(Serialize)]
        struct Req<'a> {
            model: &'a str,
            messages: Vec<Msg<'a>>,
            temperature: f32,
            #[serde(skip_serializing_if = "Option::is_none")]
            top_p: Option<f32>,
            #[serde(skip_serializing_if = "Option::is_none")]
            max_tokens: Option<u32>,
        }

        #[derive(Serialize)]
        struct Msg<'a> {
            role: &'a str,
            content: &'a str,
        }

        let mut messages: Vec<Msg> = Vec::new();

        if let Some(sys) = system.as_deref()
            && !sys.is_empty()
        {
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
            top_p: Some(0.9),
            max_tokens: None,
        };

        let auth = HttpClient::bearer(&self.api_key);
        let body = self
            .http
            .post_json(&url, &[auth], &req)
            .await
            .with_context(|| format!("OpenRouter POST {}", url))?;

        let resp: OACompatResp = serde_json::from_str(&body).context("parse OpenRouter response")?;
        // Check for upstream provider errors returned by OpenRouter.
        if let Some(err) = resp.error {
            anyhow::bail!("OpenRouter upstream error (model={}): {}", model, err.message);
        }
        let input_tokens = resp.usage.as_ref().and_then(|u| u.prompt_tokens);
        let output_tokens = resp.usage.as_ref().and_then(|u| u.completion_tokens);
        let text = resp
            .first_text()
            .ok_or_else(|| anyhow!("no content from OpenRouter (model={})", model))?;
        Ok(LlmOutput {
            text,
            input_tokens,
            output_tokens,
        })
    }
}

/// Context limits for models accessed via OpenRouter.
/// Uses the same limits as direct clients where known,
/// falls back to a conservative default.
pub fn openrouter_ctx_limit_tokens(model: &str) -> usize {
    let m = model.to_ascii_lowercase();

    // Anthropic
    if m.contains("claude") {
        return 185_000;
    }
    // OpenAI
    if m.contains("gpt-5") || m.contains("gpt-4.1") {
        return 400_000;
    }
    if m.contains("gpt-4o") || m.contains("gpt-4") {
        return 128_000;
    }
    // xAI / Grok — leave ~50 k headroom for segments + output on top of trimmed prefix
    if m.contains("grok-code-fast") {
        return 200_000;
    }
    if m.contains("grok-4") {
        return 200_000;
    }
    if m.contains("grok") {
        return 90_000;
    }
    // DeepSeek — hard cap is 131 072 on OpenRouter; leave ~25 k headroom for segments + output
    if m.contains("deepseek") {
        return 106_000;
    }
    // Gemini
    if m.contains("gemini") {
        return 900_000;
    }
    // Meta / Llama
    if m.contains("maverick") {
        return 992_000;
    }
    if m.contains("scout") {
        return 320_000;
    }
    if m.contains("llama") {
        return 120_000;
    }

    DEFAULT_CTX_LIMIT
}

#[derive(Debug, Deserialize)]
struct OACompatResp {
    #[serde(default)]
    choices: Vec<Choice>,
    #[serde(default)]
    usage: Option<UsageInfo>,
    /// OpenRouter returns an `error` object when the upstream provider fails.
    #[serde(default)]
    error: Option<OAError>,
}
#[derive(Debug, Deserialize)]
struct OAError {
    message: String,
}
#[derive(Debug, Deserialize)]
struct Choice {
    message: MsgOut,
}
#[derive(Debug, Deserialize)]
struct MsgOut {
    content: String,
}
#[derive(Debug, Deserialize)]
struct UsageInfo {
    #[serde(default)]
    prompt_tokens: Option<u32>,
    #[serde(default)]
    completion_tokens: Option<u32>,
}
impl OACompatResp {
    fn first_text(self) -> Option<String> {
        self.choices.into_iter().next().map(|c| c.message.content)
    }
}

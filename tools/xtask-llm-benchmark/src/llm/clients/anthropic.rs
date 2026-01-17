use crate::bench::utils::debug_llm_verbose;
use crate::llm::prompt::BuiltPrompt;
use crate::llm::segmentation::{
    anthropic_ctx_limit_tokens, build_anthropic_messages, desired_output_tokens, deterministic_trim_prefix,
    non_context_reserve_tokens_env,
};
use crate::llm::types::Vendor;
use anyhow::{anyhow, bail, Context, Result};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue, CONTENT_TYPE};
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};

#[derive(Clone)]
pub struct AnthropicClient {
    base: String,
    api_key: String,
    client: Client,
}

impl AnthropicClient {
    pub fn new(base: String, api_key: String) -> Self {
        Self {
            base: normalize_base(&base),
            api_key,
            client: Client::new(),
        }
    }

    fn url_messages(&self) -> String {
        format!("{}/v1/messages", self.base.trim_end_matches('/'))
    }

    pub async fn generate(&self, model: &str, prompt: &BuiltPrompt) -> Result<String> {
        let system = prompt.system.clone();
        let segs = prompt.segments.clone();
        let mut static_prefix = prompt.static_prefix.clone().unwrap_or_default();
        let model_norm = normalize_anthropic_model(model);

        let ctx_limit = anthropic_ctx_limit_tokens(model_norm);
        let reserve = non_context_reserve_tokens_env(Vendor::Anthropic);
        let allowance = ctx_limit.saturating_sub(reserve);
        static_prefix = deterministic_trim_prefix(&static_prefix, allowance);

        // Build messages, putting the context first for cache wins
        let (system_json, mut messages) = build_anthropic_messages(system.as_deref(), &segs);
        if !static_prefix.is_empty() {
            // Mark static prefix for caching - this content will be cached and reused
            // across multiple requests, significantly reducing costs for repeated prefixes
            messages.insert(
                0,
                serde_json::json!({
                    "role":"user",
                    "content":[{
                        "type":"text",
                        "text": static_prefix,
                        "cache_control": {"type": "ephemeral"}
                    }]
                }),
            );
        }

        // Anthropic requires max_tokens (output tokens). Default from env or a sane fallback.
        let max_tokens = anthropic_max_output_tokens();

        #[derive(Serialize)]
        struct Req {
            model: String,
            max_tokens: u32,
            #[serde(skip_serializing_if = "Option::is_none")]
            system: Option<serde_json::Value>,
            messages: Vec<serde_json::Value>,
        }
        let req = Req {
            model: model_norm.to_string(),
            max_tokens,
            system: system_json,
            messages,
        };

        let mut hm = HeaderMap::new();
        hm.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        hm.insert(
            HeaderName::from_static("x-api-key"),
            HeaderValue::from_str(&self.api_key)?,
        );
        hm.insert(
            HeaderName::from_static("anthropic-version"),
            HeaderValue::from_static("2023-06-01"),
        );
        // Enable prompt caching - reduces cost by ~90% for repeated prefixes
        hm.insert(
            HeaderName::from_static("anthropic-beta"),
            HeaderValue::from_static("prompt-caching-2024-07-31"),
        );

        let url = self.url_messages();
        let (status, body) = self.post_with_retries(&url, hm, &req).await?;
        if debug_llm_verbose() {
            let preview = if body.len() > 2000 {
                format!("{}…", &body[..2000])
            } else {
                body.clone()
            };
            eprintln!("[anthropic] http_status={} body_preview={}", status, preview);
        }
        if !status.is_success() {
            bail!("POST {} -> {}: {}", url, status, body);
        }

        #[derive(Deserialize)]
        struct MsgResp {
            #[serde(default)]
            content: Vec<ContentPart>,
        }
        #[derive(Deserialize)]
        struct ContentPart {
            #[serde(rename = "type")]
            _type: String,
            #[serde(default)]
            text: Option<String>,
        }

        let parsed: MsgResp = serde_json::from_str(&body).context("parse anthropic resp")?;
        parsed
            .content
            .into_iter()
            .find_map(|p| p.text)
            .ok_or_else(|| anyhow!("no text"))
    }

    async fn post_with_retries(
        &self,
        url: &str,
        headers: HeaderMap,
        payload: &impl serde::Serialize,
    ) -> Result<(StatusCode, String)> {
        let mut attempt = 0u32;
        loop {
            attempt += 1;
            let resp = self
                .client
                .post(url)
                .headers(headers.clone())
                .json(payload)
                .send()
                .await
                .with_context(|| format!("POST {} send failed", url))?;
            let status = resp.status();
            let h = resp.headers().clone();
            let body = resp.text().await.unwrap_or_default();

            if status.is_success() {
                return Ok((status, body));
            }

            let retryable = status == StatusCode::TOO_MANY_REQUESTS
                || status == StatusCode::BAD_GATEWAY
                || status == StatusCode::SERVICE_UNAVAILABLE
                || status == StatusCode::GATEWAY_TIMEOUT;

            if !retryable || attempt >= 10 {
                bail!("POST {} -> {}: {}", url, status, body);
            }

            let wait = compute_backoff(status, &h, attempt);
            eprintln!("[anthropic] {} attempt {} — backoff {:?}", status, attempt, wait);
            tokio::time::sleep(wait).await;
        }
    }
}

fn normalize_base(input: &str) -> String {
    let mut b = input.trim().trim_end_matches('/').to_string();
    if b.starts_with("http://") {
        panic!("Anthropic base must be HTTPS");
    }
    if !b.starts_with("https://") {
        b = format!("https://{}", b);
    }
    if b.ends_with("/v1") {
        b.truncate(b.len() - 3);
    }
    b
}

fn h_f64(h: &HeaderMap, k: &str) -> Option<f64> {
    h.get(k).and_then(|v| v.to_str().ok())?.parse().ok()
}

fn compute_backoff(status: StatusCode, headers: &HeaderMap, attempt: u32) -> std::time::Duration {
    if status == StatusCode::TOO_MANY_REQUESTS {
        if let Some(s) = h_f64(headers, "retry-after") {
            return std::time::Duration::from_secs_f64(s.max(0.25));
        }
        if let Some(s) = h_f64(headers, "anthropic-ratelimit-tokens-reset") {
            return std::time::Duration::from_secs_f64(s.max(0.25));
        }
        if let Some(s) = h_f64(headers, "anthropic-ratelimit-requests-reset") {
            return std::time::Duration::from_secs_f64(s.max(0.25));
        }
    }
    let shift = attempt.min(8);
    let factor = 1u64.checked_shl(shift).unwrap_or(u64::MAX);
    let base_ms = 400u64.saturating_mul(factor);
    let cap_ms = 10_000u64;
    let jitter = (attempt as u64 * 137) % 300;
    std::time::Duration::from_millis(base_ms.min(cap_ms) + jitter)
}

fn anthropic_max_output_tokens() -> u32 {
    desired_output_tokens().max(1) as u32
}

pub fn normalize_anthropic_model(id: &str) -> &str {
    let lid = id.to_ascii_lowercase().replace('_', "-");
    match lid.as_str() {
        // Sonnet 4.5
        "sonnet-4.5" | "claude-sonnet-4.5" | "claude-sonnet-4-5" => "claude-sonnet-4-5",
        "claude-sonnet-4-5-20250929" => "claude-sonnet-4-5-20250929",

        // Sonnet 4
        "sonnet-4" | "claude-sonnet-4" => "claude-sonnet-4-20250514",

        _ => id, // return the original input; never the temporary
    }
}

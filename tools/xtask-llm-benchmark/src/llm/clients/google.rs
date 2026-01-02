use anyhow::{anyhow, Context, Result};
use reqwest::header::HeaderMap;
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};

use super::http::HttpClient;
use crate::llm::prompt::BuiltPrompt;
use crate::llm::segmentation::{
    deterministic_trim_prefix, gemini_ctx_limit_tokens, non_context_reserve_tokens_env, Segment,
};
use crate::llm::types::Vendor;

/// Google uses API key in the query string rather than Authorization header.
#[derive(Clone)]
pub struct GoogleGeminiClient {
    base: String, // e.g. https://generativelanguage.googleapis.com
    api_key: String,
    http: HttpClient,
}

impl GoogleGeminiClient {
    pub fn new(http: HttpClient, base: String, api_key: String) -> Self {
        Self { base, api_key, http }
    }

    pub async fn generate(&self, model: &str, prompt: &BuiltPrompt) -> Result<String> {
        // ---- Never trim system or dynamic segments ----
        let system = prompt.system.clone();
        let segs: Vec<Segment<'_>> = prompt.segments.clone();

        // ---- Trim ONLY the static prefix by: ctx_limit - headroom - tokens(system + segments)
        let mut static_prefix = prompt.static_prefix.clone().unwrap_or_default();

        let ctx_limit = gemini_ctx_limit_tokens(model);
        let reserve = non_context_reserve_tokens_env(Vendor::Google);
        let allowance = ctx_limit.saturating_sub(reserve);
        static_prefix = deterministic_trim_prefix(&static_prefix, allowance);

        let url = format!(
            "{}/v1beta/models/{}:generateContent?key={}",
            self.base.trim_end_matches('/'),
            urlencoding::encode(model),
            urlencoding::encode(&self.api_key)
        );

        // ----- Request payload -----
        #[derive(Serialize)]
        struct Req<'a> {
            #[serde(skip_serializing_if = "Option::is_none")]
            system_instruction: Option<SystemInstruction<'a>>,
            contents: Vec<Content<'a>>,
            #[serde(skip_serializing_if = "Option::is_none")]
            safety_settings: Option<Vec<SafetySetting>>,
        }

        #[derive(Serialize)]
        struct SystemInstruction<'a> {
            parts: [Part<'a>; 1],
        }

        #[derive(Serialize)]
        struct Content<'a> {
            role: &'a str,
            parts: Vec<Part<'a>>,
        }

        #[derive(Serialize)]
        struct Part<'a> {
            text: &'a str,
        }

        #[derive(Serialize)]
        struct SafetySetting {
            category: String,
            threshold: String,
        }

        let mut contents: Vec<Content<'_>> = Vec::new();

        // Static prefix first if present.
        if !static_prefix.is_empty() {
            contents.push(Content {
                role: "user",
                parts: vec![Part { text: &static_prefix }],
            });
        }

        // Dynamic segments in order (unchanged).
        for s in &segs {
            contents.push(Content {
                role: s.role,
                parts: vec![Part { text: &s.text }],
            });
        }

        let system_instruction = system.as_deref().map(|sys| SystemInstruction {
            parts: [Part { text: sys }],
        });

        let req = Req {
            system_instruction,
            contents,
            safety_settings: None,
        };

        // =======================
        // Tiered waits / retries:
        // =======================
        let body = {
            let mut attempt: u32 = 0;
            loop {
                attempt += 1;

                // Use raw so we can inspect status/headers before consuming the body
                let resp = self
                    .http
                    .post_json_raw(&url, &[], &req)
                    .await
                    .with_context(|| format!("POST {} send failed", url))?;

                let status = resp.status();
                let headers = resp.headers().clone();
                let text = resp.text().await.unwrap_or_default();

                if status.is_success() {
                    break text;
                }

                // Retry policy mirrors Anthropic: 429/502/503/504
                let retryable = status == StatusCode::TOO_MANY_REQUESTS
                    || status == StatusCode::BAD_GATEWAY
                    || status == StatusCode::SERVICE_UNAVAILABLE
                    || status == StatusCode::GATEWAY_TIMEOUT;

                if !retryable || attempt >= 10 {
                    return Err(anyhow!("POST {} -> {}: {}", url, status, text));
                }

                let wait = compute_backoff_google(status, &headers, attempt);
                eprintln!("[google] {} attempt {} — backoff {:?}", status, attempt, wait);
                tokio::time::sleep(wait).await;
            }
        };

        // ----- Parse response -----
        #[derive(Debug, Deserialize)]
        struct GeminiResp {
            candidates: Vec<Candidate>,
        }

        #[derive(Debug, Deserialize)]
        struct Candidate {
            content: ContentOut,
        }

        #[derive(Debug, Deserialize)]
        struct ContentOut {
            parts: Vec<PartOut>,
        }

        #[derive(Debug, Deserialize)]
        struct PartOut {
            #[serde(default)]
            text: Option<String>,
        }

        impl GeminiResp {
            fn first_text(self) -> Option<String> {
                self.candidates
                    .into_iter()
                    .flat_map(|c| c.content.parts)
                    .find_map(|p| p.text)
            }
        }

        let resp: GeminiResp = serde_json::from_str(&body).context("parse gemini response")?;
        let out = resp.first_text().ok_or_else(|| anyhow!("no text in Gemini response"))?;
        Ok(out)
    }
}

// ---- private helpers ----
fn compute_backoff_google(status: StatusCode, headers: &HeaderMap, attempt: u32) -> std::time::Duration {
    // 1) Honor server hints first
    if let Some(s) = header_f64(headers, "retry-after") {
        return std::time::Duration::from_secs_f64(s.max(0.25));
    }
    if status == StatusCode::TOO_MANY_REQUESTS {
        if let Some(s) = header_f64(headers, "x-ratelimit-reset") {
            return std::time::Duration::from_secs_f64(s.max(0.25));
        }
        if let Some(s) = header_f64(headers, "x-ratelimit-reset-seconds") {
            return std::time::Duration::from_secs_f64(s.max(0.25));
        }
        if let Some(ms) = header_f64(headers, "x-ratelimit-reset-ms") {
            return std::time::Duration::from_millis(ms.max(250.0) as u64);
        }
    }

    // 2) Fallback: capped exponential with light jitter
    let shift: u32 = attempt.min(8);
    let factor: u64 = 1u64.checked_shl(shift).unwrap_or(u64::MAX);
    let base_ms = 400u64.saturating_mul(factor);
    let cap_ms = 10_000u64;
    let jitter = (attempt as u64 * 137) % 300;
    std::time::Duration::from_millis(base_ms.min(cap_ms) + jitter)
}

fn header_f64(h: &HeaderMap, k: &str) -> Option<f64> {
    h.get(k).and_then(|v| v.to_str().ok())?.trim().parse::<f64>().ok()
}

use crate::bench::utils::debug_llm_verbose;
use crate::llm::prompt::BuiltPrompt;
use crate::llm::segmentation::{
    build_openai_responses_input, deterministic_trim_prefix, estimate_tokens, headroom_tokens_env,
    non_context_reserve_tokens_env, openai_ctx_limit_tokens,
};
use crate::llm::types::Vendor;
use anyhow::{bail, Context, Result};
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub struct OpenAiClient {
    client: Client,
    base: String,
    api_key: String,
}

impl OpenAiClient {
    pub fn new(base: String, api_key: String) -> Self {
        Self {
            client: Client::new(),
            base,
            api_key,
        }
    }

    fn responses_url(&self) -> String {
        format!("{}/v1/responses", self.base.trim_end_matches('/'))
    }

    pub async fn generate(&self, model: &str, prompt: &BuiltPrompt) -> Result<String> {
        let system = prompt.system.clone();
        let segs = prompt.segments.clone();

        let mut static_prefix = prompt.static_prefix.clone().unwrap_or_default();

        let headroom = headroom_tokens_env(Vendor::OpenAi);
        let ctx_limit = openai_ctx_limit_tokens(model);
        let reserve = non_context_reserve_tokens_env(Vendor::OpenAi);
        let allowance = ctx_limit.saturating_sub(reserve);
        static_prefix = deterministic_trim_prefix(&static_prefix, allowance);

        let static_opt = if static_prefix.is_empty() {
            None
        } else {
            Some(static_prefix.as_str())
        };

        // Build input (system + trimmed prefix + untouched segments)
        // Note: OpenAI's Responses API automatically caches repeated prefixes for
        // gpt-4o, gpt-4.1, gpt-5, and similar models. No explicit cache_control needed.
        // The static_prefix (docs) is placed first to maximize cache hits across tasks.
        let input = build_openai_responses_input(system.as_deref(), static_opt, &segs);

        if debug_llm_verbose() {
            eprintln!("\n[openai] model={model}");
            eprintln!(
                "[openai] system={} chars, {} tok",
                system.as_deref().map(|s| s.len()).unwrap_or(0),
                system.as_deref().map(estimate_tokens).unwrap_or(0)
            );

            eprintln!(
                "[openai] ctx_limit={} headroom={} sys_tok={} seg_tok={}",
                ctx_limit,
                headroom,
                system.as_deref().map(estimate_tokens).unwrap_or(0),
                segs.iter().map(|s| estimate_tokens(&s.text)).sum::<usize>()
            );

            for (i, s) in segs.iter().enumerate() {
                let head: String = s.text.chars().take(300).collect();
                eprintln!(
                    "[openai] seg[{i}] role={} chars={} tok={} head='{}...'",
                    s.role,
                    s.text.len(),
                    estimate_tokens(&s.text),
                    head
                );
            }
        }

        #[derive(Serialize)]
        struct Req<'a> {
            model: &'a str,
            input: Vec<Value>,
            #[serde(skip_serializing_if = "Option::is_none")]
            max_output_tokens: Option<u32>,
        }

        let url = self.responses_url();
        let payload = Req {
            model,
            input,
            max_output_tokens: None,
        };

        let (status, body) = Self::post_once(&self.client, &self.api_key, &url, &payload)
            .await
            .with_context(|| format!("POST {} send failed", url))?;

        if debug_llm_verbose() {
            let preview = if body.len() > 2000 {
                format!("{}…", &body[..2000])
            } else {
                body.clone()
            };
            eprintln!("[openai] http_status={} body_preview={}", status, preview);
        }

        if !status.is_success() {
            bail!("POST {} -> {}: {}", url, status, body);
        }

        #[derive(Deserialize)]
        struct ContentItem {
            #[serde(rename = "type")]
            #[serde(default)]
            _type: Option<String>,
            #[serde(default)]
            text: Option<String>,
        }
        #[derive(Deserialize)]
        struct OutputItem {
            #[serde(rename = "type")]
            #[serde(default)]
            r#type: Option<String>,
            #[serde(default)]
            content: Option<Vec<ContentItem>>,
            #[serde(default)]
            summary: Option<Vec<ContentItem>>,
        }
        #[derive(Deserialize)]
        struct ResponsesPayload {
            #[serde(default)]
            status: Option<String>,
            #[serde(default)]
            incomplete_details: Option<Value>,
            #[serde(default)]
            output_text: Option<String>,
            #[serde(default)]
            output: Option<Vec<OutputItem>>,
        }

        let parsed: ResponsesPayload = serde_json::from_str(&body).context("parse OpenAI response")?;

        if let Some(t) = parsed.output_text.as_ref() {
            if !t.is_empty() {
                return Ok(t.clone());
            }
        }
        if let Some(items) = parsed.output.as_ref() {
            for it in items {
                if it.r#type.as_deref() == Some("message") {
                    if let Some(content) = &it.content {
                        for c in content {
                            if let Some(txt) = &c.text {
                                if !txt.is_empty() {
                                    return Ok(txt.clone());
                                }
                            }
                        }
                    }
                }
            }
            for it in items {
                if it.r#type.as_deref() == Some("reasoning") {
                    if let Some(summary) = &it.summary {
                        for c in summary {
                            if let Some(txt) = &c.text {
                                if !txt.is_empty() {
                                    return Ok(txt.clone());
                                }
                            }
                        }
                    }
                }
            }
        }

        let status_s = parsed.status.unwrap_or_default();
        let details = parsed.incomplete_details.unwrap_or(Value::Null);
        bail!("no text output (status='{}', incomplete_details={})", status_s, details);
    }

    async fn post_once(
        client: &Client,
        api_key: &str,
        url: &str,
        payload: &impl Serialize,
    ) -> reqwest::Result<(StatusCode, String)> {
        let resp = client
            .post(url)
            .bearer_auth(api_key)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .json(payload)
            .send()
            .await?;
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        Ok((status, body))
    }
}

use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::env;

use super::http::HttpClient;
use super::oa_compat::OACompatResp;
use crate::llm::prompt::BuiltPrompt;
use crate::llm::segmentation::{
    deterministic_trim_prefix, non_context_reserve_tokens_env, output_token_limit_env, Segment,
};
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

    pub async fn preflight_credits(&self, model: &str) -> Result<OpenRouterCreditStatus> {
        let key_info = self.fetch_key_info().await?;
        let min_credits = min_credits_threshold();
        let mut unchecked_allowed = false;
        let mut model_probe = None;

        if let Some(remaining) = key_info.limit_remaining
            && remaining <= min_credits
        {
            bail!(
                "OpenRouter API key has insufficient remaining credits: {:.4} <= {:.4}",
                remaining,
                min_credits
            );
        }

        let account = match env::var("OPENROUTER_MANAGEMENT_API_KEY")
            .ok()
            .filter(|v| !v.trim().is_empty())
        {
            Some(key) => Some(self.fetch_account_credits(&key).await?),
            None => None,
        };

        if let Some(account) = &account
            && account.remaining <= min_credits
        {
            bail!(
                "OpenRouter account has insufficient remaining credits: {:.4} <= {:.4}",
                account.remaining,
                min_credits
            );
        }

        if account.is_none() && key_info.limit_remaining.is_none() {
            if allow_unchecked_credits() {
                unchecked_allowed = true;
            } else {
                self.probe_model(model).await?;
                model_probe = Some(model.to_string());
            }
        }

        Ok(OpenRouterCreditStatus {
            key_limit: key_info.limit,
            key_limit_remaining: key_info.limit_remaining,
            account_remaining: account.map(|a| a.remaining),
            min_credits,
            model_probe,
            unchecked_allowed,
        })
    }

    async fn fetch_key_info(&self) -> Result<OpenRouterKeyInfo> {
        let url = format!("{}/key", self.base.trim_end_matches('/'));
        let auth = HttpClient::bearer(&self.api_key);
        let body = self
            .http
            .get_text(&url, &[auth])
            .await
            .with_context(|| format!("OpenRouter key preflight GET {}", url))?;

        let resp: OpenRouterKeyResp = serde_json::from_str(&body).context("parse OpenRouter key response")?;
        Ok(resp.data)
    }

    async fn fetch_account_credits(&self, management_key: &str) -> Result<OpenRouterAccountCredits> {
        let url = format!("{}/credits", self.base.trim_end_matches('/'));
        let auth = HttpClient::bearer(management_key);
        let body = self
            .http
            .get_text(&url, &[auth])
            .await
            .with_context(|| format!("OpenRouter account credit preflight GET {}", url))?;

        let resp: OpenRouterCreditsResp = serde_json::from_str(&body).context("parse OpenRouter credits response")?;
        Ok(OpenRouterAccountCredits {
            remaining: resp.data.total_credits - resp.data.total_usage,
        })
    }

    async fn probe_model(&self, model: &str) -> Result<()> {
        let url = format!("{}/chat/completions", self.base.trim_end_matches('/'));

        #[derive(Serialize)]
        struct Req<'a> {
            model: &'a str,
            messages: [Msg<'a>; 1],
            temperature: f32,
            max_tokens: u32,
        }

        #[derive(Serialize)]
        struct Msg<'a> {
            role: &'a str,
            content: &'a str,
        }

        let req = Req {
            model,
            messages: [Msg {
                role: "user",
                content: "ping",
            }],
            temperature: 0.0,
            max_tokens: 16,
        };
        let auth = HttpClient::bearer(&self.api_key);
        let body = self
            .http
            .post_json(&url, &[auth], &req)
            .await
            .with_context(|| format!("OpenRouter model probe failed for '{model}'"))?;

        let resp: serde_json::Value = serde_json::from_str(&body).context("parse OpenRouter probe response")?;
        if let Some(err) = resp.get("error") {
            let message = err
                .get("message")
                .and_then(|message| message.as_str())
                .unwrap_or("unknown OpenRouter probe error");
            bail!("OpenRouter model probe failed for '{}': {}", model, message);
        }

        Ok(())
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
            top_p: None,
            max_tokens: output_token_limit_env().map(|limit| limit.max(1) as u32),
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

#[derive(Debug, Clone)]
pub struct OpenRouterCreditStatus {
    pub key_limit: Option<f64>,
    pub key_limit_remaining: Option<f64>,
    pub account_remaining: Option<f64>,
    pub min_credits: f64,
    pub model_probe: Option<String>,
    pub unchecked_allowed: bool,
}

impl OpenRouterCreditStatus {
    pub fn summary(&self) -> String {
        let key_remaining = match (self.key_limit, self.key_limit_remaining) {
            (Some(limit), Some(remaining)) => format!("key remaining {remaining:.4}/{limit:.4}"),
            (Some(limit), None) => format!("key limit {limit:.4}, remaining unknown"),
            (None, Some(remaining)) => format!("key remaining {remaining:.4}"),
            (None, None) => "key has no configured limit".to_string(),
        };

        let credit_status = match self.account_remaining {
            Some(remaining) => {
                format!(
                    "{key_remaining}; account remaining {remaining:.4}; min {:.4}",
                    self.min_credits
                )
            }
            None => format!(
                "{key_remaining}; account balance not checked (set OPENROUTER_MANAGEMENT_API_KEY); min {:.4}",
                self.min_credits
            ),
        };

        if let Some(model) = &self.model_probe {
            format!("{credit_status}; model probe OK for '{model}'")
        } else if self.unchecked_allowed {
            format!("{credit_status}; unchecked credits allowed by OPENROUTER_ALLOW_UNCHECKED_CREDITS")
        } else {
            credit_status
        }
    }
}

#[derive(Debug, Deserialize)]
struct OpenRouterKeyResp {
    data: OpenRouterKeyInfo,
}

#[derive(Debug, Deserialize)]
struct OpenRouterKeyInfo {
    limit: Option<f64>,
    limit_remaining: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct OpenRouterCreditsResp {
    data: OpenRouterCreditsData,
}

#[derive(Debug, Deserialize)]
struct OpenRouterCreditsData {
    total_credits: f64,
    total_usage: f64,
}

#[derive(Debug, Clone)]
struct OpenRouterAccountCredits {
    remaining: f64,
}

fn min_credits_threshold() -> f64 {
    let openrouter = env::var("OPENROUTER_MIN_CREDITS").ok();
    let global = env::var("LLM_MIN_CREDITS").ok();
    parse_min_credits_threshold(openrouter.as_deref(), global.as_deref())
}

fn allow_unchecked_credits() -> bool {
    let value = env::var("OPENROUTER_ALLOW_UNCHECKED_CREDITS").ok();
    parse_env_flag(value.as_deref())
}

fn parse_min_credits_threshold(openrouter: Option<&str>, global: Option<&str>) -> f64 {
    [openrouter, global]
        .into_iter()
        .flatten()
        .find_map(|v| v.trim().parse::<f64>().ok())
        .unwrap_or(0.0)
}

fn parse_env_flag(value: Option<&str>) -> bool {
    value
        .map(|v| matches!(v.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "y"))
        .unwrap_or(false)
}

/// Context limits for models accessed via OpenRouter.
/// Uses the same limits as direct clients where known,
/// falls back to a conservative default.
pub fn openrouter_ctx_limit_tokens(model: &str) -> usize {
    let m = model.to_ascii_lowercase();

    // Anthropic
    if m.contains("claude") {
        if m.contains("4.6")
            || m.contains("4-6")
            || m.contains("4.7")
            || m.contains("4-7")
            || m.contains("4.8")
            || m.contains("4-8")
        {
            return 1_000_000;
        }
        return 185_000;
    }
    // OpenAI
    if m.contains("gpt-5.5") {
        return 1_050_000;
    }
    if m.contains("gpt-5") || m.contains("gpt-4.1") {
        return 400_000;
    }
    if m.contains("gpt-4o") || m.contains("gpt-4") {
        return 128_000;
    }
    // xAI / Grok
    if m.contains("grok-build-0.1") || m.contains("grok-code-fast") {
        return 200_000;
    }
    if m.contains("grok-4.3") {
        return 1_000_000;
    }
    if m.contains("grok-4") {
        return 200_000;
    }
    if m.contains("grok") {
        return 90_000;
    }
    // DeepSeek
    if m.contains("deepseek-v4") {
        return 1_000_000;
    }
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

#[cfg(test)]
mod tests {
    use super::{parse_env_flag, parse_min_credits_threshold};

    #[test]
    fn openrouter_min_credits_overrides_global_threshold() {
        assert_eq!(parse_min_credits_threshold(Some("2.5"), Some("1.0")), 2.5);
        assert_eq!(parse_min_credits_threshold(None, Some("1.0")), 1.0);
        assert_eq!(parse_min_credits_threshold(Some("not-a-number"), Some("1.0")), 1.0);
        assert_eq!(parse_min_credits_threshold(None, None), 0.0);
    }

    #[test]
    fn unchecked_credit_escape_hatch_accepts_common_true_values() {
        for value in ["1", "true", "TRUE", " yes ", "y"] {
            assert!(parse_env_flag(Some(value)));
        }
        for value in [None, Some(""), Some("0"), Some("false"), Some("no")] {
            assert!(!parse_env_flag(value));
        }
    }
}

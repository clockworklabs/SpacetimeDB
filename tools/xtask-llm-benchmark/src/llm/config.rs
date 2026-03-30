use anyhow::Result;
use std::{env, sync::Arc};

use crate::llm::clients::http::HttpClient;
use crate::llm::clients::{
    AnthropicClient, DeepSeekClient, GoogleGeminiClient, MetaLlamaClient, OpenAiClient, OpenRouterClient, XaiGrokClient,
};
use crate::llm::provider::{LlmProvider, RouterProvider};
use crate::llm::types::Vendor;

fn force_vendor_from_env() -> Option<Vendor> {
    match env::var("LLM_VENDOR").ok().as_deref() {
        Some("openai") => Some(Vendor::OpenAi),
        Some("anthropic") => Some(Vendor::Anthropic),
        Some("google") | Some("gemini") => Some(Vendor::Google),
        Some("xai") | Some("grok") => Some(Vendor::Xai),
        Some("deepseek") => Some(Vendor::DeepSeek),
        Some("meta") | Some("llama") => Some(Vendor::Meta),
        Some("openrouter") | Some("or") => Some(Vendor::OpenRouter),
        _ => None,
    }
}

/// Env vars:
/// - OPENROUTER_API_KEY                            (unified proxy — routes to any vendor)
/// - OPENAI_API_KEY,        OPENAI_BASE_URL        (default https://api.openai.com)
/// - ANTHROPIC_API_KEY,     ANTHROPIC_BASE_URL     (default https://api.anthropic.com)
/// - GOOGLE_API_KEY,        GOOGLE_BASE_URL        (default https://generativelanguage.googleapis.com)
/// - XAI_API_KEY,           XAI_BASE_URL           (default https://api.x.ai)
/// - DEEPSEEK_API_KEY,      DEEPSEEK_BASE_URL      (default https://api.deepseek.com)
/// - META_API_KEY,          META_BASE_URL          (no default)
/// - LLM_VENDOR: openai|anthropic|google|xai|deepseek|meta|openrouter
///
/// When OPENROUTER_API_KEY is set, it acts as a fallback for any vendor that doesn't
/// have its own direct API key configured. This means you can set just OPENROUTER_API_KEY
/// to run all models through OpenRouter, or mix direct keys with OpenRouter fallback.
pub fn make_provider_from_env() -> Result<Arc<dyn LlmProvider>> {
    let http = HttpClient::new()?;

    // Filter out empty strings so an empty env var falls through to OpenRouter.
    let non_empty = |k: &str| env::var(k).ok().filter(|v| !v.trim().is_empty());
    let openai_key = non_empty("OPENAI_API_KEY");
    let anth_key = non_empty("ANTHROPIC_API_KEY");
    let google_key = non_empty("GOOGLE_API_KEY");
    let xai_key = non_empty("XAI_API_KEY");
    let deep_key = non_empty("DEEPSEEK_API_KEY");
    let meta_key = non_empty("META_API_KEY");
    let openrouter_key = non_empty("OPENROUTER_API_KEY");

    // IMPORTANT: no trailing /v1 here; clients append their own versioned paths.
    let openai_base = env::var("OPENAI_BASE_URL").unwrap_or_else(|_| "https://api.openai.com".to_string());
    let anth_base = env::var("ANTHROPIC_BASE_URL").unwrap_or_else(|_| "https://api.anthropic.com".to_string());
    let google_base =
        env::var("GOOGLE_BASE_URL").unwrap_or_else(|_| "https://generativelanguage.googleapis.com".to_string());
    let xai_base = env::var("XAI_BASE_URL").unwrap_or_else(|_| "https://api.x.ai".to_string());
    let deep_base = env::var("DEEPSEEK_BASE_URL").unwrap_or_else(|_| "https://api.deepseek.com".to_string());
    let meta_base = env::var("META_BASE_URL").ok();

    let openai = openai_key
        .as_ref()
        .map(|k| OpenAiClient::new(openai_base.clone(), k.clone()));

    let anthropic = anth_key
        .as_ref()
        .map(|k| AnthropicClient::new(anth_base.clone(), k.clone()));

    let google = google_key
        .as_ref()
        .map(|k| GoogleGeminiClient::new(http.clone(), google_base.clone(), k.clone()));

    let xai = xai_key
        .as_ref()
        .map(|k| XaiGrokClient::new(http.clone(), xai_base.clone(), k.clone()));

    let deepseek = deep_key
        .as_ref()
        .map(|k| DeepSeekClient::new(http.clone(), deep_base.clone(), k.clone()));

    let meta = match (meta_key, meta_base) {
        (Some(k), Some(b)) => Some(MetaLlamaClient::new(http.clone(), b, k)),
        _ => None,
    };

    let openrouter = openrouter_key.map(|k| OpenRouterClient::new(http.clone(), k));

    let force = force_vendor_from_env();
    let router = RouterProvider::new(openai, anthropic, google, xai, deepseek, meta, openrouter, force);
    Ok(Arc::new(router))
}

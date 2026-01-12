use anyhow::Result;
use std::{env, sync::Arc};

use crate::llm::clients::http::HttpClient;
use crate::llm::clients::{
    AnthropicClient, DeepSeekClient, GoogleGeminiClient, MetaLlamaClient, OpenAiClient, XaiGrokClient,
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
        _ => None,
    }
}

/// Env vars:
/// - OPENAI_API_KEY,        OPENAI_BASE_URL        (default https://api.openai.com)
/// - ANTHROPIC_API_KEY,     ANTHROPIC_BASE_URL     (default https://api.anthropic.com)
/// - GOOGLE_API_KEY,        GOOGLE_BASE_URL        (default https://generativelanguage.googleapis.com)
/// - XAI_API_KEY,           XAI_BASE_URL           (default https://api.x.ai)
/// - DEEPSEEK_API_KEY,      DEEPSEEK_BASE_URL      (default https://api.deepseek.com)
/// - META_API_KEY,          META_BASE_URL          (no default)
/// - LLM_VENDOR: openai|anthropic|google|xai|deepseek|meta
pub fn make_provider_from_env() -> Result<Arc<dyn LlmProvider>> {
    let http = HttpClient::new()?;

    let openai_key = env::var("OPENAI_API_KEY").ok();
    let anth_key = env::var("ANTHROPIC_API_KEY").ok();
    let google_key = env::var("GOOGLE_API_KEY").ok();
    let xai_key = env::var("XAI_API_KEY").ok();
    let deep_key = env::var("DEEPSEEK_API_KEY").ok();
    let meta_key = env::var("META_API_KEY").ok();

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

    let force = force_vendor_from_env();
    let router = RouterProvider::new(openai, anthropic, google, xai, deepseek, meta, force);
    Ok(Arc::new(router))
}

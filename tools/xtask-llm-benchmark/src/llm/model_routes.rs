use crate::llm::types::Vendor;
use serde::{Deserialize, Serialize};
use std::sync::LazyLock;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelRoute {
    pub display_name: String,              // human-friendly label for reports
    pub vendor: Vendor,                    // which API family to use
    pub api_model: String,                 // model id expected by the vendor's direct API
    pub openrouter_model: Option<String>,  // OpenRouter model id (if different from api_model)
}

static DEFAULT_ROUTES: LazyLock<Vec<ModelRoute>> = LazyLock::new(|| {
    use Vendor::*;
    vec![
        // OpenAI: Best GPT-5.2-Codex, Cheaper GPT-5-mini
        ModelRoute::new("GPT-5.2-Codex", OpenAi, "gpt-5.2-codex", Some("openai/gpt-5.2-codex")),
        ModelRoute::new("GPT-5-mini", OpenAi, "gpt-5-mini", Some("openai/gpt-5-mini")),
        // Claude: Best Opus 4.6, Cheaper Sonnet 4.6
        // Direct API uses dashes (claude-opus-4-6); OpenRouter uses dots (claude-opus-4.6)
        ModelRoute::new("Claude Opus 4.6", Anthropic, "claude-opus-4-6", Some("anthropic/claude-opus-4.6")),
        ModelRoute::new("Claude Sonnet 4.6", Anthropic, "claude-sonnet-4-6", Some("anthropic/claude-sonnet-4.6")),
        // Grok: Best Grok 4, Cheaper Grok Code
        // grok-4 → x-ai/grok-4.20-beta on OpenRouter; grok-code-fast-1 not on OpenRouter → x-ai/grok-3
        ModelRoute::new("Grok 4", Xai, "grok-4", Some("x-ai/grok-4.20-beta")),
        ModelRoute::new("Grok Code", Xai, "grok-code-fast-1", Some("x-ai/grok-code-fast-1")),
        // Gemini: direct via GOOGLE_API_KEY, falls back to OpenRouter if not set
        ModelRoute::new("Gemini 3.1 Pro", Google, "gemini-3.1-pro-preview", Some("google/gemini-3.1-pro-preview")),
        ModelRoute::new("Gemini 3 Flash", Google, "gemini-3-flash-preview", Some("google/gemini-3-flash-preview")),
        // DeepSeek: Reasoner (thinking), Chat (general)
        // deepseek-reasoner is listed as deepseek-r1 on OpenRouter
        ModelRoute::new("DeepSeek Reasoner", DeepSeek, "deepseek-reasoner", Some("deepseek/deepseek-r1")),
        ModelRoute::new("DeepSeek Chat", DeepSeek, "deepseek-chat", Some("deepseek/deepseek-chat")),
    ]
});

impl ModelRoute {
    pub fn new(display_name: &str, vendor: Vendor, api_model: &str, openrouter_model: Option<&str>) -> Self {
        Self {
            display_name: display_name.to_string(),
            vendor,
            api_model: api_model.to_string(),
            openrouter_model: openrouter_model.map(|s| s.to_string()),
        }
    }
}

pub fn default_model_routes() -> &'static [ModelRoute] {
    &DEFAULT_ROUTES
}

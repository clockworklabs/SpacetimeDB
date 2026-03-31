use crate::llm::types::Vendor;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelRoute {
    pub display_name: &'static str,              // human-friendly label for reports
    pub vendor: Vendor,                          // which API family to use
    pub api_model: &'static str,                 // model id expected by the vendor's direct API
    pub openrouter_model: Option<&'static str>,  // OpenRouter model id (if different from api_model)
}

pub fn default_model_routes() -> &'static [ModelRoute] {
    use Vendor::*;
    &[
        // OpenAI: Best GPT-5.2-Codex, Cheaper GPT-5-mini
        ModelRoute {
            display_name: "GPT-5.2-Codex",
            vendor: OpenAi,
            api_model: "gpt-5.2-codex",
            openrouter_model: Some("openai/gpt-5.2-codex"),
        },
        ModelRoute {
            display_name: "GPT-5-mini",
            vendor: OpenAi,
            api_model: "gpt-5-mini",
            openrouter_model: Some("openai/gpt-5-mini"),
        },
        // Claude: Best Opus 4.6, Cheaper Sonnet 4.6
        // Direct API uses dashes (claude-opus-4-6); OpenRouter uses dots (claude-opus-4.6)
        ModelRoute {
            display_name: "Claude Opus 4.6",
            vendor: Anthropic,
            api_model: "claude-opus-4-6",
            openrouter_model: Some("anthropic/claude-opus-4.6"),
        },
        ModelRoute {
            display_name: "Claude Sonnet 4.6",
            vendor: Anthropic,
            api_model: "claude-sonnet-4-6",
            openrouter_model: Some("anthropic/claude-sonnet-4.6"),
        },
        // Grok: Best Grok 4, Cheaper Grok Code
        // grok-4 → x-ai/grok-4.20-beta on OpenRouter; grok-code-fast-1 not on OpenRouter → x-ai/grok-3
        ModelRoute {
            display_name: "Grok 4",
            vendor: Xai,
            api_model: "grok-4",
            openrouter_model: Some("x-ai/grok-4.20-beta"),
        },
        ModelRoute {
            display_name: "Grok Code",
            vendor: Xai,
            api_model: "grok-code-fast-1",
            openrouter_model: Some("x-ai/grok-code-fast-1"),
        },
        // Gemini: direct via GOOGLE_API_KEY, falls back to OpenRouter if not set
        ModelRoute {
            display_name: "Gemini 3.1 Pro",
            vendor: Google,
            api_model: "gemini-3.1-pro-preview",
            openrouter_model: Some("google/gemini-3.1-pro-preview"),
        },
        ModelRoute {
            display_name: "Gemini 3 Flash",
            vendor: Google,
            api_model: "gemini-3-flash-preview",
            openrouter_model: Some("google/gemini-3-flash-preview"),
        },
        // DeepSeek: Reasoner (thinking), Chat (general)
        // deepseek-reasoner is listed as deepseek-r1 on OpenRouter
        ModelRoute {
            display_name: "DeepSeek Reasoner",
            vendor: DeepSeek,
            api_model: "deepseek-reasoner",
            openrouter_model: Some("deepseek/deepseek-r1"),
        },
        ModelRoute {
            display_name: "DeepSeek Chat",
            vendor: DeepSeek,
            api_model: "deepseek-chat",
            openrouter_model: Some("deepseek/deepseek-chat"),
        },
    ]
}

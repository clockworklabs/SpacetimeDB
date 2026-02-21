use crate::llm::types::Vendor;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelRoute {
    pub display_name: &'static str, // human-friendly label for reports
    pub vendor: Vendor,             // which API family to use
    pub api_model: &'static str,    // model id expected by the vendor API
}

pub fn default_model_routes() -> &'static [ModelRoute] {
    use Vendor::*;
    &[
        // OpenAI: Best GPT-5.2-Codex, Cheaper GPT-5-mini
        ModelRoute {
            display_name: "GPT-5.2-Codex",
            vendor: OpenAi,
            api_model: "gpt-5.2-codex",
        },
        ModelRoute {
            display_name: "GPT-5-mini",
            vendor: OpenAi,
            api_model: "gpt-5-mini",
        },
        // Claude: Best Opus 4.6, Cheaper Sonnet 4.6
        ModelRoute {
            display_name: "Claude Opus 4.6",
            vendor: Anthropic,
            api_model: "claude-opus-4-6",
        },
        ModelRoute {
            display_name: "Claude Sonnet 4.6",
            vendor: Anthropic,
            api_model: "claude-sonnet-4-6",
        },
        // Grok: Best Grok 4, Cheaper Grok Code
        ModelRoute {
            display_name: "Grok 4",
            vendor: Xai,
            api_model: "grok-4",
        },
        ModelRoute {
            display_name: "Grok Code",
            vendor: Xai,
            api_model: "grok-code-fast-1",
        },
        // Gemini: Best 3.1 Pro, Cheaper 3 Flash
        ModelRoute {
            display_name: "Gemini 3.1 Pro",
            vendor: Meta,
            api_model: "gemini-3.1-pro-preview",
        },
        ModelRoute {
            display_name: "Gemini 3 Flash",
            vendor: Meta,
            api_model: "gemini-3-flash-preview",
        },
        // DeepSeek: Reasoner (thinking), Chat (general)
        ModelRoute {
            display_name: "DeepSeek Reasoner",
            vendor: DeepSeek,
            api_model: "deepseek-reasoner",
        },
        ModelRoute {
            display_name: "DeepSeek Chat",
            vendor: DeepSeek,
            api_model: "deepseek-chat",
        },
    ]
}

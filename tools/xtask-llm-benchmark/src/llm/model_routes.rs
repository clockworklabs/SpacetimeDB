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
        //GPT
        ModelRoute {
            display_name: "GPT-5",
            vendor: OpenAi,
            api_model: "gpt-5",
        },
        ModelRoute {
            display_name: "GPT-4.1",
            vendor: OpenAi,
            api_model: "gpt-4.1",
        },
        ModelRoute {
            display_name: "o4-mini",
            vendor: OpenAi,
            api_model: "o4-mini",
        },
        ModelRoute {
            display_name: "GPT-4o",
            vendor: OpenAi,
            api_model: "gpt-4o",
        },
        // CLAUDE (Anthropic)
        ModelRoute {
            display_name: "Claude 4.5 Sonnet",
            vendor: Anthropic,
            api_model: "claude-sonnet-4-5",
        },
        ModelRoute {
            display_name: "Claude 4 Sonnet",
            vendor: Anthropic,
            api_model: "claude-sonnet-4",
        },
        ModelRoute {
            display_name: "Claude 4.5 Haiku",
            vendor: Anthropic,
            api_model: "claude-haiku-4-5",
        },
        //GROK
        ModelRoute {
            display_name: "Grok 4",
            vendor: Xai,
            api_model: "grok-4",
        },
        ModelRoute {
            display_name: "Grok 3 Mini (Beta)",
            vendor: Xai,
            api_model: "grok-3-mini",
        },
        //GEMINI
        ModelRoute {
            display_name: "Gemini 2.5 Pro",
            vendor: Google,
            api_model: "gemini-2.5-pro",
        },
        ModelRoute {
            display_name: "Gemini 2.5 Flash",
            vendor: Google,
            api_model: "gemini-2.5-flash",
        },
        //DEEPSPEEK
        ModelRoute {
            display_name: "DeepSeek V3",
            vendor: DeepSeek,
            api_model: "deepseek-chat",
        },
        ModelRoute {
            display_name: "DeepSeek R1",
            vendor: DeepSeek,
            api_model: "deepseek-reasoner",
        },
        //META
        ModelRoute {
            display_name: "Meta Llama 3.1 405B",
            vendor: Meta,
            api_model: "meta-llama/llama-3.1-405b-instruct",
        },
    ]
}

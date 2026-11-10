use crate::llm::types::Vendor;
use serde::{Deserialize, Serialize};
use std::sync::LazyLock;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelRoute {
    pub display_name: String,             // human-friendly label for reports
    pub vendor: Vendor,                   // which API family to use
    pub api_model: String,                // model id expected by the vendor's direct API
    pub openrouter_model: Option<String>, // OpenRouter model id (if different from api_model)
}

static DEFAULT_ROUTES: LazyLock<Vec<ModelRoute>> = LazyLock::new(|| {
    use Vendor::*;
    vec![
        // OpenAI: Best GPT-5.5, Cheaper GPT-5.4-mini
        ModelRoute::new("GPT-5.5", OpenAi, "gpt-5.5", Some("openai/gpt-5.5")),
        ModelRoute::new("GPT-5.4-mini", OpenAi, "gpt-5.4-mini", Some("openai/gpt-5.4-mini")),
        // Claude: Best Opus 4.8, Cheaper Sonnet 4.6
        // Direct API uses dashes (claude-opus-4-8); OpenRouter uses dots (claude-opus-4.8)
        ModelRoute::new(
            "Claude Opus 4.8",
            Anthropic,
            "claude-opus-4-8",
            Some("anthropic/claude-opus-4.8"),
        ),
        ModelRoute::new(
            "Claude Sonnet 4.6",
            Anthropic,
            "claude-sonnet-4-6",
            Some("anthropic/claude-sonnet-4.6"),
        ),
        // Grok: Best Grok 4.3, coding-specialized Grok Build
        ModelRoute::new("Grok 4.3", Xai, "grok-4.3", Some("x-ai/grok-4.3")),
        ModelRoute::new("Grok Build 0.1", Xai, "grok-build-0.1", Some("x-ai/grok-build-0.1")),
        // Gemini: direct via GOOGLE_API_KEY, falls back to OpenRouter if not set
        ModelRoute::new(
            "Gemini 3.1 Pro",
            Google,
            "gemini-3.1-pro-preview",
            Some("google/gemini-3.1-pro-preview"),
        ),
        ModelRoute::new(
            "Gemini 3.5 Flash",
            Google,
            "gemini-3.5-flash",
            Some("google/gemini-3.5-flash"),
        ),
        // DeepSeek: Pro (highest capability), Flash (cheaper/faster)
        ModelRoute::new(
            "DeepSeek V4 Pro",
            DeepSeek,
            "deepseek-v4-pro",
            Some("deepseek/deepseek-v4-pro"),
        ),
        ModelRoute::new(
            "DeepSeek V4 Flash",
            DeepSeek,
            "deepseek-v4-flash",
            Some("deepseek/deepseek-v4-flash"),
        ),
    ]
});

impl ModelRoute {
    /// Stable result key shared by direct and OpenRouter routes, independent of display labels.
    pub fn model_id(&self) -> String {
        if let Some(id) = &self.openrouter_model {
            return id.clone();
        }
        if self.api_model.contains('/') {
            return self.api_model.clone();
        }
        let prefix = match self.vendor {
            Vendor::Xai => "x-ai",
            Vendor::Meta | Vendor::OpenRouter => return self.api_model.clone(),
            vendor => vendor.slug(),
        };
        format!("{prefix}/{}", self.api_model)
    }

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_identity_is_independent_of_labels_and_route_source() {
        let mut routes = vec![
            ModelRoute::new("GPT-5.5", Vendor::OpenAi, "gpt-5.5", Some("openai/gpt-5.5")),
            ModelRoute::new("openai/gpt-5.5", Vendor::OpenRouter, "openai/gpt-5.5", None),
            ModelRoute::new("OpenAI: GPT-5.5", Vendor::OpenRouter, "openai/gpt-5.5", None),
            ModelRoute::new("Direct model", Vendor::OpenAi, "gpt-5.5", None),
        ];
        for route in &mut routes {
            assert_eq!(route.model_id(), "openai/gpt-5.5");
            route.display_name = "Changed label".into();
            assert_eq!(route.model_id(), "openai/gpt-5.5");
        }
        assert_ne!(
            ModelRoute::new("Same label", Vendor::OpenAi, "test", None).model_id(),
            ModelRoute::new("Same label", Vendor::Anthropic, "test", None).model_id(),
        );
        assert_eq!(
            ModelRoute::new("Grok", Vendor::Xai, "grok-test", None).model_id(),
            "x-ai/grok-test"
        );
        assert_eq!(
            ModelRoute::new("Llama", Vendor::Meta, "meta-llama/test", None).model_id(),
            "meta-llama/test"
        );
    }
}

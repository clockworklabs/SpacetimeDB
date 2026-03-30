use anyhow::{Context, Result};
use async_trait::async_trait;

use crate::llm::clients::{
    AnthropicClient, DeepSeekClient, GoogleGeminiClient, MetaLlamaClient, OpenAiClient, OpenRouterClient, XaiGrokClient,
};
use crate::llm::model_routes::ModelRoute;
use crate::llm::prompt::BuiltPrompt;
use crate::llm::types::{LlmOutput, Vendor};

#[async_trait]
pub trait LlmProvider: Send + Sync {
    async fn generate(&self, route: &ModelRoute, prompt: &BuiltPrompt) -> Result<LlmOutput>;
}

pub struct RouterProvider {
    pub openai: Option<OpenAiClient>,
    pub anthropic: Option<AnthropicClient>,
    pub google: Option<GoogleGeminiClient>,
    pub xai: Option<XaiGrokClient>,
    pub deepseek: Option<DeepSeekClient>,
    pub meta: Option<MetaLlamaClient>,
    /// OpenRouter client used as a unified fallback when a direct vendor client
    /// is not configured. Set via `OPENROUTER_API_KEY`.
    pub openrouter: Option<OpenRouterClient>,
    pub force: Option<Vendor>,
}

impl RouterProvider {
    pub fn new(
        openai: Option<OpenAiClient>,
        anthropic: Option<AnthropicClient>,
        google: Option<GoogleGeminiClient>,
        xai: Option<XaiGrokClient>,
        deepseek: Option<DeepSeekClient>,
        meta: Option<MetaLlamaClient>,
        openrouter: Option<OpenRouterClient>,
        force: Option<Vendor>,
    ) -> Self {
        Self {
            openai,
            anthropic,
            google,
            xai,
            deepseek,
            meta,
            openrouter,
            force,
        }
    }
}

#[async_trait]
impl LlmProvider for RouterProvider {
    async fn generate(&self, route: &ModelRoute, prompt: &BuiltPrompt) -> Result<LlmOutput> {
        // Web search mode: route all models through OpenRouter with :online suffix.
        // OpenRouter's :online feature adds Bing-powered web search to any model.
        if prompt.search_enabled {
            let cli = self.openrouter.as_ref().context(
                "Search mode requires OPENROUTER_API_KEY — OpenRouter provides unified web search via :online models",
            )?;
            let base_model = route
                .openrouter_model
                .map(|s| s.to_string())
                .unwrap_or_else(|| openrouter_model_id(route.vendor, route.api_model));
            let online_model = format!("{base_model}:online");
            eprintln!(
                "[search] {} → OpenRouter :online model '{}'",
                route.display_name, online_model
            );
            return cli.generate(&online_model, prompt).await;
        }

        let vendor = self.force.unwrap_or(route.vendor);

        // If vendor is explicitly OpenRouter, or if the direct client isn't configured
        // but OpenRouter is available, route through OpenRouter.
        if vendor == Vendor::OpenRouter {
            let cli = self
                .openrouter
                .as_ref()
                .context("OpenRouter client not configured (set OPENROUTER_API_KEY)")?;
            let model = route
                .openrouter_model
                .unwrap_or(route.api_model);
            return cli.generate(model, prompt).await;
        }

        // Try direct client first, fall back to OpenRouter if available.
        match vendor {
            Vendor::OpenAi => match self.openai.as_ref() {
                Some(cli) => cli.generate(route.api_model, prompt).await,
                None => self.fallback_openrouter(route, prompt, "OpenAI").await,
            },
            Vendor::Anthropic => match self.anthropic.as_ref() {
                Some(cli) => cli.generate(route.api_model, prompt).await,
                None => self.fallback_openrouter(route, prompt, "Anthropic").await,
            },
            Vendor::Google => match self.google.as_ref() {
                Some(cli) => cli.generate(route.api_model, prompt).await,
                None => self.fallback_openrouter(route, prompt, "Google").await,
            },
            Vendor::Xai => match self.xai.as_ref() {
                Some(cli) => cli.generate(route.api_model, prompt).await,
                None => self.fallback_openrouter(route, prompt, "xAI").await,
            },
            Vendor::DeepSeek => match self.deepseek.as_ref() {
                Some(cli) => cli.generate(route.api_model, prompt).await,
                None => self.fallback_openrouter(route, prompt, "DeepSeek").await,
            },
            Vendor::Meta => match self.meta.as_ref() {
                Some(cli) => cli.generate(route.api_model, prompt).await,
                None => self.fallback_openrouter(route, prompt, "Meta").await,
            },
            Vendor::OpenRouter => unreachable!("handled above"),
        }
    }
}

impl RouterProvider {
    /// Fall back to the OpenRouter client when a direct vendor client is not configured.
    async fn fallback_openrouter(&self, route: &ModelRoute, prompt: &BuiltPrompt, vendor_name: &str) -> Result<LlmOutput> {
        match self.openrouter.as_ref() {
            Some(cli) => {
                let or_model = route
                    .openrouter_model
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| openrouter_model_id(route.vendor, route.api_model));
                eprintln!(
                    "[openrouter] {} client not configured, falling back to OpenRouter for model '{}'",
                    vendor_name, or_model
                );
                cli.generate(&or_model, prompt).await
            }
            None => anyhow::bail!(
                "{} client not configured and no OpenRouter fallback available. \
                 Set {}_API_KEY or OPENROUTER_API_KEY.",
                vendor_name,
                vendor_name.to_ascii_uppercase()
            ),
        }
    }
}

/// Map a vendor + bare model id to the `vendor/model` namespace that OpenRouter requires.
/// If the model id already contains `/` it is returned as-is (e.g. `google/gemini-3.1-pro-preview`).
fn openrouter_model_id(vendor: Vendor, api_model: &str) -> String {
    if api_model.contains('/') {
        return api_model.to_string();
    }
    let prefix = match vendor {
        Vendor::Anthropic => "anthropic",
        Vendor::OpenAi => "openai",
        Vendor::Xai => "x-ai",
        Vendor::DeepSeek => "deepseek",
        Vendor::Google => "google",
        // Meta rows already carry a full `vendor/model` id (caught by the `/` check above).
        Vendor::Meta | Vendor::OpenRouter => return api_model.to_string(),
    };
    format!("{}/{}", prefix, api_model)
}

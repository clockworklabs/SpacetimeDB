use anyhow::{Context, Result};
use async_trait::async_trait;

use crate::llm::clients::{
    AnthropicClient, DeepSeekClient, GoogleGeminiClient, MetaLlamaClient, OpenAiClient, OpenRouterClient,
    XaiGrokClient,
};
use crate::llm::model_routes::ModelRoute;
use crate::llm::prompt::BuiltPrompt;
use crate::llm::types::Vendor;

#[async_trait]
pub trait LlmProvider: Send + Sync {
    async fn generate(&self, route: &ModelRoute, prompt: &BuiltPrompt) -> Result<String>;
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
    async fn generate(&self, route: &ModelRoute, prompt: &BuiltPrompt) -> Result<String> {
        let vendor = self.force.unwrap_or(route.vendor);

        // If vendor is explicitly OpenRouter, or if the direct client isn't configured
        // but OpenRouter is available, route through OpenRouter.
        if vendor == Vendor::OpenRouter {
            let cli = self
                .openrouter
                .as_ref()
                .context("OpenRouter client not configured (set OPENROUTER_API_KEY)")?;
            return cli.generate(route.api_model, prompt).await;
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
    async fn fallback_openrouter(
        &self,
        route: &ModelRoute,
        prompt: &BuiltPrompt,
        vendor_name: &str,
    ) -> Result<String> {
        match self.openrouter.as_ref() {
            Some(cli) => {
                eprintln!(
                    "[openrouter] {} client not configured, falling back to OpenRouter for model '{}'",
                    vendor_name, route.api_model
                );
                cli.generate(route.api_model, prompt).await
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

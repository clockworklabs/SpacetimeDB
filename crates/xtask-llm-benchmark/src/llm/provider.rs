use anyhow::{Context, Result};
use async_trait::async_trait;

use crate::llm::clients::{
    AnthropicClient, DeepSeekClient, GoogleGeminiClient, MetaLlamaClient, OpenAiClient, XaiGrokClient,
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
        force: Option<Vendor>,
    ) -> Self {
        Self {
            openai,
            anthropic,
            google,
            xai,
            deepseek,
            meta,
            force,
        }
    }
}

#[async_trait]
impl LlmProvider for RouterProvider {
    async fn generate(&self, route: &ModelRoute, prompt: &BuiltPrompt) -> Result<String> {
        let vendor = self.force.unwrap_or(route.vendor);
        match vendor {
            Vendor::OpenAi => {
                let cli = self.openai.as_ref().context("OpenAI client not configured")?;
                cli.generate(route.api_model, prompt).await
            }
            Vendor::Anthropic => {
                let cli = self.anthropic.as_ref().context("Anthropic client not configured")?;
                cli.generate(route.api_model, prompt).await
            }
            Vendor::Google => {
                let cli = self.google.as_ref().context("Google client not configured")?;
                cli.generate(route.api_model, prompt).await
            }
            Vendor::Xai => {
                let cli = self.xai.as_ref().context("xAI client not configured")?;
                cli.generate(route.api_model, prompt).await
            }
            Vendor::DeepSeek => {
                let cli = self.deepseek.as_ref().context("DeepSeek client not configured")?;
                cli.generate(route.api_model, prompt).await
            }
            Vendor::Meta => {
                let c = self
                    .meta
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("Meta Llama not configured"))?;
                c.generate(&route.api_model, prompt).await
            }
        }
    }
}

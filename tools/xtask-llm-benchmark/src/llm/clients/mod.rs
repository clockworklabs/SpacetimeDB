pub mod anthropic;
pub mod deepseek;
pub mod google;
pub(crate) mod http;
pub mod meta;
pub(crate) mod oa_compat;
pub mod openai;
pub mod openrouter;
pub mod xai;

use anyhow::{bail, Result};
use async_trait::async_trait;

pub use anthropic::AnthropicClient;
pub use deepseek::DeepSeekClient;
pub use google::GoogleGeminiClient;
pub use meta::MetaLlamaClient;
pub use openai::OpenAiClient;
pub use openrouter::OpenRouterClient;
pub use xai::XaiGrokClient;

use crate::llm::prompt::BuiltPrompt;
use crate::llm::types::LlmOutput;

#[derive(Debug, Clone)]
pub struct ClientPreflight {
    summary: String,
}

impl ClientPreflight {
    pub fn new(summary: impl Into<String>) -> Self {
        Self {
            summary: summary.into(),
        }
    }

    pub fn summary(&self) -> &str {
        &self.summary
    }
}

#[async_trait]
pub trait LlmClient: Send + Sync {
    fn provider_name(&self) -> &'static str;

    async fn preflight(&self, model: &str) -> Result<ClientPreflight> {
        bail!(
            "{} credit preflight is not implemented for model '{}'",
            self.provider_name(),
            model
        )
    }

    async fn generate(&self, model: &str, prompt: &BuiltPrompt) -> Result<LlmOutput>;
}

macro_rules! impl_direct_llm_client {
    ($ty:ty, $provider_name:literal) => {
        #[async_trait]
        impl LlmClient for $ty {
            fn provider_name(&self) -> &'static str {
                $provider_name
            }

            async fn generate(&self, model: &str, prompt: &BuiltPrompt) -> Result<LlmOutput> {
                <$ty>::generate(self, model, prompt).await
            }
        }
    };
}

impl_direct_llm_client!(OpenAiClient, "OpenAI");
impl_direct_llm_client!(AnthropicClient, "Anthropic");
impl_direct_llm_client!(GoogleGeminiClient, "Google");
impl_direct_llm_client!(XaiGrokClient, "xAI");
impl_direct_llm_client!(DeepSeekClient, "DeepSeek");
impl_direct_llm_client!(MetaLlamaClient, "Meta");

#[async_trait]
impl LlmClient for OpenRouterClient {
    fn provider_name(&self) -> &'static str {
        "OpenRouter"
    }

    async fn preflight(&self, model: &str) -> Result<ClientPreflight> {
        let status = self.preflight_credits(model).await?;
        Ok(ClientPreflight::new(status.summary()))
    }

    async fn generate(&self, model: &str, prompt: &BuiltPrompt) -> Result<LlmOutput> {
        OpenRouterClient::generate(self, model, prompt).await
    }
}

use anyhow::{Context, Result};
use async_trait::async_trait;
use std::collections::HashMap;

use crate::llm::clients::{
    AnthropicClient, DeepSeekClient, GoogleGeminiClient, LlmClient, MetaLlamaClient, OpenAiClient, OpenRouterClient,
    XaiGrokClient,
};
use crate::llm::model_routes::ModelRoute;
use crate::llm::prompt::BuiltPrompt;
use crate::llm::types::{LlmOutput, Vendor};

#[async_trait]
pub trait LlmProvider: Send + Sync {
    async fn preflight_route(&self, route: &ModelRoute, search_enabled: bool) -> Result<()>;
    async fn generate(&self, route: &ModelRoute, prompt: &BuiltPrompt) -> Result<LlmOutput>;
}

pub struct RouterProvider {
    clients: HashMap<Vendor, Box<dyn LlmClient>>,
    pub force: Option<Vendor>,
}

impl RouterProvider {
    #[allow(clippy::too_many_arguments)]
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
        let mut clients: HashMap<Vendor, Box<dyn LlmClient>> = HashMap::new();

        if let Some(client) = openai {
            clients.insert(Vendor::OpenAi, Box::new(client));
        }
        if let Some(client) = anthropic {
            clients.insert(Vendor::Anthropic, Box::new(client));
        }
        if let Some(client) = google {
            clients.insert(Vendor::Google, Box::new(client));
        }
        if let Some(client) = xai {
            clients.insert(Vendor::Xai, Box::new(client));
        }
        if let Some(client) = deepseek {
            clients.insert(Vendor::DeepSeek, Box::new(client));
        }
        if let Some(client) = meta {
            clients.insert(Vendor::Meta, Box::new(client));
        }
        if let Some(client) = openrouter {
            clients.insert(Vendor::OpenRouter, Box::new(client));
        }

        Self { clients, force }
    }
}

struct ResolvedClient<'a> {
    client: &'a dyn LlmClient,
    endpoint_name: &'static str,
    model: String,
    fallback_from: Option<&'static str>,
    search_enabled: bool,
}

#[async_trait]
impl LlmProvider for RouterProvider {
    async fn preflight_route(&self, route: &ModelRoute, search_enabled: bool) -> Result<()> {
        let resolved = self.resolve_client(route, search_enabled)?;
        let status = resolved.client.preflight(&resolved.model).await.with_context(|| {
            format!(
                "{} credit preflight failed for model '{}'",
                resolved.endpoint_name, resolved.model
            )
        })?;

        eprintln!(
            "[preflight] {} -> {} '{}' OK ({})",
            route.display_name,
            resolved.endpoint_name,
            resolved.model,
            status.summary()
        );
        Ok(())
    }

    async fn generate(&self, route: &ModelRoute, prompt: &BuiltPrompt) -> Result<LlmOutput> {
        let resolved = self.resolve_client(route, prompt.search_enabled)?;

        if resolved.search_enabled {
            eprintln!(
                "[search] {} -> OpenRouter :online model '{}'",
                route.display_name, resolved.model
            );
        } else if let Some(vendor_name) = resolved.fallback_from {
            eprintln!(
                "[openrouter] {} client not configured, falling back to OpenRouter for model '{}'",
                vendor_name, resolved.model
            );
        }

        resolved.client.generate(&resolved.model, prompt).await
    }
}

impl RouterProvider {
    fn resolve_client<'a>(&'a self, route: &ModelRoute, search_enabled: bool) -> Result<ResolvedClient<'a>> {
        if search_enabled {
            let base_model = route
                .openrouter_model
                .clone()
                .unwrap_or_else(|| openrouter_model_id(route.vendor, &route.api_model));
            return self.resolve_openrouter(format!("{base_model}:online"), None, true);
        }

        let vendor = self.force.unwrap_or(route.vendor);

        if vendor == Vendor::OpenRouter {
            let model = route.openrouter_model.as_deref().unwrap_or(&route.api_model);
            return self.resolve_openrouter(model.to_string(), None, false);
        }

        let direct = self.clients.get(&vendor).map(|client| client.as_ref());
        self.resolve_direct_or_openrouter(direct, route, vendor)
    }

    fn resolve_direct_or_openrouter<'a>(
        &'a self,
        direct: Option<&'a dyn LlmClient>,
        route: &ModelRoute,
        vendor: Vendor,
    ) -> Result<ResolvedClient<'a>> {
        if let Some(client) = direct {
            return Ok(ResolvedClient {
                client,
                endpoint_name: vendor.display_name(),
                model: route.api_model.clone(),
                fallback_from: None,
                search_enabled: false,
            });
        }

        let model = route
            .openrouter_model
            .clone()
            .unwrap_or_else(|| openrouter_model_id(route.vendor, &route.api_model));
        self.resolve_openrouter(model, Some(vendor.display_name()), false)
    }

    fn resolve_openrouter<'a>(
        &'a self,
        model: String,
        fallback_from: Option<&'static str>,
        search_enabled: bool,
    ) -> Result<ResolvedClient<'a>> {
        let client = self
            .clients
            .get(&Vendor::OpenRouter)
            .map(|client| client.as_ref())
            .context("OpenRouter client not configured (set OPENROUTER_API_KEY)")?;

        Ok(ResolvedClient {
            client,
            endpoint_name: "OpenRouter",
            model,
            fallback_from,
            search_enabled,
        })
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

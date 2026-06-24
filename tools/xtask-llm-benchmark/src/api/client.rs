use anyhow::{anyhow, Context, Result};
use serde_json::json;

use crate::bench::normalize::{canonical_mode, normalize_model_names};
use crate::bench::types::{Results, RunOutcome};
use crate::llm::types::Vendor;
use crate::llm::ModelRoute;

#[derive(Debug)]
struct RemoteModelRouteRow {
    display_name: String,
    vendor: String,
    api_model: String,
    openrouter_model: Option<String>,
    active: Option<bool>,
    available: Option<bool>,
}

fn read_string_field(row: &serde_json::Map<String, serde_json::Value>, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|key| row.get(*key).and_then(|value| value.as_str()))
        .map(str::to_string)
}

fn read_bool_field(row: &serde_json::Map<String, serde_json::Value>, keys: &[&str]) -> Option<bool> {
    keys.iter()
        .find_map(|key| row.get(*key).and_then(|value| value.as_bool()))
}

fn parse_model_route_value(value: serde_json::Value) -> Result<RemoteModelRouteRow> {
    let row = value
        .as_object()
        .ok_or_else(|| anyhow!("remote model row must be an object"))?;

    Ok(RemoteModelRouteRow {
        display_name: read_string_field(row, &["display_name", "displayName", "name"]).unwrap_or_default(),
        vendor: read_string_field(row, &["vendor"]).unwrap_or_default(),
        api_model: read_string_field(row, &["api_model", "apiModel"]).unwrap_or_default(),
        openrouter_model: read_string_field(row, &["openrouter_model", "openrouterModel"]),
        active: read_bool_field(row, &["active"]),
        available: read_bool_field(row, &["available"]),
    })
}

fn parse_model_route_row(row: RemoteModelRouteRow) -> Result<Option<ModelRoute>> {
    if row.active == Some(false) || row.available == Some(false) {
        return Ok(None);
    }

    let vendor = Vendor::parse(&row.vendor).ok_or_else(|| anyhow!("unknown model vendor '{}'", row.vendor))?;
    let display_name = row.display_name.trim();
    let api_model = row.api_model.trim();

    if display_name.is_empty() {
        anyhow::bail!("remote model row is missing display_name");
    }
    if api_model.is_empty() {
        anyhow::bail!("remote model row '{}' is missing api_model", display_name);
    }

    Ok(Some(ModelRoute::new(
        display_name,
        vendor,
        api_model,
        row.openrouter_model.as_deref().filter(|s| !s.trim().is_empty()),
    )))
}

pub fn parse_model_routes_response(body: &serde_json::Value) -> Result<Vec<ModelRoute>> {
    let models = body.get("models").unwrap_or(body);
    let rows: Vec<serde_json::Value> =
        serde_json::from_value(models.clone()).context("parse llm benchmark model rows")?;

    let mut routes = Vec::new();
    for row in rows.into_iter().map(parse_model_route_value) {
        let row = row?;
        if let Some(route) = parse_model_route_row(row)? {
            routes.push(route);
        }
    }

    if routes.is_empty() {
        anyhow::bail!("no active available LLM benchmark models returned by website");
    }

    Ok(routes)
}

/// HTTP client for the SpacetimeDB LLM benchmark API (spacetime-web Postgres).
///
/// Supports endpoints owned by spacetime-web:
/// - `POST /api/llm-benchmark-upload` - upload benchmark results
/// - `POST /api/llm-benchmark-tasks` - upload task catalog
/// - `GET /api/llm-benchmark-models?active=true` - fetch active benchmark models
#[derive(Clone)]
pub struct ApiClient {
    base_url: String,
    api_key: String,
}

impl ApiClient {
    pub fn new(base_url: &str, api_key: &str) -> Result<Self> {
        Ok(Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            api_key: api_key.to_string(),
        })
    }

    fn client(&self) -> Result<reqwest::blocking::Client> {
        reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .context("failed to build HTTP client")
    }

    /// Build from environment variables `LLM_BENCHMARK_UPLOAD_URL` and `LLM_BENCHMARK_API_KEY`.
    /// Returns `None` if `LLM_BENCHMARK_UPLOAD_URL` is not set.
    pub fn from_env() -> Result<Option<Self>> {
        let url = match std::env::var("LLM_BENCHMARK_UPLOAD_URL") {
            Ok(u) if !u.is_empty() => u,
            _ => return Ok(None),
        };
        let key =
            std::env::var("LLM_BENCHMARK_API_KEY").context("LLM_BENCHMARK_API_KEY required when UPLOAD_URL is set")?;
        Self::new(&url, &key).map(Some)
    }

    /// Upload a batch of run outcomes for a single (lang, mode) combination.
    /// Normalizes model names and sanitizes volatile fields before upload.
    /// If `analysis` is provided, it is stored in the `llm_benchmark_analysis` table.
    pub fn upload_batch(&self, mode: &str, outcomes: &[RunOutcome], analysis: Option<&str>) -> Result<usize> {
        if outcomes.is_empty() {
            return Ok(0);
        }

        let mode = canonical_mode(mode);

        // Build in-memory Results so we can normalize model names
        let mut results = Results::default();
        {
            use crate::bench::normalize::{canonical_model_name, ensure_lang, ensure_mode, ensure_model};

            for r in outcomes {
                let lang_v = ensure_lang(&mut results, &r.lang);
                let mode_v = ensure_mode(lang_v, mode, Some(r.hash.clone()));
                let canonical_name = canonical_model_name(&r.model_name);
                let model_v = ensure_model(mode_v, &canonical_name);
                model_v.route_api_model = r.route_api_model.clone();

                let mut sanitized = r.clone();
                sanitized.sanitize_for_commit();
                model_v.tasks.insert(r.task.clone(), sanitized);
            }
        }
        normalize_model_names(&mut results);

        let url = format!("{}/api/llm-benchmark-upload", self.base_url);
        let client = self.client()?;
        let mut total_uploaded = 0usize;

        for lang_entry in &results.languages {
            for mode_entry in &lang_entry.modes {
                // Serialize models and inject analysis into each model object if provided
                let mut models_json = serde_json::to_value(&mode_entry.models)?;
                if let Some(text) = analysis
                    && let Some(arr) = models_json.as_array_mut()
                {
                    for model in arr {
                        model["analysis"] = json!(text);
                    }
                }

                let payload = json!({
                    "lang": lang_entry.lang,
                    "mode": mode_entry.mode,
                    "hash": mode_entry.hash,
                    "models": models_json,
                });

                let resp = client
                    .post(&url)
                    .header("Authorization", format!("Bearer {}", self.api_key))
                    .header("Content-Type", "application/json")
                    .json(&payload)
                    .send()
                    .with_context(|| format!("upload failed for {}/{}", lang_entry.lang, mode_entry.mode))?;

                if resp.status().is_success() {
                    let body: serde_json::Value = resp.json().unwrap_or_default();
                    let inserted = body["inserted"].as_u64().unwrap_or(0);
                    total_uploaded += inserted as usize;
                    println!(
                        "\u{1f4e4} uploaded {}/{}: {} results",
                        lang_entry.lang, mode_entry.mode, inserted
                    );
                } else {
                    let status = resp.status();
                    let body = resp.text().unwrap_or_default();
                    anyhow::bail!(
                        "upload failed for {}/{}: {} - {}",
                        lang_entry.lang,
                        mode_entry.mode,
                        status,
                        body
                    );
                }
            }
        }

        Ok(total_uploaded)
    }

    /// Fetch active/available benchmark models from the website model registry.
    pub fn fetch_model_routes(&self) -> Result<Vec<ModelRoute>> {
        let url = format!("{}/api/llm-benchmark-models?active=true", self.base_url);
        let resp = self
            .client()?
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .send()
            .context("fetch LLM benchmark models failed")?;

        if resp.status().is_success() {
            let body: serde_json::Value = resp.json().context("parse model registry response")?;
            parse_model_routes_response(&body)
        } else {
            let status = resp.status();
            let body = resp.text().unwrap_or_default();
            anyhow::bail!("fetch LLM benchmark models failed: {} - {}", status, body);
        }
    }

    /// Upload the task catalog to `POST /api/llm-benchmark-tasks`, derived from
    /// the benchmarks directory structure on disk.
    pub fn upload_task_catalog(&self, bench_root: &std::path::Path) -> Result<usize> {
        use std::collections::BTreeMap;
        use std::fs;

        let mut categories: BTreeMap<String, Vec<serde_json::Value>> = BTreeMap::new();

        let cats = fs::read_dir(bench_root).with_context(|| format!("read_dir {}", bench_root.display()))?;
        for cat_entry in cats.filter_map(|e| e.ok()) {
            if !cat_entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                continue;
            }
            let cat_name = cat_entry.file_name().to_string_lossy().to_string();
            let cat_path = cat_entry.path();

            let tasks = match fs::read_dir(&cat_path) {
                Ok(rd) => rd,
                Err(_) => continue,
            };
            for task_entry in tasks.filter_map(|e| e.ok()) {
                if !task_entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                    continue;
                }
                let task_name = task_entry.file_name().to_string_lossy().to_string();

                // Humanize task_name for title
                let title = task_name
                    .trim_start_matches(|c: char| c == 't' || c == '_' || c.is_ascii_digit())
                    .replace('_', " ")
                    .trim()
                    .to_string();
                let title = if title.is_empty() {
                    task_name.clone()
                } else {
                    title
                        .split_whitespace()
                        .map(|w| {
                            let mut c = w.chars();
                            match c.next() {
                                None => String::new(),
                                Some(f) => f.to_uppercase().to_string() + c.as_str(),
                            }
                        })
                        .collect::<Vec<_>>()
                        .join(" ")
                };

                // Read per-language prompts and golden answers
                let tasks_dir = task_entry.path().join("tasks");
                let answers_dir = task_entry.path().join("answers");
                let mut golden_answers = serde_json::Map::new();
                let mut descriptions = serde_json::Map::new();

                for (lang, prompt_file, answer_file) in [
                    ("rust", "rust.txt", "rust.rs"),
                    ("csharp", "csharp.txt", "csharp.cs"),
                    ("typescript", "typescript.txt", "typescript.ts"),
                ] {
                    if let Ok(prompt) = fs::read_to_string(tasks_dir.join(prompt_file)) {
                        descriptions.insert(lang.to_string(), json!(prompt.trim()));
                    }
                    if let Ok(answer) = fs::read_to_string(answers_dir.join(answer_file)) {
                        golden_answers.insert(lang.to_string(), json!(answer));
                    }
                }

                categories.entry(cat_name.clone()).or_default().push(json!({
                    "id": task_name,
                    "title": title,
                    "description": descriptions.get("rust").unwrap_or(&json!("")),
                    "descriptions": descriptions,
                    "golden_answers": golden_answers,
                }));
            }
        }

        let url = format!("{}/api/llm-benchmark-tasks", self.base_url);
        let payload = json!({ "categories": categories });

        let resp = self
            .client()?
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&payload)
            .send()
            .context("upload task catalog failed")?;

        if resp.status().is_success() {
            let body: serde_json::Value = resp.json().unwrap_or_default();
            let upserted = body["upserted"].as_u64().unwrap_or(0) as usize;
            println!("\u{1f4e4} uploaded task catalog: {} tasks", upserted);
            Ok(upserted)
        } else {
            let status = resp.status();
            let body = resp.text().unwrap_or_default();
            anyhow::bail!("task catalog upload failed: {} \u{2014} {}", status, body);
        }
    }

    /// Fetch available run dates from `GET /api/llm-benchmark-results?dates=true`.
    pub fn fetch_run_dates(&self, lang: Option<&str>, mode: Option<&str>) -> Result<Vec<String>> {
        let mut params = vec!["dates=true".to_string()];
        if let Some(l) = lang {
            params.push(format!("lang={}", urlencoding::encode(l)));
        }
        if let Some(m) = mode {
            params.push(format!("mode={}", urlencoding::encode(m)));
        }
        let url = format!("{}/api/llm-benchmark-results?{}", self.base_url, params.join("&"));

        let resp = self
            .client()?
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .send()
            .context("fetch run dates failed")?;

        if resp.status().is_success() {
            let body: serde_json::Value = resp.json().context("parse dates response")?;
            Ok(body["dates"]
                .as_array()
                .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                .unwrap_or_default())
        } else {
            let status = resp.status();
            let body = resp.text().unwrap_or_default();
            anyhow::bail!("fetch run dates failed: {} \u{2014} {}", status, body);
        }
    }

    /// Fetch failure results from `GET /api/llm-benchmark-results?failures=true`.
    pub fn fetch_failures(
        &self,
        lang: Option<&str>,
        mode: Option<&str>,
        model: Option<&str>,
        date: Option<&str>,
    ) -> Result<(Vec<serde_json::Value>, Option<String>)> {
        let mut params = vec!["failures=true".to_string()];
        if let Some(l) = lang {
            params.push(format!("lang={}", urlencoding::encode(l)));
        }
        if let Some(m) = mode {
            params.push(format!("mode={}", urlencoding::encode(m)));
        }
        if let Some(m) = model {
            params.push(format!("model={}", urlencoding::encode(m)));
        }
        if let Some(d) = date {
            params.push(format!("date={}", urlencoding::encode(d)));
        }
        let url = format!("{}/api/llm-benchmark-results?{}", self.base_url, params.join("&"));

        let resp = self
            .client()?
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .send()
            .context("fetch failures failed")?;

        if resp.status().is_success() {
            let body: serde_json::Value = resp.json().context("parse failures response")?;
            let date = body["date"].as_str().map(String::from);
            let results = body["results"].as_array().cloned().unwrap_or_default();
            Ok((results, date))
        } else {
            let status = resp.status();
            let body = resp.text().unwrap_or_default();
            anyhow::bail!("fetch failures failed: {} \u{2014} {}", status, body);
        }
    }

    /// Upload analysis for a specific (lang, mode, model) via the upload endpoint.
    pub fn upload_analysis(&self, lang: &str, mode: &str, model_name: &str, analysis: &str, date: &str) -> Result<()> {
        let payload = json!({
            "lang": lang,
            "mode": mode,
            "date": date,
            "hash": null,
            "models": [{
                "name": model_name,
                "tasks": {},
                "analysis": analysis,
            }],
        });

        let url = format!("{}/api/llm-benchmark-upload", self.base_url);
        let resp = self
            .client()?
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&payload)
            .send()
            .with_context(|| format!("upload analysis failed for {}/{}/{}", lang, mode, model_name))?;

        if resp.status().is_success() {
            println!("  uploaded analysis for {}/{}/{}/{}", lang, mode, model_name, date);
            Ok(())
        } else {
            let status = resp.status();
            let body = resp.text().unwrap_or_default();
            anyhow::bail!("upload analysis failed: {} \u{2014} {}", status, body);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_active_available_model_routes() {
        let body = json!({
            "models": [
                {
                    "displayName": "GPT Test",
                    "vendor": "openai",
                    "apiModel": "gpt-test",
                    "openrouterModel": "openai/gpt-test",
                    "active": true,
                    "available": true
                },
                {
                    "displayName": "Inactive",
                    "vendor": "openai",
                    "apiModel": "inactive",
                    "active": false,
                    "available": true
                },
                {
                    "displayName": "Unavailable",
                    "vendor": "openai",
                    "apiModel": "unavailable",
                    "active": true,
                    "available": false
                }
            ]
        });

        let routes = parse_model_routes_response(&body).unwrap();
        assert_eq!(routes.len(), 1);
        assert_eq!(routes[0].display_name, "GPT Test");
        assert_eq!(routes[0].vendor, Vendor::OpenAi);
        assert_eq!(routes[0].api_model, "gpt-test");
        assert_eq!(routes[0].openrouter_model.as_deref(), Some("openai/gpt-test"));
    }

    #[test]
    fn parses_snake_case_model_route_fields() {
        let body = json!({
            "models": [
                {
                    "display_name": "GPT Test",
                    "vendor": "openai",
                    "api_model": "gpt-test",
                    "openrouter_model": "openai/gpt-test",
                    "active": true,
                    "available": true
                }
            ]
        });

        let routes = parse_model_routes_response(&body).unwrap();
        assert_eq!(routes.len(), 1);
        assert_eq!(routes[0].display_name, "GPT Test");
        assert_eq!(routes[0].api_model, "gpt-test");
        assert_eq!(routes[0].openrouter_model.as_deref(), Some("openai/gpt-test"));
    }
}

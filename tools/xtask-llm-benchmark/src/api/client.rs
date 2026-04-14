use anyhow::{Context, Result};
use serde_json::json;

use crate::bench::results_merge::{canonical_mode, normalize_model_names};
use crate::bench::types::RunOutcome;
use crate::results::schema::Results;

/// HTTP client for the SpacetimeDB LLM benchmark API (spacetime-web Postgres).
///
/// Supports two POST endpoints that already exist in spacetime-web:
/// - `POST /api/llm-benchmark-upload` — upload benchmark results
/// - `POST /api/llm-benchmark-tasks` — upload task catalog
#[derive(Clone)]
pub struct ApiClient {
    client: reqwest::blocking::Client,
    base_url: String,
    api_key: String,
}

impl ApiClient {
    pub fn new(base_url: &str, api_key: &str) -> Result<Self> {
        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .context("failed to build HTTP client")?;
        Ok(Self {
            client,
            base_url: base_url.trim_end_matches('/').to_string(),
            api_key: api_key.to_string(),
        })
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
    pub fn upload_batch(
        &self,
        lang: &str,
        mode: &str,
        hash: &str,
        outcomes: &[RunOutcome],
        analysis: Option<&str>,
    ) -> Result<usize> {
        if outcomes.is_empty() {
            return Ok(0);
        }

        let mode = canonical_mode(mode);

        // Build in-memory Results so we can normalize model names
        let mut results = Results::default();
        {
            use crate::bench::results_merge::{canonical_model_name, ensure_lang, ensure_mode, ensure_model};

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
        let mut total_uploaded = 0usize;

        for lang_entry in &results.languages {
            for mode_entry in &lang_entry.modes {
                let mut payload = json!({
                    "lang": lang_entry.lang,
                    "mode": mode_entry.mode,
                    "hash": mode_entry.hash,
                    "models": mode_entry.models,
                });
                if let Some(text) = analysis {
                    payload["analysis"] = json!(text);
                }

                let resp = self
                    .client
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
                    eprintln!(
                        "\u{26a0}\u{fe0f} upload failed for {}/{}: {} \u{2014} {}",
                        lang_entry.lang, mode_entry.mode, status, body
                    );
                }
            }
        }

        let _ = lang;
        let _ = hash;
        Ok(total_uploaded)
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

                // Read first line of prompt.md as description
                let prompt_path = task_entry.path().join("prompt.md");
                let description = fs::read_to_string(&prompt_path)
                    .ok()
                    .and_then(|s| s.lines().next().map(|l| l.trim_start_matches('#').trim().to_string()))
                    .unwrap_or_default();

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

                categories.entry(cat_name.clone()).or_default().push(json!({
                    "id": task_name,
                    "title": title,
                    "description": description,
                }));
            }
        }

        let url = format!("{}/api/llm-benchmark-tasks", self.base_url);
        let payload = json!({ "categories": categories });

        let resp = self
            .client
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

}

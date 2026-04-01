use anyhow::{Context, Result};
use fs2::FileExt;
use std::collections::BTreeMap;
use std::fs;
use std::io::{Read, Write};
use std::path::Path;
use tempfile::NamedTempFile;

use crate::bench::types::RunOutcome;
use crate::results::schema::{LangEntry, ModeEntry, ModelEntry, Results};

fn load_results(path: &Path) -> Result<Results> {
    if !path.exists() {
        return Ok(Results::default());
    }
    let mut f = fs::File::open(path)?;
    let mut s = String::new();
    f.read_to_string(&mut s)?;
    let mut root: Results = serde_json::from_str(&s).with_context(|| format!("failed parsing {}", path.display()))?;
    normalize_model_names(&mut root);
    Ok(root)
}

/// Normalize all model names in loaded results and merge duplicates.
pub fn normalize_model_names(root: &mut Results) {
    for lang in &mut root.languages {
        for mode in &mut lang.modes {
            let mut merged: Vec<ModelEntry> = Vec::new();
            for mut model in mode.models.drain(..) {
                let canonical = canonical_model_name(&model.name);
                model.name = canonical;
                if let Some(existing) = merged.iter_mut().find(|m| m.name == model.name) {
                    // Merge tasks from duplicate into existing entry
                    for (task_id, outcome) in model.tasks {
                        existing.tasks.insert(task_id, outcome);
                    }
                    if existing.route_api_model.is_none() {
                        existing.route_api_model = model.route_api_model;
                    }
                } else {
                    merged.push(model);
                }
            }
            mode.models = merged;
        }
    }
}

fn save_atomic(path: &Path, root: &Results) -> Result<()> {
    let parent = path.parent().context("no parent dir for results path")?;
    fs::create_dir_all(parent)?;
    let mut tmp = NamedTempFile::new_in(parent)?;
    serde_json::to_writer_pretty(&mut tmp, root)?;
    tmp.flush()?;
    tmp.persist(path)?;
    Ok(())
}

pub fn ensure_lang<'a>(root: &'a mut Results, lang: &str) -> &'a mut LangEntry {
    if let Some(i) = root.languages.iter().position(|x| x.lang == lang) {
        return &mut root.languages[i];
    }
    root.languages.push(LangEntry {
        lang: lang.to_string(),
        modes: Vec::new(),
        golden_answers: BTreeMap::new(),
    });
    root.languages.last_mut().unwrap()
}

fn ensure_mode<'a>(lang_v: &'a mut LangEntry, mode: &str, hash: Option<String>) -> &'a mut ModeEntry {
    if let Some(i) = lang_v.modes.iter().position(|m| m.mode == mode) {
        if let Some(h) = hash {
            lang_v.modes[i].hash = Some(h);
        }
        return &mut lang_v.modes[i];
    }
    lang_v.modes.push(ModeEntry {
        mode: mode.to_string(),
        hash,
        models: Vec::new(),
    });
    lang_v.modes.last_mut().unwrap()
}

fn ensure_model<'a>(mode_v: &'a mut ModeEntry, name: &str) -> &'a mut ModelEntry {
    if let Some(i) = mode_v.models.iter().position(|m| m.name == name) {
        return &mut mode_v.models[i];
    }
    mode_v.models.push(ModelEntry {
        name: name.to_string(),
        route_api_model: None,
        tasks: Default::default(), // HashMap<String, RunOutcome>
    });
    mode_v.models.last_mut().unwrap()
}

/// Normalize mode aliases to their canonical names before saving.
fn canonical_mode(mode: &str) -> &str {
    match mode {
        "none" | "no_guidelines" => "no_context",
        other => other,
    }
}

/// Normalize model names so that OpenRouter-style IDs and case variants
/// resolve to the canonical display name from model_routes.
fn canonical_model_name(name: &str) -> String {
    use crate::llm::model_routes::default_model_routes;
    let lower = name.to_ascii_lowercase();
    for route in default_model_routes() {
        // Match by openrouter model id (e.g. "anthropic/claude-sonnet-4.6")
        if let Some(ref or) = route.openrouter_model {
            if lower == or.to_ascii_lowercase() {
                return route.display_name.to_string();
            }
        }
        // Match by api model id (e.g. "claude-sonnet-4-6")
        if lower == route.api_model.to_ascii_lowercase() {
            return route.display_name.to_string();
        }
        // Match by case-insensitive display name (e.g. "claude sonnet 4.6")
        if lower == route.display_name.to_ascii_lowercase() {
            return route.display_name.to_string();
        }
    }
    name.to_string()
}

pub fn merge_task_runs(path: &Path, mode: &str, runs: &[RunOutcome]) -> Result<()> {
    if runs.is_empty() {
        return Ok(());
    }
    let mode = canonical_mode(mode);

    let lock_path = path.with_extension("lock");
    let lock = fs::OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(&lock_path)?;
    lock.lock_exclusive()?;

    let res = (|| -> Result<()> {
        let mut root = load_results(path)?;

        for r in runs {
            let lang_v = ensure_lang(&mut root, &r.lang);

            // Always bump the mode hash to the latest run's hash
            let mode_v = ensure_mode(lang_v, mode, Some(r.hash.clone()));

            let canonical_name = canonical_model_name(&r.model_name);
            let model_v = ensure_model(mode_v, &canonical_name);

            // Always replace with the latest value (even if None)
            model_v.route_api_model = r.route_api_model.clone();

            // Sanitize volatile fields before saving to reduce git diff noise
            let mut sanitized = r.clone();
            sanitized.sanitize_for_commit();

            // Always overwrite the task result
            model_v.tasks.insert(r.task.clone(), sanitized);
        }

        // Update the top-level timestamp
        root.generated_at = Some(chrono::Utc::now().to_rfc3339());

        save_atomic(path, &root)
    })();

    let _ = lock.unlock();
    res
}

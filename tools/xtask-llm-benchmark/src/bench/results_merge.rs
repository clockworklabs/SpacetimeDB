use std::collections::BTreeMap;

use crate::results::schema::{LangEntry, ModeEntry, ModelEntry, Results};

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

pub fn ensure_mode<'a>(lang_v: &'a mut LangEntry, mode: &str, hash: Option<String>) -> &'a mut ModeEntry {
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

pub fn ensure_model<'a>(mode_v: &'a mut ModeEntry, name: &str) -> &'a mut ModelEntry {
    if let Some(i) = mode_v.models.iter().position(|m| m.name == name) {
        return &mut mode_v.models[i];
    }
    mode_v.models.push(ModelEntry {
        name: name.to_string(),
        route_api_model: None,
        tasks: Default::default(),
    });
    mode_v.models.last_mut().unwrap()
}

/// Normalize mode aliases to their canonical names before saving.
pub fn canonical_mode(mode: &str) -> &str {
    match mode {
        "none" | "no_guidelines" => "no_context",
        other => other,
    }
}

/// Normalize model names so that OpenRouter-style IDs and case variants
/// resolve to the canonical display name from model_routes.
pub fn canonical_model_name(name: &str) -> String {
    use crate::llm::model_routes::default_model_routes;
    let lower = name.to_ascii_lowercase();
    for route in default_model_routes() {
        // Match by openrouter model id (e.g. "anthropic/claude-sonnet-4.6")
        if let Some(ref or) = route.openrouter_model
            && lower == or.to_ascii_lowercase()
        {
            return route.display_name.to_string();
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

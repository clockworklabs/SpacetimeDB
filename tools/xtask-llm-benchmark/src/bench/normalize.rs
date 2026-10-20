use crate::bench::types::{LangEntry, ModeEntry, ModelEntry, Results};

pub fn ensure_lang<'a>(root: &'a mut Results, lang: &str) -> &'a mut LangEntry {
    if let Some(i) = root.languages.iter().position(|x| x.lang == lang) {
        return &mut root.languages[i];
    }
    root.languages.push(LangEntry {
        lang: lang.to_string(),
        modes: Vec::new(),
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

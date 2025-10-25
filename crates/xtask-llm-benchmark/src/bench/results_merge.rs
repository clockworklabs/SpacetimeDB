use anyhow::{Context, Result};
use fs2::FileExt;
use std::collections::HashMap;
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
    let root: Results = serde_json::from_str(&s).with_context(|| format!("failed parsing {}", path.display()))?;
    Ok(root)
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
        golden_answers: HashMap::new(),
    });
    root.languages.last_mut().unwrap()
}

fn ensure_mode<'a>(lang_v: &'a mut LangEntry, mode: &str, hash: Option<String>) -> &'a mut ModeEntry {
    if let Some(i) = lang_v.modes.iter().position(|m| m.mode == mode) {
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

pub fn merge_task_runs(path: &Path, mode: &str, runs: &[RunOutcome]) -> Result<()> {
    if runs.is_empty() {
        return Ok(());
    }

    let lock_path = path.with_extension("lock");
    let lock = fs::OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .open(&lock_path)?;
    lock.lock_exclusive()?;

    let res = (|| -> Result<()> {
        let mut root = load_results(path)?;

        for r in runs {
            let lang_v = ensure_lang(&mut root, &r.lang);
            let mode_v = ensure_mode(lang_v, mode, Some(r.hash.clone()));
            let model_v = ensure_model(mode_v, &r.model_name);

            // lift once at model level if available
            if model_v.route_api_model.is_none() {
                model_v.route_api_model = r.route_api_model.clone();
            }

            // store full RunOutcome per task id (overwrite)
            model_v.tasks.insert(r.task.clone(), r.clone());
        }

        save_atomic(path, &root)
    })();

    let _ = lock.unlock();
    res
}

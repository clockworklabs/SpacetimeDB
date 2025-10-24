use super::schema::{
    BenchmarkRun, CategorySummary, GoldenAnswer, LangSummary, ModeSummary, ModelSummary, Results, Summary, Totals,
};
use crate::bench::results_merge::ensure_lang;
use crate::context::constants::results_path_run;
use anyhow::{Context, Result};
use chrono::{SecondsFormat, Utc};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

pub fn load_run<P: AsRef<Path>>(path: P) -> Result<BenchmarkRun> {
    use std::io::ErrorKind;
    let p = path.as_ref();

    match fs::read_to_string(p) {
        Ok(raw) => {
            let v: BenchmarkRun = serde_json::from_str(&raw).with_context(|| format!("parse {}", p.display()))?;
            Ok(v)
        }
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(BenchmarkRun {
            version: 1,
            generated_at: String::new(),
            modes: vec![],
        }),
        Err(e) => Err(e).with_context(|| format!("read {}", p.display())),
    }
}

pub fn write_run(run: &BenchmarkRun) -> Result<()> {
    let path: PathBuf = results_path_run();
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, serde_json::to_vec_pretty(run)?)?;
    fs::rename(&tmp, path)?; // atomic-ish replace
    Ok(())
}

fn pct(passed: u32, total: u32) -> f32 {
    if total == 0 {
        0.0
    } else {
        (passed as f32) * 100.0 / (total as f32)
    }
}

/// Build `Summary` from your in-memory `Results`.
pub fn summary_from_results(results: &Results) -> Summary {
    let mut by_language: HashMap<String, LangSummary> = HashMap::new();

    // Results.languages: Vec<LangEntry>
    for lang_ent in &results.languages {
        let lang_key = lang_ent.lang.clone();
        let lang_sum = by_language
            .entry(lang_key)
            .or_insert_with(|| LangSummary { modes: HashMap::new() });

        // LangEntry.modes: Vec<ModeEntry>
        for mode_ent in &lang_ent.modes {
            let mode_key = mode_ent.mode.clone();
            let mode_sum = lang_sum
                .modes
                .entry(mode_key)
                .or_insert_with(|| ModeSummary { models: HashMap::new() });

            // ModeEntry.models: Vec<ModelEntry>
            for model_ent in &mode_ent.models {
                let model_key = model_ent.name.clone();
                let model_sum = mode_sum.models.entry(model_key).or_insert_with(|| ModelSummary {
                    categories: HashMap::new(),
                    totals: Totals::default(),
                });

                // ModelEntry.tasks: HashMap<String, TaskEntry>
                for (_task_id, t) in &model_ent.tasks {
                    let cat_sum = model_sum
                        .categories
                        .entry(t.category.clone().expect("Missing category"))
                        .or_insert_with(CategorySummary::default);

                    cat_sum.tasks += 1;
                    cat_sum.total_tests += t.total_tests;
                    cat_sum.passed_tests += t.passed_tests;

                    model_sum.totals.tasks += 1;
                    model_sum.totals.total_tests += t.total_tests;
                    model_sum.totals.passed_tests += t.passed_tests;
                }

                for v in model_sum.categories.values_mut() {
                    v.pass_pct = pct(v.passed_tests, v.total_tests);
                }
                model_sum.totals.pass_pct = pct(model_sum.totals.passed_tests, model_sum.totals.total_tests);
            }
        }
    }

    Summary {
        version: 1,
        generated_at: Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true),
        by_language,
    }
}

/// Convenience: read the details file **into your Results**, then write summary.
pub fn write_summary_from_details_file<PIn: AsRef<Path>, POut: AsRef<Path>>(
    details_json: PIn,
    out_json: POut,
) -> Result<()> {
    let data = fs::read_to_string(details_json.as_ref())
        .with_context(|| format!("failed to read {}", details_json.as_ref().display()))?;
    let results: Results =
        serde_json::from_str(&data).with_context(|| "failed to deserialize Results from details file")?;
    let summary = summary_from_results(&results);
    let pretty = serde_json::to_string_pretty(&summary)?;
    fs::write(out_json.as_ref(), pretty).with_context(|| format!("failed to write {}", out_json.as_ref().display()))?;
    Ok(())
}

fn collect_seen_tasks_by_lang(root: &Results) -> HashMap<&'static str, HashSet<String>> {
    let mut map: HashMap<&'static str, HashSet<String>> = HashMap::new();

    for le in &root.languages {
        let target = match le.lang.as_str() {
            "rust" => map.entry("rust").or_default(),
            "csharp" => map.entry("csharp").or_default(),
            _ => continue,
        };
        for me in &le.modes {
            for mdl in &me.models {
                for tid in mdl.tasks.keys() {
                    target.insert(tid.to_string());
                }
            }
        }
    }
    map
}

/// Update language-level golden answers from filesystem.
/// - `all=false`  : only ingest if task_id exists for that language in JSON
/// - `overwrite=false`: keep existing entries (no clobber)
pub fn update_golden_answers_on_disk(
    results_json_path: &Path,
    bench_root: &Path,
    all: bool,
    overwrite: bool,
) -> Result<()> {
    // Load current results
    let bytes = fs::read(results_json_path).with_context(|| format!("read {}", results_json_path.display()))?;
    let mut root: Results =
        serde_json::from_slice(&bytes).with_context(|| format!("parse {}", results_json_path.display()))?;

    let seen = collect_seen_tasks_by_lang(&root);

    // benchmarks/<category>/<task>/answers/{rust.rs|csharp.cs}
    let cats = fs::read_dir(bench_root).with_context(|| format!("read_dir {}", bench_root.display()))?;

    for cat_entry in cats {
        let cat_entry = match cat_entry {
            Ok(e) => e,
            Err(_) => continue,
        };
        let Ok(cat_ft) = cat_entry.file_type() else { continue };
        if !cat_ft.is_dir() {
            continue;
        }
        let Ok(cat_name) = cat_entry.file_name().into_string() else {
            continue;
        };
        let cat_path = cat_entry.path();

        let tasks = match fs::read_dir(&cat_path) {
            Ok(rd) => rd,
            Err(_) => continue,
        };
        for task_entry in tasks {
            let task_entry = match task_entry {
                Ok(e) => e,
                Err(_) => continue,
            };
            let Ok(task_ft) = task_entry.file_type() else { continue };
            if !task_ft.is_dir() {
                continue;
            }
            let Ok(task_name) = task_entry.file_name().into_string() else {
                continue;
            };
            let task_path = task_entry.path();

            let answers_dir = task_path.join("answers");
            if !answers_dir.is_dir() {
                continue;
            }

            // IDs: bare (matches runs like "t_004_insert"); alias kept for compatibility.
            let task_id_bare = task_name.clone();
            let task_id_alias = format!("{}/{}", cat_name, task_name);

            // rust.rs → lang "rust"
            let rust_path = answers_dir.join("rust.rs");
            if rust_path.is_file() {
                if all
                    || seen
                        .get("rust")
                        .map_or(false, |s| s.contains(&task_id_bare) || s.contains(&task_id_alias))
                {
                    let le = ensure_lang(&mut root, "rust");
                    let text =
                        fs::read_to_string(&rust_path).with_context(|| format!("read {}", rust_path.display()))?;
                    if overwrite || !le.golden_answers.contains_key(&task_id_bare) {
                        le.golden_answers.insert(
                            task_id_bare.clone(),
                            GoldenAnswer {
                                answer: text.clone(),
                                syntax: Some("rust".into()),
                            },
                        );
                    }
                    // comment out the next line to drop the alias entirely
                    le.golden_answers.entry(task_id_alias.clone()).or_insert(GoldenAnswer {
                        answer: text,
                        syntax: Some("rust".into()),
                    });
                }
            }

            // csharp.cs → lang "csharp"
            let cs_path = answers_dir.join("csharp.cs");
            if cs_path.is_file() {
                if all
                    || seen
                        .get("csharp")
                        .map_or(false, |s| s.contains(&task_id_bare) || s.contains(&task_id_alias))
                {
                    let le = ensure_lang(&mut root, "csharp");
                    let text = fs::read_to_string(&cs_path).with_context(|| format!("read {}", cs_path.display()))?;
                    if overwrite || !le.golden_answers.contains_key(&task_id_bare) {
                        le.golden_answers.insert(
                            task_id_bare.clone(),
                            GoldenAnswer {
                                answer: text.clone(),
                                syntax: Some("csharp".into()),
                            },
                        );
                    }
                    // comment out the next line to drop the alias entirely
                    le.golden_answers.entry(task_id_alias.clone()).or_insert(GoldenAnswer {
                        answer: text,
                        syntax: Some("csharp".into()),
                    });
                }
            }
        }
    }

    // Save pretty-printed
    let mut out = Vec::new();
    serde_json::to_writer_pretty(&mut out, &root)?;
    out.push(b'\n');
    fs::write(results_json_path, out).with_context(|| format!("write {}", results_json_path.display()))?;
    Ok(())
}

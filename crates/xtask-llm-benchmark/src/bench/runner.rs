use anyhow::{anyhow, bail, Context, Result};
use chrono::Utc;
use futures::{stream, StreamExt};
use serde_json::json;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Instant;
use tokio::sync::Mutex;
use tokio::task;

use crate::bench::publishers::{DotnetPublisher, SpacetimeRustPublisher};
use crate::bench::results_merge::merge_task_runs;
use crate::bench::templates::materialize_project;
use crate::bench::types::RunOneError;
pub(crate) use crate::bench::types::{RunOutcome, TaskPaths};
use crate::bench::utils::{
    bench_concurrency, category_slug, debug_llm, fmt_dur, print_llm_output, sanitize_db_name, task_slug,
    work_server_dir_scoped,
};
use crate::bench::{registry, Publisher};
use crate::context::constants::results_path_details;
use crate::eval::{Lang, ScoreDetails};
use crate::llm::model_routes::ModelRoute;
use crate::llm::provider::LlmProvider;

pub struct TaskRunner {
    pub bench_root: PathBuf,
    pub rust_publisher: SpacetimeRustPublisher,
    pub cs_publisher: DotnetPublisher,
}

static BUILT_KEYS: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();

fn build_key(lang: Lang, selectors: Option<&[String]>) -> String {
    let v = match selectors {
        Some(s) if !s.is_empty() => {
            let mut t = s.to_vec();
            t.sort(); // stable key independent of order
            t
        }
        _ => vec!["ALL".to_string()],
    };
    let joined = v.join(",");
    format!("{lang:?}:{joined}")
}

/// Build goldens **once per (lang, selector-set)** in this process.
/// If selectors is None/empty, that means "ALL tasks".
pub async fn ensure_goldens_built_once(bench_root: &Path, lang: Lang, selectors: Option<&[String]>) -> Result<()> {
    let key = build_key(lang, selectors);
    let set = BUILT_KEYS.get_or_init(|| Mutex::new(HashSet::new()));
    {
        let set = set.lock();
        if set.await.contains(&key) {
            return Ok(());
        }
    }
    // single-flight for this key
    let set_guard = set.lock().await;
    if set_guard.contains(&key) {
        return Ok(());
    }

    // IMPORTANT: pass selectors through so we only build needed goldens
    build_goldens_only_for_lang(bench_root, lang, selectors).await?;

    // mark as built
    drop(set_guard);
    let mut set = BUILT_KEYS.get().unwrap().lock().await;
    set.insert(key);
    Ok(())
}

async fn publish_rust_async(publisher: SpacetimeRustPublisher, wdir: PathBuf, db: String) -> Result<()> {
    task::spawn_blocking(move || publisher.publish(&wdir, &db)).await??;
    Ok(())
}
async fn publish_cs_async(publisher: DotnetPublisher, wdir: PathBuf, db: String) -> Result<()> {
    task::spawn_blocking(move || publisher.publish(&wdir, &db)).await??;
    Ok(())
}

impl TaskRunner {
    pub fn new(bench_root: PathBuf, rust_publisher: SpacetimeRustPublisher, cs_publisher: DotnetPublisher) -> Self {
        Self {
            bench_root,
            rust_publisher,
            cs_publisher,
        }
    }

    pub async fn publish_golden_only(
        &self,
        lang: Lang,
        category: &str,
        task_id: &str,
        golden_src_text: &str,
        golden_db: String,
    ) -> Result<()> {
        let lang_name = match lang {
            Lang::Rust => "rust",
            Lang::CSharp => "csharp",
        };

        let golden_wdir = work_server_dir_scoped(category, task_id, lang_name, "golden", "");
        if golden_wdir.exists() {
            let _ = fs::remove_dir_all(&golden_wdir);
        }
        let _golden_proj_root = materialize_project(lang_name, category, task_id, "golden", "", golden_src_text)?;

        match lang {
            Lang::Rust => publish_rust_async(self.rust_publisher, golden_wdir, golden_db).await?,
            Lang::CSharp => publish_cs_async(self.cs_publisher, golden_wdir, golden_db).await?,
        }
        Ok(())
    }

    async fn publish_llm(
        &self,
        lang: Lang,
        category: &str,
        task_id: &str,
        route_tag: &str,
        llm_output: &str,
        llm_db: String,
    ) -> Result<()> {
        let lang_name = match lang {
            Lang::Rust => "rust",
            Lang::CSharp => "csharp",
        };

        // llm
        let llm_wdir = work_server_dir_scoped(category, task_id, lang_name, "llm", route_tag);
        if llm_wdir.exists() {
            let _ = fs::remove_dir_all(&llm_wdir);
        }
        let _llm_proj_root = materialize_project(lang_name, category, task_id, "llm", route_tag, llm_output)?;

        match lang {
            Lang::Rust => {
                publish_rust_async(self.rust_publisher, llm_wdir, llm_db).await?;
            }
            Lang::CSharp => {
                publish_cs_async(self.cs_publisher, llm_wdir, llm_db).await?;
            }
        }

        Ok(())
    }

    pub async fn run_one(
        &self,
        task: &TaskPaths,
        lang_name: &str,
        lang: Lang,
        route: &ModelRoute,
        context: &str,
        hash: &str,
        llm: &dyn LlmProvider,
    ) -> Result<RunOutcome, RunOneError> {
        let wall = Instant::now();
        let started = Utc::now();

        let category = category_slug(&task.root);
        let task_id = task_slug(&task.root);
        let route_tag = sanitize_db_name(&route.display_name);
        let golden_db = sanitize_db_name(&format!("{}-{}-golden", category, task_id));
        let llm_db = sanitize_db_name(&format!("{}-{}-{}-llm", category, task_id, route_tag));

        // resolve spec + prompt
        let ctor = registry::resolve_by_path(&task.root)?;
        let spec = ctor();

        // fetch scorers up front
        let scorers = spec.scorers_for(lang, &route_tag);
        let total_tasks = scorers.len();

        let prompt_builder = (spec.make_prompt)(lang);
        println!("→ [{}] {}: building prompt", lang_name, route.display_name);
        let prompt = prompt_builder.build_segmented(context);

        println!("→ [{}] {}: calling provider", lang_name, route.display_name);
        let llm_output = tokio::time::timeout(std::time::Duration::from_secs(200), llm.generate(route, &prompt))
            .await
            .map_err(|_| RunOneError::Other(anyhow!("LLM call timed out")))?
            .map_err(RunOneError::Other)?;

        if debug_llm() {
            print_llm_output(route.display_name, &task_id, &llm_output);
        }

        // Publish — NO early return. Capture error immutably.
        let publish_error: Option<String> = self
            .publish_llm(lang, &category, &task_id, &route_tag, &llm_output, llm_db.clone())
            .await
            .err()
            .map(|e| {
                eprintln!(
                    "⚠️ publish failed for {}/{}/{}: {e:#}",
                    category, task_id, route.display_name
                );
                format!("{:#}", e)
            });

        // Scoring (skip if publish failed)
        let mut passed = 0usize;
        let mut partial_sum = 0f32;
        let mut scorer_details: HashMap<String, ScoreDetails> = HashMap::new();

        if publish_error.is_none() {
            println!("→ [{}] {}: scoring", lang_name, route.display_name);
            for s in &scorers {
                let r = s.score(&llm_output);
                if r.pass {
                    passed += 1;
                } else {
                    partial_sum += r.partial.max(0.0).min(1.0);
                }
                scorer_details.insert(s.id().to_string(), r.clone());
            }
        } else {
            println!(
                "→ [{}] {}: publish failed — skipping scoring (0/{})",
                lang_name, route.display_name, total_tasks
            );
            // Record the error in details
            scorer_details.insert(
                "publish_error".into(),
                ScoreDetails {
                    pass: false,
                    partial: 0.0,
                    notes: serde_json::json!({
                        "phase": "build_or_publish",
                        "error": publish_error.as_deref().unwrap_or("unknown"),
                    }),
                },
            );
        }

        let score_pct = if total_tasks == 0 {
            0.0
        } else {
            ((passed as f32) + partial_sum) / (total_tasks as f32) * 100.0
        };

        let finished = Utc::now();
        let took = wall.elapsed();
        println!(
            "→ [{}] {}: done (passed {}/{}, {:.1}%) — {}",
            lang_name,
            route.display_name,
            passed,
            total_tasks,
            score_pct,
            fmt_dur(took)
        );

        Ok(RunOutcome {
            hash: hash.to_string(),
            task: task_id.clone(),
            lang: lang_name.to_string(),
            model_name: route.display_name.to_string(),
            vendor: route.vendor.slug().to_string(),

            // publish status + scoring results
            golden_published: publish_error.is_none(),
            total_tests: total_tasks as u32,
            passed_tests: passed as u32,
            category: Some(category.clone()),

            // ALWAYS return the LLM code (even on publish error)
            llm_output: Some(llm_output),

            // paths/dbs
            route_api_model: Some(route.api_model.to_string()),
            golden_db: Some(golden_db),
            llm_db: Some(llm_db),
            work_dir_golden: Some(
                work_server_dir_scoped(&category, &task_id, lang_name, "golden", "")
                    .to_string_lossy()
                    .into_owned(),
            ),
            work_dir_llm: Some(
                work_server_dir_scoped(&category, &task_id, lang_name, "llm", &route_tag)
                    .to_string_lossy()
                    .into_owned(),
            ),

            scorer_details: Some(scorer_details),
            started_at: Some(started),
            finished_at: Some(finished),
        })
    }
}

pub async fn run_all_for_model_async_for_lang(
    bench_root: &Path,
    mode: &str,
    hash: &str,
    route: &ModelRoute,
    context: &str,
    llm: &dyn LlmProvider,
    lang: Lang,
) -> Result<Vec<RunOutcome>> {
    let total_wall = Instant::now();

    // 1) run per-task LLM builds + scoring
    let tasks = discover_tasks(bench_root)?;
    let runner = TaskRunner::new(PathBuf::from(bench_root), SpacetimeRustPublisher, DotnetPublisher);
    let lang_name = lang.as_str();
    let buf = bench_concurrency();

    let results: Vec<(TaskPaths, Result<RunOutcome, RunOneError>)> =
        futures::stream::iter(tasks.into_iter().map(|task| {
            let runner = &runner;
            let route = route;
            let lang_name = lang_name.to_string();
            async move {
                let started = Utc::now();
                let res = runner.run_one(&task, &lang_name, lang, route, context, hash, llm).await;
                (
                    task,
                    res.map(|mut o| {
                        o.started_at.get_or_insert(started);
                        o
                    }),
                )
            }
        }))
        .buffer_unordered(buf)
        .collect()
        .await;

    let mut outcomes = Vec::new();
    let mut errs = 0usize;
    for (task, r) in results {
        match r {
            Ok(v) => outcomes.push(v),
            // you caught an error that *includes* the generated code
            Err(RunOneError::WithOutput { msg, llm_output }) => {
                errs += 1;
                eprintln!("⚠️ task failed but continuing: {msg}");
                outcomes.push(build_fail_outcome(
                    &task,
                    lang_name,
                    route,
                    hash,
                    anyhow::anyhow!(msg),
                    Some(llm_output),
                ));
            }
            // generic error, no code available
            Err(RunOneError::Other(e)) => {
                errs += 1;
                eprintln!("⚠️ task failed but continuing: {e:?}");
                outcomes.push(build_fail_outcome(&task, lang_name, route, hash, e, None));
            }
        }
    }

    println!("[runner] completed batch: ok={} err={}", outcomes.len(), errs);

    if !outcomes.is_empty() {
        merge_task_runs(results_path_details().as_path(), mode, &outcomes)?;
    } else {
        eprintln!("[runner] no successful runs; not calling merge_task_runs");
    }

    println!(
        "✓ [{}] {}: total {}",
        lang_name,
        route.display_name,
        fmt_dur(total_wall.elapsed())
    );
    Ok(outcomes)
}

// run only selected tasks by selectors like 1/01/001 or t_001
pub async fn run_selected_for_model_async_for_lang(
    bench_root: &Path,
    mode: &str,
    hash: &str,
    route: &ModelRoute,
    context: &str,
    llm: &dyn LlmProvider,
    lang: Lang,
    selectors: &[impl AsRef<str>],
) -> Result<Vec<RunOutcome>> {
    let total_wall = Instant::now();

    let wanted: HashSet<String> = selectors
        .iter()
        .map(|s| normalize_task_selector(s.as_ref()))
        .collect::<Result<_>>()?;

    let tasks = discover_tasks(bench_root)?;
    let selected: Vec<TaskPaths> = tasks
        .into_iter()
        .filter(|t| {
            let name = t.root.file_name().and_then(|x| x.to_str()).unwrap_or("");
            wanted.iter().any(|w| name.starts_with(w))
        })
        .collect();

    if selected.is_empty() {
        bail!("no tasks matched {:?}", wanted);
    }

    let runner = TaskRunner::new(PathBuf::from(bench_root), SpacetimeRustPublisher, DotnetPublisher);
    let lang_name = lang.as_str();
    let buf = bench_concurrency();

    let results: Vec<(TaskPaths, Result<RunOutcome, RunOneError>)> =
        futures::stream::iter(selected.into_iter().map(|task| {
            let runner = &runner;
            let route = route;
            let lang_name = lang_name.to_string();
            async move {
                let started = Utc::now();
                let res = runner.run_one(&task, &lang_name, lang, route, context, hash, llm).await;
                (
                    task,
                    res.map(|mut o| {
                        o.started_at.get_or_insert(started);
                        o
                    }),
                )
            }
        }))
        .buffer_unordered(buf)
        .collect()
        .await;

    let mut outcomes = Vec::with_capacity(results.len());
    let mut errs = 0usize;

    for (task, r) in results {
        match r {
            Ok(v) => outcomes.push(v),
            Err(RunOneError::WithOutput { msg, llm_output }) => {
                errs += 1;
                eprintln!("⚠️ task failed but continuing: {msg}");
                outcomes.push(build_fail_outcome(
                    &task,
                    lang_name,
                    route,
                    hash,
                    anyhow::anyhow!(msg),
                    Some(llm_output),
                ));
            }
            Err(RunOneError::Other(e)) => {
                errs += 1;
                eprintln!("⚠️ task failed but continuing: {e:?}");
                outcomes.push(build_fail_outcome(&task, lang_name, route, hash, e, None));
            }
        }
    }

    if !outcomes.is_empty() {
        merge_task_runs(results_path_details().as_path(), mode, &outcomes)?;
    }

    println!(
        "✓ [{}] {}: total {} (err={})",
        lang_name,
        route.display_name,
        fmt_dur(total_wall.elapsed()),
        errs
    );
    Ok(outcomes)
}

pub async fn run_selected_or_all_for_model_async_for_lang(
    bench_root: &Path,
    mode: &str,
    hash: &str,
    route: &ModelRoute,
    context: &str,
    llm: &dyn LlmProvider,
    lang: Lang,
    selectors: Option<&[impl AsRef<str>]>,
) -> Result<Vec<RunOutcome>> {
    if let Some(sels) = selectors {
        if !sels.is_empty() {
            return run_selected_for_model_async_for_lang(bench_root, mode, hash, route, context, llm, lang, sels)
                .await;
        }
    }
    run_all_for_model_async_for_lang(bench_root, mode, hash, route, context, llm, lang).await
}

pub async fn build_goldens_only_for_lang(bench_root: &Path, lang: Lang, selectors: Option<&[String]>) -> Result<()> {
    let tasks = if let Some(sels) = selectors {
        let wanted: HashSet<String> = sels.iter().map(|s| normalize_task_selector(s)).collect::<Result<_>>()?;
        let all = discover_tasks(bench_root)?;
        let filtered: Vec<TaskPaths> = all
            .into_iter()
            .filter(|t| {
                let name = t.root.file_name().and_then(|x| x.to_str()).unwrap_or("");
                wanted.iter().any(|w| name.starts_with(w))
            })
            .collect();
        if filtered.is_empty() {
            bail!("no tasks matched {:?}", wanted);
        }
        filtered
    } else {
        discover_tasks(bench_root)?
    };

    let runner = TaskRunner::new(PathBuf::from(bench_root), SpacetimeRustPublisher, DotnetPublisher);
    let lang_name = lang.as_str();
    let buf = bench_concurrency();

    stream::iter(tasks.into_iter().map(|task| {
        let runner = &runner;
        async move {
            let category = category_slug(&task.root);
            let task_id = task_slug(&task.root);
            let golden_db = sanitize_db_name(&format!("{}-{}-golden", category, task_id));
            let golden_src_text = load_golden_source(&task, lang)?;
            println!("→ [{}] build golden {} {}", lang_name, category, task_id);
            runner
                .publish_golden_only(lang, &category, &task_id, &golden_src_text, golden_db)
                .await
        }
    }))
    .buffer_unordered(buf)
    .collect::<Vec<_>>()
    .await
    .into_iter()
    .collect::<Result<Vec<_>>>()?;

    println!("✓ [{}] goldens build/publish: complete", lang_name);
    Ok(())
}

fn discover_tasks(benchmarks_root: &Path) -> Result<Vec<TaskPaths>> {
    let mut out = Vec::new();
    for cat in read_dirs(benchmarks_root)? {
        for task in read_dirs(&cat)? {
            out.push(TaskPaths {
                root: task.clone(),
                answers_rust: task.join("answers/rust/server"),
                answers_csharp: task.join("answers/csharp/server"),
            });
        }
    }
    Ok(out)
}

fn build_fail_outcome(
    task: &TaskPaths,
    lang_name: &str,
    route: &ModelRoute,
    hash: &str,
    err: anyhow::Error,
    llm_output: Option<String>,
) -> RunOutcome {
    let category = category_slug(&task.root);
    let task_id = task_slug(&task.root);
    let now = Utc::now();
    let mut sd: HashMap<String, ScoreDetails> = HashMap::new();
    sd.insert(
        "publish_error".to_string(),
        ScoreDetails {
            pass: false,
            partial: 0.0,
            notes: json!({
                "phase": "build_or_publish",
                "error": format!("{:#}", err),
            }),
        },
    );

    RunOutcome {
        hash: hash.to_string(),
        task: task_id.clone(),
        lang: lang_name.to_string(),
        golden_published: false,
        category: Some(category),

        model_name: route.display_name.to_string(),
        total_tests: 1,
        passed_tests: 0,

        llm_output,

        route_api_model: Some(route.api_model.to_string()),
        golden_db: None,
        llm_db: None,
        work_dir_golden: None,
        work_dir_llm: None,
        scorer_details: Some(sd),

        vendor: route.vendor.slug().to_string(),
        started_at: Some(now),
        finished_at: Some(now),
    }
}

fn read_dirs(p: &Path) -> Result<Vec<PathBuf>> {
    let mut v = Vec::new();
    for e in fs::read_dir(p).with_context(|| format!("read_dir {}", p.display()))? {
        let e = e?;
        let path = e.path();
        if path.is_dir() {
            v.push(path);
        }
    }
    Ok(v)
}

// TEST_CASE/answers/csharp.cs and TEST_CASE/rust.rs
fn load_golden_source(task: &TaskPaths, lang: Lang) -> Result<String> {
    match lang {
        Lang::Rust => {
            let p = task.root.join("answers").join("rust.rs");
            fs::read_to_string(&p).with_context(|| format!("read {}", p.display()))
        }
        Lang::CSharp => {
            let p = task.root.join("answers").join("csharp.cs");
            fs::read_to_string(&p).with_context(|| format!("read {}", p.display()))
        }
    }
}

// "1" | "01" | "001" | "t_001" -> "t_001"
fn normalize_task_selector(raw: &str) -> Result<String> {
    let s = raw.trim().to_ascii_lowercase();
    if s.is_empty() {
        bail!("empty task selector");
    }
    if let Some(rest) = s.strip_prefix("t_") {
        if rest.chars().all(|c| c.is_ascii_digit()) {
            let n: u32 = rest.parse()?;
            return Ok(format!("t_{:03}", n));
        }
        bail!("invalid task selector: {raw}");
    }
    if s.chars().all(|c| c.is_ascii_digit()) {
        let n: u32 = s.parse()?;
        return Ok(format!("t_{:03}", n));
    }
    bail!("invalid task selector: {raw}")
}

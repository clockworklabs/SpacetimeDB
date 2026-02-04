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

use crate::bench::publishers::{DotnetPublisher, SpacetimeRustPublisher, TypeScriptPublisher};
use crate::bench::results_merge::merge_task_runs;
use crate::bench::templates::materialize_project;
use crate::bench::types::{BenchRunContext, PublishParams, RunContext, RunOneError};
pub(crate) use crate::bench::types::{RunOutcome, TaskPaths};
use crate::bench::utils::{
    bench_concurrency, bench_csharp_concurrency, category_slug, debug_llm, fmt_dur, print_llm_output, sanitize_db_name,
    task_slug, work_server_dir_scoped,
};
use crate::bench::Publisher;
use crate::eval::{Lang, ScoreDetails};
use crate::generated::resolve_by_path;
use crate::llm::model_routes::ModelRoute;

pub struct TaskRunner {
    pub bench_root: PathBuf,
    pub rust_publisher: SpacetimeRustPublisher,
    pub cs_publisher: DotnetPublisher,
    pub ts_publisher: TypeScriptPublisher,
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
pub async fn ensure_goldens_built_once(
    host: Option<String>,
    bench_root: &Path,
    lang: Lang,
    selectors: Option<&[String]>,
) -> Result<()> {
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
    build_goldens_only_for_lang(host, bench_root, lang, selectors).await?;

    // mark as built
    drop(set_guard);
    let mut set = BUILT_KEYS.get().unwrap().lock().await;
    set.insert(key);
    Ok(())
}

async fn publish_rust_async(
    publisher: SpacetimeRustPublisher,
    host_url: String,
    wdir: PathBuf,
    db: String,
) -> Result<()> {
    task::spawn_blocking(move || publisher.publish(&host_url, &wdir, &db)).await??;
    Ok(())
}
async fn publish_cs_async(publisher: DotnetPublisher, host_url: String, wdir: PathBuf, db: String) -> Result<()> {
    task::spawn_blocking(move || publisher.publish(&host_url, &wdir, &db)).await??;
    Ok(())
}
async fn publish_ts_async(publisher: TypeScriptPublisher, host_url: String, wdir: PathBuf, db: String) -> Result<()> {
    task::spawn_blocking(move || publisher.publish(&host_url, &wdir, &db)).await??;
    Ok(())
}

impl TaskRunner {
    pub fn new(
        bench_root: PathBuf,
        rust_publisher: SpacetimeRustPublisher,
        cs_publisher: DotnetPublisher,
        ts_publisher: TypeScriptPublisher,
    ) -> Self {
        Self {
            bench_root,
            rust_publisher,
            cs_publisher,
            ts_publisher,
        }
    }

    pub async fn publish_golden_only(
        &self,
        lang: Lang,
        category: &str,
        task_id: &str,
        golden_src_text: &str,
        golden_db: String,
        host: Option<String>,
    ) -> Result<()> {
        self.publish(
            PublishParams {
                lang,
                category,
                task_id,
                route_tag: "",
                source_text: golden_src_text,
                db_name: golden_db,
                host,
            },
            "golden",
        )
        .await
    }

    async fn publish_llm(&self, params: PublishParams<'_>) -> Result<()> {
        self.publish(params, "llm").await
    }

    async fn publish(&self, params: PublishParams<'_>, phase: &str) -> Result<()> {
        let lang_name = match params.lang {
            Lang::Rust => "rust",
            Lang::CSharp => "csharp",
            Lang::TypeScript => "typescript",
        };

        let wdir = work_server_dir_scoped(params.category, params.task_id, lang_name, phase, params.route_tag);
        if wdir.exists() {
            let _ = fs::remove_dir_all(&wdir);
        }
        let _proj_root = materialize_project(
            lang_name,
            params.category,
            params.task_id,
            phase,
            params.route_tag,
            params.source_text,
        )?;

        let host_url = params.host.unwrap_or_else(|| "local".to_owned());
        match params.lang {
            Lang::Rust => publish_rust_async(self.rust_publisher, host_url, wdir, params.db_name).await?,
            Lang::CSharp => publish_cs_async(self.cs_publisher, host_url, wdir, params.db_name).await?,
            Lang::TypeScript => publish_ts_async(self.ts_publisher, host_url, wdir, params.db_name).await?,
        }

        Ok(())
    }

    pub async fn run_one(&self, task: &TaskPaths, cfg: &RunContext<'_>) -> Result<RunOutcome, RunOneError> {
        let wall = Instant::now();
        let started = Utc::now();

        let category = category_slug(&task.root);
        let task_id = task_slug(&task.root);
        let route_tag = sanitize_db_name(cfg.route.display_name);
        let golden_db = sanitize_db_name(&format!("{}-{}-golden", category, task_id));
        let llm_db = sanitize_db_name(&format!("{}-{}-{}-llm", category, task_id, route_tag));

        let ctor = resolve_by_path(&task.root)?;
        let spec = ctor();

        let scorers = spec.scorers_for(cfg.lang, &route_tag, cfg.host.as_deref().unwrap_or("local"));
        let total_tasks = scorers.len();

        let prompt_builder = (spec.make_prompt)(cfg.lang);
        println!("→ [{}] {}: building prompt", cfg.lang_name, cfg.route.display_name);
        let prompt = prompt_builder.build_segmented(cfg.context);

        println!("→ [{}] {}: calling provider", cfg.lang_name, cfg.route.display_name);
        let llm_output = tokio::time::timeout(std::time::Duration::from_secs(90), cfg.llm.generate(cfg.route, &prompt))
            .await
            .map_err(|_| RunOneError::Other(anyhow!("LLM call timed out")))?
            .map_err(RunOneError::Other)?;

        if debug_llm() {
            print_llm_output(cfg.route.display_name, &task_id, &llm_output);
        }

        let publish_error: Option<String> = self
            .publish_llm(PublishParams {
                lang: cfg.lang,
                category: &category,
                task_id: &task_id,
                route_tag: &route_tag,
                source_text: &llm_output,
                db_name: llm_db.clone(),
                host: cfg.host.clone(),
            })
            .await
            .err()
            .map(|e| {
                eprintln!(
                    "⚠️ publish failed for {}/{}/{}: {e:#}",
                    category, task_id, cfg.route.display_name
                );
                format!("{:#}", e)
            });

        let mut passed = 0usize;
        let mut partial_sum = 0f32;
        let mut scorer_details: HashMap<String, ScoreDetails> = HashMap::new();

        if publish_error.is_none() {
            println!("→ [{}] {}: scoring", cfg.lang_name, cfg.route.display_name);
            for s in &scorers {
                let r = s.score(&llm_output);
                if r.pass {
                    passed += 1;
                }
                partial_sum += r.partial.clamp(0.0, 1.0);
                scorer_details.insert(s.id().to_string(), r);
            }
        } else {
            println!(
                "→ [{}] {}: publish failed — skipping scoring (0/{})",
                cfg.lang_name, cfg.route.display_name, total_tasks
            );
            scorer_details.insert(
                "publish_error".into(),
                ScoreDetails {
                    pass: false,
                    partial: 0.0,
                    notes: json!({
                        "phase": "build_or_publish",
                        "error": publish_error.as_deref().unwrap_or("unknown"),
                    }),
                },
            );
        }

        let score_pct = if total_tasks == 0 {
            0.0
        } else {
            (partial_sum / total_tasks as f32) * 100.0
        };

        let finished = Utc::now();
        let took = wall.elapsed();
        println!(
            "→ [{}] {}/{}/{}: done (passed {}/{}, {:.1}%) — {}",
            cfg.lang_name,
            category,
            task_id,
            cfg.route.display_name,
            passed as u32,
            total_tasks,
            score_pct,
            fmt_dur(took)
        );

        Ok(RunOutcome {
            hash: cfg.hash.to_string(),
            task: task_id.clone(),
            lang: cfg.lang_name.to_string(),
            model_name: cfg.route.display_name.to_string(),
            vendor: cfg.route.vendor.slug().to_string(),
            golden_published: publish_error.is_none(),
            total_tests: total_tasks as u32,
            passed_tests: passed as u32,
            category: Some(category.clone()),
            llm_output: Some(llm_output),
            route_api_model: Some(cfg.route.api_model.to_string()),
            golden_db: Some(golden_db),
            llm_db: Some(llm_db),
            work_dir_golden: Some(
                work_server_dir_scoped(&category, &task_id, cfg.lang_name, "golden", "")
                    .to_string_lossy()
                    .into_owned(),
            ),
            work_dir_llm: Some(
                work_server_dir_scoped(&category, &task_id, cfg.lang_name, "llm", &route_tag)
                    .to_string_lossy()
                    .into_owned(),
            ),
            scorer_details: Some(scorer_details),
            started_at: Some(started),
            finished_at: Some(finished),
        })
    }
}

pub async fn run_all_for_model_async_for_lang(cfg: &BenchRunContext<'_>) -> Result<Vec<RunOutcome>> {
    let total_wall = Instant::now();

    // 1) run per-task LLM builds + scoring
    let tasks = discover_tasks(cfg.bench_root)?;
    let runner = TaskRunner::new(
        PathBuf::from(cfg.bench_root),
        SpacetimeRustPublisher,
        DotnetPublisher,
        TypeScriptPublisher,
    );
    let lang_name = cfg.lang.as_str();
    let buf = match cfg.lang {
        Lang::CSharp => bench_csharp_concurrency(),
        _ => bench_concurrency(),
    };

    let results: Vec<(TaskPaths, Result<RunOutcome, RunOneError>)> =
        futures::stream::iter(tasks.into_iter().map(|task| {
            let runner = &runner;
            let route = cfg.route;
            let lang = cfg.lang;
            let lang_name = lang_name.to_string();
            let context = cfg.context;
            let hash = cfg.hash;
            let llm = cfg.llm;
            let host = cfg.host.clone();

            async move {
                let started = Utc::now();
                let run_cfg = RunContext {
                    lang_name: &lang_name,
                    lang,
                    route,
                    context,
                    hash,
                    llm,
                    host,
                };

                let res = runner.run_one(&task, &run_cfg).await;
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
            Err(RunOneError::WithOutput { msg, llm_output }) => {
                errs += 1;
                eprintln!("⚠️ task failed but continuing: {msg}");
                outcomes.push(build_fail_outcome(
                    &task,
                    lang_name,
                    cfg.route,
                    cfg.hash,
                    anyhow::anyhow!(msg),
                    Some(llm_output),
                ));
            }
            Err(RunOneError::Other(e)) => {
                errs += 1;
                eprintln!("⚠️ task failed but continuing: {e:?}");
                outcomes.push(build_fail_outcome(&task, lang_name, cfg.route, cfg.hash, e, None));
            }
        }
    }

    println!("[runner] completed batch: ok={} err={}", outcomes.len(), errs);

    if !outcomes.is_empty() {
        merge_task_runs(&cfg.details_path, cfg.mode, &outcomes)?;
    } else {
        eprintln!("[runner] no successful runs; not calling merge_task_runs");
    }

    println!(
        "✓ [{}] {}: total {}",
        lang_name,
        cfg.route.display_name,
        fmt_dur(total_wall.elapsed())
    );

    Ok(outcomes)
}

// run only selected tasks by selectors like 1/01/001 or t_001
pub async fn run_selected_for_model_async_for_lang(cfg: &BenchRunContext<'_>) -> Result<Vec<RunOutcome>> {
    let total_wall = Instant::now();

    let wanted: HashSet<String> = cfg
        .selectors
        .iter()
        .flat_map(|s| s.iter())
        .map(|s| normalize_task_selector(s.as_str()))
        .collect::<Result<_>>()?;

    let tasks = discover_tasks(cfg.bench_root)?;
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

    let runner = TaskRunner::new(
        PathBuf::from(cfg.bench_root),
        SpacetimeRustPublisher,
        DotnetPublisher,
        TypeScriptPublisher,
    );
    let lang_name = cfg.lang.as_str();
    let buf = match cfg.lang {
        Lang::CSharp => bench_csharp_concurrency(),
        _ => bench_concurrency(),
    };

    let results: Vec<(TaskPaths, Result<RunOutcome, RunOneError>)> =
        futures::stream::iter(selected.into_iter().map(|task| {
            let runner = &runner;
            let route = cfg.route;
            let lang = cfg.lang;
            let lang_name = lang_name.to_string();
            let context = cfg.context;
            let hash = cfg.hash;
            let llm = cfg.llm;

            async move {
                let started = Utc::now();
                let run_cfg = RunContext {
                    lang_name: &lang_name,
                    lang,
                    route,
                    context,
                    hash,
                    llm,
                    host: cfg.host.clone(),
                };

                let res = runner.run_one(&task, &run_cfg).await;
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
                    cfg.route,
                    cfg.hash,
                    anyhow::anyhow!(msg),
                    Some(llm_output),
                ));
            }
            Err(RunOneError::Other(e)) => {
                errs += 1;
                eprintln!("⚠️ task failed but continuing: {e:?}");
                outcomes.push(build_fail_outcome(&task, lang_name, cfg.route, cfg.hash, e, None));
            }
        }
    }

    if !outcomes.is_empty() {
        merge_task_runs(&cfg.details_path, cfg.mode, &outcomes)?;
    }

    println!(
        "✓ [{}] {}: total {} (err={})",
        lang_name,
        cfg.route.display_name,
        fmt_dur(total_wall.elapsed()),
        errs
    );
    Ok(outcomes)
}

pub async fn run_selected_or_all_for_model_async_for_lang(ctx: &BenchRunContext<'_>) -> Result<Vec<RunOutcome>> {
    if let Some(sels) = ctx.selectors {
        if !sels.is_empty() {
            let sel_cfg = BenchRunContext {
                bench_root: ctx.bench_root,
                mode: ctx.mode,
                hash: ctx.hash,
                route: ctx.route,
                context: ctx.context,
                llm: ctx.llm,
                lang: ctx.lang,
                selectors: Option::from(sels),
                host: ctx.host.clone(),
                details_path: ctx.details_path.clone(),
            };
            return run_selected_for_model_async_for_lang(&sel_cfg).await;
        }
    }

    run_all_for_model_async_for_lang(ctx).await
}

pub async fn build_goldens_only_for_lang(
    host: Option<String>,
    bench_root: &Path,
    lang: Lang,
    selectors: Option<&[String]>,
) -> Result<()> {
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

    let runner = TaskRunner::new(
        PathBuf::from(bench_root),
        SpacetimeRustPublisher,
        DotnetPublisher,
        TypeScriptPublisher,
    );
    let lang_name = lang.as_str();
    let buf = match lang {
        Lang::CSharp => bench_csharp_concurrency(),
        _ => bench_concurrency(),
    };

    stream::iter(tasks.into_iter().map(|task| {
        let runner = &runner;
        let host_clone = host.clone();
        async move {
            let category = category_slug(&task.root);
            let task_id = task_slug(&task.root);
            let golden_db = sanitize_db_name(&format!("{}-{}-golden", category, task_id));
            let golden_src_text = load_golden_source(&task, lang)?;
            println!("→ [{}] build golden {} {}", lang_name, category, task_id);
            runner
                .publish_golden_only(lang, &category, &task_id, &golden_src_text, golden_db, host_clone)
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
                answers_typescript: task.join("answers/typescript/server"),
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

// TEST_CASE/answers/csharp.cs, TEST_CASE/rust.rs, and TEST_CASE/answers/typescript.ts
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
        Lang::TypeScript => {
            let p = task.root.join("answers").join("typescript.ts");
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

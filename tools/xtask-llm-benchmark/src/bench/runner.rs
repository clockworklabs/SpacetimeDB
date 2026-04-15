use anyhow::{anyhow, bail, Context, Result};
use chrono::Utc;
use futures::{stream, StreamExt};
use serde_json::json;
use spacetimedb_data_structures::map::{HashCollectionExt as _, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Instant;
use tokio::sync::Mutex;
use tokio::task;

use crate::bench::publishers::{DotnetPublisher, SpacetimeRustPublisher, TypeScriptPublisher};
use crate::bench::templates::materialize_project;
use crate::bench::types::{BenchRunContext, PublishParams, RunContext, RunOneError};
pub(crate) use crate::bench::types::{RunOutcome, TaskPaths};
use crate::bench::utils::{
    bench_concurrency, bench_csharp_concurrency, bench_rust_concurrency, category_slug, debug_llm, fmt_dur,
    print_llm_output, sanitize_db_name, task_slug, work_server_dir_scoped,
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
        let route_tag = sanitize_db_name(&cfg.route.display_name);
        let golden_db = sanitize_db_name(&format!("{}-{}-golden", category, task_id));
        let llm_db = sanitize_db_name(&format!("{}-{}-{}-llm", category, task_id, route_tag));

        let ctor = resolve_by_path(&task.root)?;
        let spec = ctor();

        let scorers = spec.scorers_for(cfg.lang, &route_tag, cfg.host.as_deref().unwrap_or("local"));
        let total_tasks = scorers.len();

        let prompt_builder = (spec.make_prompt)(cfg.lang);
        println!("→ [{}] {}: building prompt", cfg.lang_name, cfg.route.display_name);
        let prompt = prompt_builder.build_segmented(cfg.mode, cfg.context);

        println!("→ [{}] {}: calling provider", cfg.lang_name, cfg.route.display_name);
        let mut gen_start = Instant::now();
        let llm_result = {
            const MAX_ATTEMPTS: u32 = 3;
            // Slow models (Gemini 3.1 Pro, DeepSeek Reasoner) can take 8+ minutes on large contexts.
            let timeout_secs = match cfg.route.display_name.to_ascii_lowercase() {
                n if n.contains("gemini") || n.contains("deepseek") => 600,
                _ => 300,
            };
            let mut last_err: anyhow::Error = anyhow!("no attempts made");
            let mut result = None;
            for attempt in 1..=MAX_ATTEMPTS {
                gen_start = Instant::now();
                let r = tokio::time::timeout(
                    std::time::Duration::from_secs(timeout_secs),
                    cfg.llm.generate(cfg.route, &prompt),
                )
                .await;
                match r {
                    Ok(Ok(output)) => {
                        result = Some(output);
                        break;
                    }
                    Ok(Err(e)) => {
                        let msg = format!("{e:#}");
                        let retryable = msg.contains("timed out")
                            || msg.contains("429")
                            || msg.contains("502")
                            || msg.contains("503")
                            || msg.contains("504")
                            || msg.contains("rate limit");
                        if retryable && attempt < MAX_ATTEMPTS {
                            let delay = if msg.contains("429") || msg.contains("rate limit") {
                                60
                            } else {
                                30
                            };
                            eprintln!(
                                "⚠️ [{}/{}] provider error (attempt {}/{}), retrying in {delay}s: {}",
                                cfg.lang_name, cfg.route.display_name, attempt, MAX_ATTEMPTS, msg
                            );
                            tokio::time::sleep(std::time::Duration::from_secs(delay)).await;
                            last_err = e;
                        } else {
                            return Err(RunOneError::Other(e));
                        }
                    }
                    Err(_) => {
                        if attempt < MAX_ATTEMPTS {
                            eprintln!(
                                "⚠️ [{}/{}] LLM call timed out after {timeout_secs}s (attempt {}/{}), retrying in 30s",
                                cfg.lang_name, cfg.route.display_name, attempt, MAX_ATTEMPTS
                            );
                            tokio::time::sleep(std::time::Duration::from_secs(30)).await;
                            last_err = anyhow!("LLM call timed out after {timeout_secs}s");
                        } else {
                            return Err(RunOneError::Other(anyhow!(
                                "LLM call timed out after {timeout_secs}s ({MAX_ATTEMPTS} attempts)"
                            )));
                        }
                    }
                }
            }
            result.ok_or_else(|| RunOneError::Other(last_err))?
        };
        let generation_duration_ms = Some(gen_start.elapsed().as_millis() as u64);
        let input_tokens = llm_result.input_tokens;
        let output_tokens = llm_result.output_tokens;
        let llm_output = llm_result.text;

        if debug_llm() {
            print_llm_output(&cfg.route.display_name, &task_id, &llm_output);
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
            input_tokens,
            output_tokens,
            generation_duration_ms,
            started_at: Some(started),
            finished_at: Some(finished),
        })
    }
}

/// Partition task results into (good outcomes with LLM output, tasks to retry).
/// Tasks where the LLM never responded (provider error) are returned for retry.
/// Tasks where the LLM responded but the code failed to compile/score are kept as outcomes.
fn partition_results(
    results: Vec<(TaskPaths, Result<RunOutcome, RunOneError>)>,
    lang_name: &str,
    route: &ModelRoute,
    hash: &str,
) -> (Vec<RunOutcome>, Vec<TaskPaths>) {
    let mut good = Vec::new();
    let mut retry = Vec::new();

    for (task, r) in results {
        match r {
            Ok(v) => {
                if v.llm_output.is_some() {
                    good.push(v);
                } else {
                    // Outcome with no LLM output = provider error recorded as result
                    retry.push(task);
                }
            }
            Err(RunOneError::WithOutput { msg, llm_output }) => {
                // LLM responded but something else failed — keep as a result
                eprintln!("\u{26a0}\u{fe0f} task failed but has output: {msg}");
                good.push(build_fail_outcome(
                    &task,
                    lang_name,
                    route,
                    hash,
                    anyhow::anyhow!(msg),
                    Some(llm_output),
                ));
            }
            Err(RunOneError::Other(e)) => {
                // No output at all — provider error, retry
                eprintln!("\u{26a0}\u{fe0f} provider error, will retry: {e:?}");
                retry.push(task);
            }
        }
    }

    (good, retry)
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
        Lang::Rust => bench_rust_concurrency(),
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
                    mode: cfg.mode,
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

    let (mut good, mut retry_tasks) = partition_results(results, lang_name, cfg.route, cfg.hash);

    // Retry provider-error tasks until all pass or none make progress
    const MAX_RETRY_ROUNDS: usize = 3;
    for round in 1..=MAX_RETRY_ROUNDS {
        if retry_tasks.is_empty() {
            break;
        }
        eprintln!(
            "[runner] retry round {}/{}: {} tasks with provider errors",
            round,
            MAX_RETRY_ROUNDS,
            retry_tasks.len()
        );
        tokio::time::sleep(std::time::Duration::from_secs(30)).await;

        let retry_results: Vec<(TaskPaths, Result<RunOutcome, RunOneError>)> =
            futures::stream::iter(retry_tasks.drain(..).map(|task| {
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
                        mode: cfg.mode,
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

        let (new_good, still_failing) = partition_results(retry_results, lang_name, cfg.route, cfg.hash);

        if new_good.is_empty() && !still_failing.is_empty() {
            // No progress — provider is likely down. Give up on these tasks.
            eprintln!(
                "[runner] no tasks recovered in retry round {} — provider may be down, dropping {} tasks",
                round,
                still_failing.len()
            );
            break;
        }

        good.extend(new_good);
        if still_failing.is_empty() {
            retry_tasks = still_failing;
            break;
        }
        retry_tasks = still_failing;
    }

    let dropped = retry_tasks.len();
    if dropped > 0 {
        eprintln!(
            "[runner] {} tasks still failing after retries — excluded from upload",
            dropped
        );
    }

    println!("[runner] completed batch: {} uploadable results", good.len());

    if cfg.dry_run {
        eprintln!("[dry-run] skipping upload ({} outcomes)", good.len());
    } else if !good.is_empty() {
        let analysis = match crate::bench::analysis::run_analysis(
            &good,
            cfg.lang.as_str(),
            cfg.mode,
            &cfg.route.display_name,
            cfg.bench_root,
            cfg.llm,
        )
        .await
        {
            Ok(Some(text)) => {
                eprintln!("[runner] generated analysis for {}/{}", cfg.lang.as_str(), cfg.mode);
                Some(text)
            }
            Ok(None) => None,
            Err(e) => {
                eprintln!("[runner] analysis failed (non-fatal): {e}");
                None
            }
        };

        if let Some(ref api) = cfg.api_client {
            api.upload_batch(cfg.mode, &good, analysis.as_deref())?;
        } else {
            eprintln!("[runner] no API client configured; skipping upload");
        }
    } else {
        eprintln!("[runner] no uploadable results; skipping upload");
    }

    println!(
        "\u{2713} [{}] {}: total {}",
        lang_name,
        cfg.route.display_name,
        fmt_dur(total_wall.elapsed())
    );

    Ok(good)
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
            let name = t
                .root
                .file_name()
                .and_then(|x: &std::ffi::OsStr| x.to_str())
                .unwrap_or("");
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
        Lang::Rust => bench_rust_concurrency(),
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
                    mode: cfg.mode,
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

    let (mut good, retry_tasks) = partition_results(results, lang_name, cfg.route, cfg.hash);

    // Retry provider-error tasks until all pass or none make progress
    let mut dropped = 0usize;
    {
        const MAX_RETRY_ROUNDS: usize = 3;
        let mut pending = retry_tasks;
        for round in 1..=MAX_RETRY_ROUNDS {
            if pending.is_empty() {
                break;
            }
            eprintln!(
                "[runner] retry round {}/{}: {} tasks with provider errors",
                round,
                MAX_RETRY_ROUNDS,
                pending.len()
            );
            tokio::time::sleep(std::time::Duration::from_secs(30)).await;

            let retry_results: Vec<(TaskPaths, Result<RunOutcome, RunOneError>)> =
                futures::stream::iter(pending.drain(..).map(|task| {
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
                            mode: cfg.mode,
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

            let (new_good, still_failing) = partition_results(retry_results, lang_name, cfg.route, cfg.hash);

            if new_good.is_empty() && !still_failing.is_empty() {
                eprintln!(
                    "[runner] no tasks recovered in retry round {} — provider may be down, dropping {} tasks",
                    round,
                    still_failing.len()
                );
                dropped = still_failing.len();
                break;
            }

            good.extend(new_good);
            pending = still_failing;
        }
        dropped += pending.len();
    }

    if dropped > 0 {
        eprintln!(
            "[runner] {} tasks still failing after retries — excluded from upload",
            dropped
        );
    }

    if cfg.dry_run {
        eprintln!("[dry-run] skipping upload ({} outcomes)", good.len());
    } else if !good.is_empty() {
        let analysis = match crate::bench::analysis::run_analysis(
            &good,
            cfg.lang.as_str(),
            cfg.mode,
            &cfg.route.display_name,
            cfg.bench_root,
            cfg.llm,
        )
        .await
        {
            Ok(Some(text)) => {
                eprintln!("[runner] generated analysis for {}/{}", cfg.lang.as_str(), cfg.mode);
                Some(text)
            }
            Ok(None) => None,
            Err(e) => {
                eprintln!("[runner] analysis failed (non-fatal): {e}");
                None
            }
        };

        if let Some(ref api) = cfg.api_client {
            api.upload_batch(cfg.mode, &good, analysis.as_deref())?;
        } else {
            eprintln!("[runner] no API client configured; skipping upload");
        }
    }

    println!(
        "\u{2713} [{}] {}: total {}",
        lang_name,
        cfg.route.display_name,
        fmt_dur(total_wall.elapsed()),
    );
    Ok(good)
}

pub async fn run_selected_or_all_for_model_async_for_lang(ctx: &BenchRunContext<'_>) -> Result<Vec<RunOutcome>> {
    if let Some(sels) = ctx.selectors
        && !sels.is_empty()
    {
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
            api_client: ctx.api_client.clone(),
            dry_run: ctx.dry_run,
        };
        return run_selected_for_model_async_for_lang(&sel_cfg).await;
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
                let name = t
                    .root
                    .file_name()
                    .and_then(|x: &std::ffi::OsStr| x.to_str())
                    .unwrap_or("");
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
        Lang::Rust => bench_rust_concurrency(),
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
        input_tokens: None,
        output_tokens: None,
        generation_duration_ms: None,
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
// "t_000_empty_reducers" | "t_001_basic_tables" -> accepted as-is (full task dir name)
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
        // Full task dir name: t_000_empty_reducers, t_001_basic_tables, etc.
        if rest.chars().next().is_some_and(|c| c.is_ascii_digit())
            && rest.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        {
            return Ok(s);
        }
        bail!("invalid task selector: {raw}");
    }
    if s.chars().all(|c| c.is_ascii_digit()) {
        let n: u32 = s.parse()?;
        return Ok(format!("t_{:03}", n));
    }
    bail!("invalid task selector: {raw}")
}

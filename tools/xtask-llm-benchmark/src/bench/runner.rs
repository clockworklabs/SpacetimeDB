use anyhow::{anyhow, bail, Context, Result};
use chrono::Utc;
use futures::{stream, StreamExt, TryStreamExt};
use serde_json::json;
use spacetimedb_data_structures::map::{HashCollectionExt as _, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use tokio::task;

use crate::bench::publishers::{DotnetPublisher, PublishedDatabase, SpacetimeRustPublisher, TypeScriptPublisher};
use crate::bench::records::BatchRecord;
use crate::bench::templates::materialize_project;
use crate::bench::types::{BenchRunContext, PublishParams, RunContext, RunOneError};
pub(crate) use crate::bench::types::{RunOutcome, TaskPaths};
use crate::bench::utils::{
    bench_concurrency, bench_csharp_concurrency, bench_rust_concurrency, category_slug, debug_llm, fmt_dur,
    golden_db_name, print_llm_output, run_scope_tag, sanitize_db_name, task_slug, work_server_dir_scoped,
};
use crate::bench::Publisher;
use crate::eval::scorers::call_reducer_json_out;
use crate::eval::{Lang, ScoreDetails, Scorer};
use crate::generated::resolve_by_path;
use crate::llm::model_routes::ModelRoute;

pub struct TaskRunner {
    pub bench_root: PathBuf,
    pub rust_publisher: SpacetimeRustPublisher,
    pub cs_publisher: DotnetPublisher,
    pub ts_publisher: TypeScriptPublisher,
}

struct PendingRetry {
    task: TaskPaths,
    error: anyhow::Error,
}

async fn collect_recorded_attempts<F, T, R>(
    attempts: impl IntoIterator<Item = F>,
    concurrency: usize,
    mut record: impl FnMut(T) -> Result<R>,
) -> Result<Vec<R>>
where
    F: std::future::Future<Output = T>,
{
    let stopped = AtomicBool::new(false);
    stream::iter(attempts)
        .take_while(|_| std::future::ready(!stopped.load(Ordering::Relaxed)))
        .buffer_unordered(concurrency)
        .map(|attempt| {
            let result = record(attempt);
            if result.is_err() {
                stopped.store(true, Ordering::Relaxed);
            }
            result
        })
        // Drain in-flight attempts before returning an error. Do not lose completed work.
        .collect::<Vec<_>>()
        .await
        .into_iter()
        .collect()
}

fn record_task_result(
    record: &mut BatchRecord,
    cfg: &BenchRunContext<'_>,
    task: TaskPaths,
    result: Result<RunOutcome, RunOneError>,
    started: chrono::DateTime<Utc>,
) -> Result<(TaskPaths, Result<RunOutcome, RunOneError>)> {
    let result = match result {
        Err(RunOneError::WithOutput { msg, llm_output }) => Ok(build_fail_outcome(
            &task,
            cfg.lang.as_str(),
            cfg.route,
            cfg.hash,
            anyhow!(msg),
            llm_output,
        )),
        other => other,
    }
    .map(|mut outcome| {
        outcome.started_at.get_or_insert(started);
        outcome
    });
    // Save before propagating an infrastructure failure; the collector drains started tasks.
    // A record write failure also aborts the batch before any website upload.
    record.record(&task.root.to_string_lossy(), &result)?;
    match result {
        Err(RunOneError::Infrastructure { msg, llm_output, .. }) => {
            let output_note = llm_output
                .as_deref()
                .map(|output| format!("; generated output retained in work directory ({} bytes)", output.len()))
                .unwrap_or_default();
            bail!("benchmark infrastructure failure: {msg}{output_note}");
        }
        other => Ok((task, other)),
    }
}

/// Build the golden databases and return handles that keep their owning CLI
/// sessions alive until scoring has finished.
pub async fn ensure_goldens_built_once(
    host: Option<String>,
    bench_root: &Path,
    lang: Lang,
    selectors: Option<&[String]>,
    golden_scope: &str,
) -> Result<Vec<PublishedDatabase>> {
    build_goldens_only_for_lang(host, bench_root, lang, selectors, golden_scope).await
}

fn with_build_cleanup<T>(project: &Path, lang: Lang, publish: impl FnOnce() -> Result<T>) -> Result<T> {
    let result = publish();
    // Only task-local outputs: never follow the shared Cargo target override or shared caches.
    let directories: &[&str] = match lang {
        Lang::Rust => &["target"],
        Lang::CSharp => &["bin", "obj", ".nuget"],
        Lang::TypeScript => &["node_modules", "dist"],
    };
    for directory in directories {
        let path = project.join(directory);
        if let Err(error) = fs::remove_dir_all(&path)
            && error.kind() != std::io::ErrorKind::NotFound
        {
            eprintln!("[cleanup] failed to remove build output {}: {error}", path.display());
        }
    }
    result
}

async fn publish_async(
    publisher: impl Publisher + 'static,
    lang: Lang,
    host_url: String,
    wdir: PathBuf,
    db: String,
    clear_database: bool,
) -> Result<PublishedDatabase> {
    task::spawn_blocking(move || {
        with_build_cleanup(&wdir, lang, || publisher.publish(&host_url, &wdir, &db, clear_database))
    })
    .await?
}

async fn delete_database_async(database: PublishedDatabase) -> Result<()> {
    task::spawn_blocking(move || database.delete()).await??;
    Ok(())
}

async fn score_async(
    scorers: Vec<Box<dyn Scorer>>,
    output: String,
    database: Option<PublishedDatabase>,
) -> Result<(Vec<(&'static str, ScoreDetails)>, String, Option<PublishedDatabase>), RunOneError> {
    let failure_output = output.clone();
    task::spawn_blocking(move || {
        // Keep the database alive if the awaiting task is cancelled while scoring.
        let scores = scorers.iter().map(|s| (s.id(), s.score(&output))).collect();
        (scores, output, database)
    })
    .await
    .map_err(|error| RunOneError::Infrastructure {
        msg: format!("scoring worker failed: {error:#}"),
        llm_output: Some(failure_output),
        provider_error: None,
        completed_outcome: None,
    })
}

pub async fn delete_databases(databases: Vec<PublishedDatabase>) -> Result<()> {
    stream::iter(databases.into_iter().map(delete_database_async))
        .buffer_unordered(8)
        .try_collect::<Vec<_>>()
        .await?;
    Ok(())
}

async fn ensure_server_healthy(host: Option<&str>) -> Result<()> {
    let Some(host) = host else {
        return Ok(());
    };
    let ping_url = format!("{}/v1/ping", host.trim_end_matches('/'));
    let response = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()?
        .get(&ping_url)
        .send()
        .await
        .with_context(|| format!("benchmark server at {host} is unavailable; refusing to upload results"))?;
    if !response.status().is_success() {
        bail!(
            "benchmark server health check at {ping_url} returned {}; refusing to upload results",
            response.status()
        );
    }
    Ok(())
}

fn provider_retry_delay(error: &anyhow::Error) -> Option<u64> {
    use crate::llm::clients::{
        http::HttpResponseError,
        oa_compat::{ProviderRefusal, ProviderResponseError},
    };
    let status_delay = |status| match status {
        429 => Some(60),
        502..=504 => Some(30),
        _ => None,
    };
    if error.downcast_ref::<ProviderRefusal>().is_some() {
        return None;
    }
    if let Some(response) = error.downcast_ref::<HttpResponseError>() {
        return status_delay(u64::from(response.status));
    }
    // Inspect private fields for retry decisions, never for console output.
    let message = if let Some(response) = error.downcast_ref::<ProviderResponseError>() {
        if let Some(code) = response.error.code.as_ref().and_then(|code| {
            code.as_u64()
                .or_else(|| code.as_str().and_then(|value| value.parse().ok()))
        }) {
            return status_delay(code);
        }
        response
            .error
            .message
            .as_deref()
            .unwrap_or_default()
            .to_ascii_lowercase()
    } else {
        format!("{error:#}").to_ascii_lowercase()
    };
    if message.contains("429") || message.contains("rate limit") {
        Some(60)
    } else if message.contains("timed out") || ["502", "503", "504"].iter().any(|status| message.contains(status)) {
        Some(30)
    } else {
        None
    }
}

fn is_code_publish_failure(message: &str, source_root: &Path, lang: Lang) -> bool {
    // Unknown publish failures are unscored. A healthy ping does not prove a valid build environment.
    let message = message.replace('\\', "/").to_ascii_lowercase();
    // The candidate built, but its schema cannot migrate the task's existing data.
    if message.contains("aborting publish due to required manual migration") {
        return true;
    }
    if [
        "error[e0463]",
        "error[e0460]",
        "error[e0514]",
        "error cs0006",
        "error cs0009",
        "error cs0012",
        "error cs0518",
        "error cs1705",
    ]
    .iter()
    .any(|code| message.contains(code))
    {
        return false;
    }
    let relative = match lang {
        Lang::Rust => "src/lib.rs",
        Lang::CSharp => "lib.cs",
        Lang::TypeScript => "src/index.ts",
    };
    let absolute = source_root
        .canonicalize()
        .or_else(|_| std::path::absolute(source_root))
        .unwrap_or_else(|_| source_root.to_path_buf())
        .join(relative)
        .to_string_lossy()
        .replace('\\', "/")
        .to_ascii_lowercase();
    let is_generated_location = |location: &str| {
        let location = location.trim().trim_start_matches("//?/").trim_start_matches("./");
        [relative, absolute.trim_start_matches("//?/")].iter().any(|path| {
            location
                .strip_prefix(path)
                .is_some_and(|rest| rest.starts_with(':') || rest.starts_with('('))
        })
    };
    let lines: Vec<_> = message.lines().map(str::trim).collect();
    let mut found = false;
    for (index, line) in lines.iter().enumerate() {
        let rust_error = matches!(lang, Lang::Rust) && (line.starts_with("error[e") || line.starts_with("error:"));
        if rust_error {
            // Cargo/rustc summaries add no source diagnostic of their own.
            if line.starts_with("error: could not compile") || line.starts_with("error: aborting due to") {
                continue;
            }
            let location = lines[index + 1..]
                .iter()
                .take_while(|line| !line.starts_with("error") && !line.starts_with("warning"))
                .find_map(|line| line.strip_prefix("--> "));
            if !location.is_some_and(is_generated_location) {
                return false;
            }
            found = true;
        } else if (matches!(lang, Lang::CSharp) && (line.contains(": error cs") || line.contains(": error stdb")))
            || (matches!(lang, Lang::TypeScript) && line.contains(": error ts"))
        {
            if !is_generated_location(line) {
                return false;
            }
            found = true;
        }
    }
    found
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

    pub async fn publish_golden_only(&self, params: PublishParams<'_>) -> Result<PublishedDatabase> {
        self.publish(params, "golden").await
    }

    async fn publish_llm(&self, params: PublishParams<'_>) -> Result<PublishedDatabase> {
        self.publish(params, "llm").await
    }

    async fn publish(&self, params: PublishParams<'_>, phase: &str) -> Result<PublishedDatabase> {
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
        let database = match params.lang {
            Lang::Rust => {
                publish_async(
                    self.rust_publisher,
                    params.lang,
                    host_url,
                    wdir,
                    params.db_name,
                    params.clear_database,
                )
                .await?
            }
            Lang::CSharp => {
                publish_async(
                    self.cs_publisher,
                    params.lang,
                    host_url,
                    wdir,
                    params.db_name,
                    params.clear_database,
                )
                .await?
            }
            Lang::TypeScript => {
                publish_async(
                    self.ts_publisher,
                    params.lang,
                    host_url,
                    wdir,
                    params.db_name,
                    params.clear_database,
                )
                .await?
            }
        };

        Ok(database)
    }

    pub async fn run_one(&self, task: &TaskPaths, cfg: &RunContext<'_>) -> Result<RunOutcome, RunOneError> {
        if cfg.lang != Lang::CSharp {
            return self.run_one_prepared(task, cfg).await;
        }
        // A C# reference hosts a .NET runtime. Keep only references for active tasks loaded.
        let scope = run_scope_tag(cfg.mode, cfg.route.vendor.slug(), &cfg.route.api_model);
        let references = build_goldens_only_for_lang(
            cfg.host.clone(),
            &self.bench_root,
            cfg.lang,
            Some(&[task_slug(&task.root)]),
            &scope,
        )
        .await
        .map_err(|error| RunOneError::Infrastructure {
            msg: format!("reference setup failed for {}: {error:#}", task.root.display()),
            llm_output: None,
            provider_error: None,
            completed_outcome: None,
        })?;
        let result = self.run_one_prepared(task, cfg).await;
        finish_reference_cleanup(result, delete_databases(references).await)
    }

    async fn run_one_prepared(&self, task: &TaskPaths, cfg: &RunContext<'_>) -> Result<RunOutcome, RunOneError> {
        let wall = Instant::now();
        let started = Utc::now();

        let category = category_slug(&task.root);
        let task_id = task_slug(&task.root);
        let route_tag = run_scope_tag(cfg.mode, cfg.route.vendor.slug(), &cfg.route.api_model);
        let golden_db = golden_db_name(&category, &task_id, &route_tag);
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
                        if let Some(delay) = provider_retry_delay(&e).filter(|_| attempt < MAX_ATTEMPTS) {
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
        let mut llm_output = llm_result.text;

        let migration_setup =
            load_migration_setup_source(task, cfg.lang).map_err(|error| RunOneError::Infrastructure {
                msg: format!("load migration setup failed: {error:#}"),
                llm_output: Some(llm_output.clone()),
                provider_error: None,
                completed_outcome: None,
            })?;

        if debug_llm() {
            print_llm_output(&cfg.route.display_name, &task_id, &llm_output);
        }

        let setup_database: Result<Option<PublishedDatabase>> = if let Some(setup_source) = migration_setup.as_deref() {
            let setup_result = self
                .publish(
                    PublishParams {
                        lang: cfg.lang,
                        category: &category,
                        task_id: &task_id,
                        route_tag: &route_tag,
                        source_text: setup_source,
                        db_name: llm_db.clone(),
                        host: cfg.host.clone(),
                        clear_database: true,
                    },
                    "llm-setup",
                )
                .await;
            match setup_result {
                Ok(database) => {
                    call_reducer_json_out(&llm_db, "seed", &[], Some(cfg.host.as_deref().unwrap_or("local"))).map_err(
                        |error| RunOneError::Infrastructure {
                            msg: format!("migration seed failed: {error}"),
                            llm_output: Some(llm_output.clone()),
                            provider_error: None,
                            completed_outcome: None,
                        },
                    )?;
                    Ok(Some(database))
                }
                Err(error) => Err(error),
            }
        } else {
            Ok(None)
        };

        let (mut published_database, publish_error) = match setup_database {
            Err(error) => {
                return Err(RunOneError::Infrastructure {
                    msg: format!("migration setup failed: {error:#}"),
                    llm_output: Some(llm_output),
                    provider_error: None,
                    completed_outcome: None,
                })
            }
            Ok(mut setup_database) => {
                let publish_result = self
                    .publish_llm(PublishParams {
                        lang: cfg.lang,
                        category: &category,
                        task_id: &task_id,
                        route_tag: &route_tag,
                        source_text: &llm_output,
                        db_name: llm_db.clone(),
                        host: cfg.host.clone(),
                        clear_database: migration_setup.is_none(),
                    })
                    .await;
                match publish_result {
                    Ok(database) => {
                        if let Some(setup_database) = setup_database.take() {
                            setup_database.relinquish();
                        }
                        (Some(database), None)
                    }
                    Err(error) => {
                        let source_root = work_server_dir_scoped(&category, &task_id, cfg.lang_name, "llm", &route_tag);
                        if !is_code_publish_failure(&format!("{error:#}"), &source_root, cfg.lang) {
                            return Err(RunOneError::Infrastructure {
                                msg: format!("publish failed without a source-code diagnostic: {error:#}"),
                                llm_output: Some(llm_output),
                                provider_error: None,
                                completed_outcome: None,
                            });
                        }
                        eprintln!(
                            "⚠️ publish failed for {}/{}/{}: {error:#}",
                            category, task_id, cfg.route.display_name
                        );
                        (None, Some(format!("{error:#}")))
                    }
                }
            }
        };

        if publish_error.is_some()
            && let Err(error) = ensure_server_healthy(cfg.host.as_deref()).await
        {
            return Err(RunOneError::Infrastructure {
                msg: format!("publish failed and the benchmark server is unavailable: {error:#}"),
                llm_output: Some(llm_output),
                provider_error: None,
                completed_outcome: None,
            });
        }

        let mut passed = 0usize;
        let mut partial_sum = 0f32;
        let mut scorer_details: HashMap<String, ScoreDetails> = HashMap::new();

        if publish_error.is_none() {
            println!("→ [{}] {}: scoring", cfg.lang_name, cfg.route.display_name);
            let (scores, output, database) = score_async(scorers, llm_output, published_database).await?;
            llm_output = output;
            published_database = database;
            for (id, r) in scores {
                if r.pass {
                    passed += 1;
                }
                partial_sum += r.partial.clamp(0.0, 1.0);
                scorer_details.insert(id.to_string(), r);
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

        if let Some(database) = published_database
            && let Err(error) = delete_database_async(database).await
        {
            return Err(RunOneError::Infrastructure {
                msg: format!("database cleanup failed for `{llm_db}`: {error:#}"),
                llm_output: Some(llm_output),
                provider_error: None,
                completed_outcome: None,
            });
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
            model_name: cfg.route.model_id(),
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
                work_server_dir_scoped(&category, &task_id, cfg.lang_name, "golden", &route_tag)
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

fn finish_reference_cleanup(
    result: Result<RunOutcome, RunOneError>,
    cleanup: Result<()>,
) -> Result<RunOutcome, RunOneError> {
    let Err(error) = cleanup else {
        return result;
    };
    let cleanup = format!("reference database cleanup failed: {error:#}");
    match result {
        Err(RunOneError::Other(error)) => Err(RunOneError::Infrastructure {
            msg: format!("{cleanup}; provider request also failed: {error:#}"),
            llm_output: None,
            provider_error: Some(error),
            completed_outcome: None,
        }),
        Err(RunOneError::Infrastructure {
            msg,
            llm_output,
            provider_error,
            completed_outcome,
        }) => Err(RunOneError::Infrastructure {
            msg: format!("{msg}; {cleanup}"),
            llm_output,
            provider_error,
            completed_outcome,
        }),
        Err(RunOneError::WithOutput { msg, llm_output }) => Err(RunOneError::Infrastructure {
            msg: format!("{msg}; {cleanup}"),
            llm_output: Some(llm_output),
            provider_error: None,
            completed_outcome: None,
        }),
        Ok(outcome) => Err(RunOneError::Infrastructure {
            msg: cleanup,
            llm_output: outcome.llm_output.clone(),
            provider_error: None,
            completed_outcome: Some(Box::new(outcome)),
        }),
    }
}

/// Partition task results into completed outcomes and retry candidates.
/// Tasks where the LLM never responded (provider/setup error) are returned for retry.
/// Tasks where the LLM responded but the code failed to compile/score are kept as outcomes.
fn partition_results(
    results: Vec<(TaskPaths, Result<RunOutcome, RunOneError>)>,
    lang_name: &str,
    route: &ModelRoute,
    hash: &str,
) -> Result<(Vec<RunOutcome>, Vec<PendingRetry>)> {
    let mut good = Vec::new();
    let mut retry = Vec::new();

    for (task, r) in results {
        match r {
            Ok(v) => good.push(v),
            Err(RunOneError::WithOutput { msg, llm_output }) => {
                // LLM responded but something else failed — keep as a result
                eprintln!("\u{26a0}\u{fe0f} task failed but has output: {msg}");
                good.push(build_fail_outcome(
                    &task,
                    lang_name,
                    route,
                    hash,
                    anyhow::anyhow!(msg),
                    llm_output,
                ));
            }
            Err(RunOneError::Infrastructure { msg, llm_output, .. }) => {
                let output_note = llm_output
                    .as_deref()
                    .map(|output| format!("; generated output retained in work directory ({} bytes)", output.len()))
                    .unwrap_or_default();
                bail!("benchmark infrastructure failure: {msg}{output_note}");
            }
            Err(RunOneError::Other(e)) => {
                // No output at all — provider error, retry
                eprintln!("\u{26a0}\u{fe0f} provider error, will retry: {e:?}");
                retry.push(PendingRetry { task, error: e });
            }
        }
    }

    Ok((good, retry))
}

fn finalize_batch(outcomes: Vec<RunOutcome>, pending: Vec<PendingRetry>) -> Result<Vec<RunOutcome>> {
    // A partial batch would change the score denominator. Reject it, not just the failed tasks.
    if let Some(first) = pending.first() {
        bail!(
            "{} tasks still have provider/setup errors after retries; refusing to upload the entire batch. First failure at {}: {:#}",
            pending.len(), first.task.root.display(), first.error
        );
    }
    Ok(outcomes)
}

fn dry_run_analysis_path(run_id: &str, lang_name: &str, mode: &str, route: &ModelRoute) -> PathBuf {
    let route_tag = sanitize_db_name(&route.display_name);
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("runs")
        .join(format!("run-{run_id}-{lang_name}-{mode}-{route_tag}-analysis.md"))
}

fn configured_analysis_path(lang_name: &str, mode: &str, route: &ModelRoute) -> Option<PathBuf> {
    let report_dir = std::env::var_os("LLM_BENCHMARK_REPORT_DIR").filter(|value| !value.is_empty())?;
    let route_tag = sanitize_db_name(&route.display_name);
    Some(PathBuf::from(report_dir).join(format!("{lang_name}-{mode}-{route_tag}.md")))
}

fn write_analysis_report(
    path: &Path,
    cfg: &BenchRunContext<'_>,
    outcomes: &[RunOutcome],
    analysis: Option<&str>,
) -> Result<()> {
    fs::create_dir_all(path.parent().unwrap_or_else(|| Path::new(".")))?;
    let report =
        crate::bench::analysis::build_report(outcomes, cfg.lang.as_str(), cfg.mode, &cfg.route.display_name, analysis);
    fs::write(path, report)?;
    Ok(())
}

async fn maybe_generate_analysis(cfg: &BenchRunContext<'_>, outcomes: &[RunOutcome]) -> Result<Option<String>> {
    let configured_path = configured_analysis_path(cfg.lang.as_str(), cfg.mode, cfg.route);
    let should_run = cfg.local_analysis || configured_path.is_some() || (!cfg.dry_run && cfg.api_client.is_some());

    if !should_run {
        return Ok(None);
    }

    let analysis = crate::bench::analysis::run_analysis(
        outcomes,
        cfg.lang.as_str(),
        cfg.mode,
        &cfg.route.display_name,
        cfg.context,
        cfg.bench_root,
        cfg.llm,
    )
    .await?;

    if cfg.dry_run
        && let Some(run_id) = cfg.dry_run_id.as_deref()
    {
        let path = dry_run_analysis_path(run_id, cfg.lang.as_str(), cfg.mode, cfg.route);
        match write_analysis_report(&path, cfg, outcomes, analysis.as_deref()) {
            Ok(()) => println!("Local analysis: {}", path.display()),
            Err(e) => eprintln!("[warn] failed to write local analysis: {e}"),
        }
    }

    if let Some(path) = configured_path {
        match write_analysis_report(&path, cfg, outcomes, analysis.as_deref()) {
            Ok(()) => println!("Analysis report: {}", path.display()),
            Err(e) => eprintln!("[warn] failed to write analysis report: {e}"),
        }
    }

    if cfg.dry_run {
        return Ok(None);
    }

    Ok(analysis)
}

async fn upload_batch_for_context(
    cfg: &BenchRunContext<'_>,
    outcomes: &[RunOutcome],
    analysis: Option<&str>,
) -> Result<()> {
    if let Some(api) = cfg.api_client.clone() {
        let mode = cfg.mode.to_string();
        let outcomes = outcomes.to_vec();
        let analysis = analysis.map(str::to_string);
        tokio::task::spawn_blocking(move || api.upload_batch(&mode, &outcomes, analysis.as_deref())).await??;
    } else {
        eprintln!("[runner] no API client configured; skipping upload");
    }

    Ok(())
}

pub async fn run_all_for_model_async_for_lang(cfg: &BenchRunContext<'_>) -> Result<Vec<RunOutcome>> {
    let total_wall = Instant::now();

    // 1) run per-task LLM builds + scoring
    let tasks = discover_tasks(cfg.bench_root)?;
    let mut record = BatchRecord::new(cfg, &tasks)?;
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

    let results: Vec<(TaskPaths, Result<RunOutcome, RunOneError>)> = collect_recorded_attempts(
        tasks.into_iter().map(|task| {
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

                let result = runner.run_one(&task, &run_cfg).await;
                (task, result, started)
            }
        }),
        buf,
        |(task, result, started)| record_task_result(&mut record, cfg, task, result, started),
    )
    .await?;

    let (mut outcomes, mut retry_tasks) = partition_results(results, lang_name, cfg.route, cfg.hash)?;

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

        let retry_results: Vec<(TaskPaths, Result<RunOutcome, RunOneError>)> = collect_recorded_attempts(
            retry_tasks.drain(..).map(|pending| {
                let runner = &runner;
                let route = cfg.route;
                let lang = cfg.lang;
                let lang_name = lang_name.to_string();
                let context = cfg.context;
                let hash = cfg.hash;
                let llm = cfg.llm;
                let host = cfg.host.clone();
                let task = pending.task;

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
                    let result = runner.run_one(&task, &run_cfg).await;
                    (task, result, started)
                }
            }),
            buf,
            |(task, result, started)| record_task_result(&mut record, cfg, task, result, started),
        )
        .await?;

        let (new_outcomes, still_failing) = partition_results(retry_results, lang_name, cfg.route, cfg.hash)?;

        if new_outcomes.is_empty() && !still_failing.is_empty() {
            // No progress — provider is likely down. Keep the failures and stop retrying.
            eprintln!(
                "[runner] no tasks recovered in retry round {} — provider may be down, keeping {} failures",
                round,
                still_failing.len()
            );
            retry_tasks = still_failing;
            break;
        }

        outcomes.extend(new_outcomes);
        if still_failing.is_empty() {
            retry_tasks = still_failing;
            break;
        }
        retry_tasks = still_failing;
    }

    let outcomes = finalize_batch(outcomes, retry_tasks)?;

    println!("[runner] completed batch: {} results", outcomes.len());

    // A dead local server turns later publish failures into misleading zeroes.
    // Refuse to analyze or upload any batch that cannot pass a final health check.
    ensure_server_healthy(cfg.host.as_deref()).await?;
    record.complete()?;

    if cfg.dry_run {
        let _ = match maybe_generate_analysis(cfg, &outcomes).await {
            Ok(text) => text,
            Err(e) => {
                eprintln!("[runner] analysis failed (non-fatal): {e}");
                None
            }
        };
        eprintln!("[dry-run] skipping upload ({} outcomes)", outcomes.len());
    } else if !outcomes.is_empty() {
        let analysis = match maybe_generate_analysis(cfg, &outcomes).await {
            Ok(text) => text,
            Err(e) => {
                eprintln!("[runner] analysis failed (non-fatal): {e}");
                None
            }
        };
        upload_batch_for_context(cfg, &outcomes, analysis.as_deref()).await?;
    } else {
        eprintln!("[runner] no results; skipping upload");
    }

    println!(
        "\u{2713} [{}] {}: total {}",
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

    let mut record = BatchRecord::new(cfg, &selected)?;
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

    let results: Vec<(TaskPaths, Result<RunOutcome, RunOneError>)> = collect_recorded_attempts(
        selected.into_iter().map(|task| {
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

                let result = runner.run_one(&task, &run_cfg).await;
                (task, result, started)
            }
        }),
        buf,
        |(task, result, started)| record_task_result(&mut record, cfg, task, result, started),
    )
    .await?;

    let (mut outcomes, retry_tasks) = partition_results(results, lang_name, cfg.route, cfg.hash)?;

    // Retry provider-error tasks until all pass or none make progress
    let pending = {
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

            let retry_results: Vec<(TaskPaths, Result<RunOutcome, RunOneError>)> = collect_recorded_attempts(
                pending.drain(..).map(|pending_retry| {
                    let runner = &runner;
                    let route = cfg.route;
                    let lang = cfg.lang;
                    let lang_name = lang_name.to_string();
                    let context = cfg.context;
                    let hash = cfg.hash;
                    let llm = cfg.llm;
                    let host = cfg.host.clone();
                    let task = pending_retry.task;

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
                        let result = runner.run_one(&task, &run_cfg).await;
                        (task, result, started)
                    }
                }),
                buf,
                |(task, result, started)| record_task_result(&mut record, cfg, task, result, started),
            )
            .await?;

            let (new_outcomes, still_failing) = partition_results(retry_results, lang_name, cfg.route, cfg.hash)?;

            if new_outcomes.is_empty() && !still_failing.is_empty() {
                eprintln!(
                    "[runner] no tasks recovered in retry round {} — provider may be down, keeping {} failures",
                    round,
                    still_failing.len()
                );
                pending = still_failing;
                break;
            }

            outcomes.extend(new_outcomes);
            pending = still_failing;
        }
        pending
    };
    let outcomes = finalize_batch(outcomes, pending)?;

    ensure_server_healthy(cfg.host.as_deref()).await?;
    record.complete()?;

    if cfg.dry_run {
        let _ = match maybe_generate_analysis(cfg, &outcomes).await {
            Ok(text) => text,
            Err(e) => {
                eprintln!("[runner] analysis failed (non-fatal): {e}");
                None
            }
        };
        eprintln!("[dry-run] skipping upload ({} outcomes)", outcomes.len());
    } else if !outcomes.is_empty() {
        let analysis = match maybe_generate_analysis(cfg, &outcomes).await {
            Ok(text) => text,
            Err(e) => {
                eprintln!("[runner] analysis failed (non-fatal): {e}");
                None
            }
        };
        upload_batch_for_context(cfg, &outcomes, analysis.as_deref()).await?;
    }

    println!(
        "\u{2713} [{}] {}: total {}",
        lang_name,
        cfg.route.display_name,
        fmt_dur(total_wall.elapsed()),
    );
    Ok(outcomes)
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
            local_analysis: ctx.local_analysis,
            dry_run_id: ctx.dry_run_id.clone(),
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
    golden_scope: &str,
) -> Result<Vec<PublishedDatabase>> {
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

    let databases = stream::iter(tasks.into_iter().map(|task| {
        let runner = &runner;
        let host_clone = host.clone();
        async move {
            let category = category_slug(&task.root);
            let task_id = task_slug(&task.root);
            let golden_db = golden_db_name(&category, &task_id, golden_scope);
            let golden_src_text = load_golden_source(&task, lang)?;
            let migration_setup = load_migration_setup_source(&task, lang)?;
            println!("→ [{}] build golden {} {}", lang_name, category, task_id);
            if let Some(setup_source) = migration_setup {
                let setup_database = runner
                    .publish_golden_only(PublishParams {
                        lang,
                        category: &category,
                        task_id: &task_id,
                        route_tag: golden_scope,
                        source_text: &setup_source,
                        db_name: golden_db.clone(),
                        host: host_clone.clone(),
                        clear_database: true,
                    })
                    .await?;
                call_reducer_json_out(&golden_db, "seed", &[], Some(host_clone.as_deref().unwrap_or("local")))
                    .map_err(anyhow::Error::msg)?;
                let database = runner
                    .publish_golden_only(PublishParams {
                        lang,
                        category: &category,
                        task_id: &task_id,
                        route_tag: golden_scope,
                        source_text: &golden_src_text,
                        db_name: golden_db,
                        host: host_clone,
                        clear_database: false,
                    })
                    .await?;
                setup_database.relinquish();
                Ok(database)
            } else {
                runner
                    .publish_golden_only(PublishParams {
                        lang,
                        category: &category,
                        task_id: &task_id,
                        route_tag: golden_scope,
                        source_text: &golden_src_text,
                        db_name: golden_db,
                        host: host_clone,
                        clear_database: true,
                    })
                    .await
            }
        }
    }))
    .buffer_unordered(buf)
    .collect::<Vec<_>>()
    .await
    .into_iter()
    .collect::<Result<Vec<_>>>()?;

    println!("✓ [{}] goldens build/publish: complete", lang_name);
    Ok(databases)
}

/// Build each selected golden, publish the same source as a stand-in candidate,
/// and run the task's complete scorer set against the pair. This proves more
/// than compilation: the golden must also satisfy every behavior and parity
/// assertion used to grade model output.
pub async fn validate_goldens_for_lang(
    host: Option<String>,
    bench_root: &Path,
    lang: Lang,
    selectors: Option<&[String]>,
) -> Result<()> {
    let route_tag = "golden-validation";
    let shared_goldens = if lang == Lang::CSharp {
        Vec::new()
    } else {
        build_goldens_only_for_lang(host.clone(), bench_root, lang, selectors, route_tag).await?
    };

    let tasks = if let Some(sels) = selectors {
        let wanted: HashSet<String> = sels.iter().map(|s| normalize_task_selector(s)).collect::<Result<_>>()?;
        let filtered: Vec<TaskPaths> = discover_tasks(bench_root)?
            .into_iter()
            .filter(|task| {
                let name = task.root.file_name().and_then(|name| name.to_str()).unwrap_or("");
                wanted.iter().any(|wanted| name.starts_with(wanted))
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
    let host_url = host.as_deref().unwrap_or("local");
    let mut failures = Vec::new();

    // Scorers can mutate both databases, so validate one task at a time and
    // preserve the scorer ordering declared by its spec.
    for task in tasks {
        let golden_databases = if lang == Lang::CSharp {
            build_goldens_only_for_lang(
                host.clone(),
                bench_root,
                lang,
                Some(&[task_slug(&task.root)]),
                route_tag,
            )
            .await?
        } else {
            Vec::new()
        };
        let category = category_slug(&task.root);
        let task_id = task_slug(&task.root);
        let candidate_db = sanitize_db_name(&format!("{}-{}-{}-llm", category, task_id, route_tag));
        let golden_src_text = load_golden_source(&task, lang)?;
        let migration_setup = load_migration_setup_source(&task, lang)?;

        println!("→ [{}] validate golden {} {}", lang_name, category, task_id);
        let candidate_database = if let Some(setup_source) = migration_setup {
            let setup_database = runner
                .publish(
                    PublishParams {
                        lang,
                        category: &category,
                        task_id: &task_id,
                        route_tag,
                        source_text: &setup_source,
                        db_name: candidate_db.clone(),
                        host: host.clone(),
                        clear_database: true,
                    },
                    "golden-validation-setup",
                )
                .await?;
            call_reducer_json_out(&candidate_db, "seed", &[], Some(host_url)).map_err(anyhow::Error::msg)?;
            let database = runner
                .publish(
                    PublishParams {
                        lang,
                        category: &category,
                        task_id: &task_id,
                        route_tag,
                        source_text: &golden_src_text,
                        db_name: candidate_db,
                        host: host.clone(),
                        clear_database: false,
                    },
                    "golden-validation",
                )
                .await?;
            setup_database.relinquish();
            database
        } else {
            runner
                .publish(
                    PublishParams {
                        lang,
                        category: &category,
                        task_id: &task_id,
                        route_tag,
                        source_text: &golden_src_text,
                        db_name: candidate_db,
                        host: host.clone(),
                        clear_database: true,
                    },
                    "golden-validation",
                )
                .await?
        };

        let ctor = resolve_by_path(&task.root)?;
        let spec = ctor();
        let (scores, _, candidate_database) = score_async(
            spec.scorers_for(lang, route_tag, host_url),
            golden_src_text,
            Some(candidate_database),
        )
        .await?;
        for (id, details) in scores {
            if !details.pass {
                failures.push(format!(
                    "{}/{} [{}]: {}",
                    category,
                    task_id,
                    id,
                    serde_json::to_string(&details.notes).unwrap_or_else(|_| "<unprintable details>".to_string())
                ));
            }
        }
        delete_database_async(candidate_database.expect("candidate database retained during scoring")).await?;
        delete_databases(golden_databases).await?;
    }
    delete_databases(shared_goldens).await?;

    if !failures.is_empty() {
        bail!(
            "[{}] golden validation failed ({} scorer failures):\n{}",
            lang_name,
            failures.len(),
            failures.join("\n")
        );
    }

    println!("✓ [{}] goldens behavior/scorer validation: complete", lang_name);
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
    llm_output: String,
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

        model_name: route.model_id(),
        total_tests: 1,
        passed_tests: 0,

        llm_output: Some(llm_output),

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

fn load_migration_setup_source(task: &TaskPaths, lang: Lang) -> Result<Option<String>> {
    let file = match lang {
        Lang::Rust => "rust.rs",
        Lang::CSharp => "csharp.cs",
        Lang::TypeScript => "typescript.ts",
    };
    let path = task.root.join("setup").join(file);
    if !path.is_file() {
        return Ok(None);
    }
    fs::read_to_string(&path)
        .with_context(|| format!("read {}", path.display()))
        .map(Some)
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

#[cfg(test)]
mod build_cleanup_tests {
    use super::*;

    #[test]
    #[ignore = "requires local server binaries and packed C# SDK; no model calls or uploads"]
    fn csharp_references_are_scoped_to_successful_and_failed_tasks() {
        struct LocalProvider {
            host: String,
            reference: String,
            output: Option<String>,
        }
        #[async_trait::async_trait]
        impl crate::llm::LlmProvider for LocalProvider {
            async fn preflight_route(&self, _: &ModelRoute, _: bool) -> Result<()> {
                Ok(())
            }
            async fn generate(
                &self,
                _: &ModelRoute,
                _: &crate::llm::prompt::BuiltPrompt,
            ) -> Result<crate::llm::LlmOutput> {
                reqwest::Client::new()
                    .get(format!("{}/v1/database/{}/schema?version=9", self.host, self.reference))
                    .send()
                    .await?
                    .error_for_status()?;
                Ok(crate::llm::LlmOutput {
                    text: self.output.clone().context("injected provider failure")?,
                    input_tokens: None,
                    output_tokens: None,
                })
            }
        }
        let server = spacetimedb_guard::SpacetimeDbGuard::try_spawn_in_temp_data_dir().unwrap();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/benchmarks");
        let task = discover_tasks(&root)
            .unwrap()
            .into_iter()
            .find(|task| task.root.ends_with("basics/t_004_insert"))
            .unwrap();
        let route = ModelRoute::new(
            "local-fixture",
            crate::llm::types::Vendor::OpenAi,
            "local-fixture",
            None,
        );
        let scope = run_scope_tag("no_context", route.vendor.slug(), &route.api_model);
        let reference = golden_db_name("basics", "t_004_insert", &scope);
        let runner = TaskRunner::new(root, SpacetimeRustPublisher, DotnetPublisher, TypeScriptPublisher);
        let source = load_golden_source(&task, Lang::CSharp).unwrap();
        for output in [Some(source), Some("this is invalid C#".into()), None] {
            let provider = LocalProvider {
                host: server.host_url.clone(),
                reference: reference.clone(),
                output,
            };
            let cfg = RunContext {
                lang_name: "csharp",
                lang: Lang::CSharp,
                mode: "no_context",
                route: &route,
                context: "",
                hash: "local-validation",
                llm: &provider,
                host: Some(server.host_url.clone()),
            };
            let result = runtime.block_on(runner.run_one(&task, &cfg));
            match provider.output.as_deref() {
                None => assert!(matches!(result, Err(RunOneError::Other(_)))),
                Some("this is invalid C#") => {
                    let outcome = result.unwrap();
                    assert_eq!(outcome.passed_tests, 0);
                }
                Some(_) => {
                    let outcome = result.unwrap();
                    assert!(outcome.total_tests > 0);
                    assert_eq!(outcome.passed_tests, outcome.total_tests);
                }
            }
            let status = runtime.block_on(async {
                reqwest::Client::new()
                    .get(format!("{}/v1/database/{reference}/schema?version=9", server.host_url))
                    .send()
                    .await
                    .unwrap()
                    .status()
            });
            assert_eq!(status, reqwest::StatusCode::NOT_FOUND, "reference leaked after task");
        }
    }

    #[tokio::test]
    async fn failed_attempt_drains_started_work_without_starting_more_tasks() {
        let mut recorded = Vec::new();
        let error = collect_recorded_attempts(
            (0..4).map(|id| async move {
                if id == 1 {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
                id
            }),
            2,
            |id| {
                recorded.push(id);
                if id == 0 {
                    bail!("infrastructure failure");
                }
                Ok(id)
            },
        )
        .await
        .unwrap_err();
        assert_eq!(error.to_string(), "infrastructure failure");
        assert_eq!(
            recorded,
            [0, 1],
            "retain in-flight evidence, but stop scheduling new tasks"
        );
    }

    #[test]
    fn only_explicit_code_diagnostics_are_scored_as_publish_failures() {
        let root = std::env::temp_dir().join("benchmark-module");
        let root = root.as_path();
        for diagnostic in [
            "error[E0425]: cannot find value `missing` in this scope",
            "error: expected one of `!` or `::`, found `main`",
            "error: unexpected closing delimiter: `}`",
            "error: mismatched closing delimiter: `)`",
            "error: invalid suffix `words` for number literal",
            "error: this file contains an unclosed delimiter",
            "error: unknown start of token: \\",
        ] {
            let diagnostic =
                format!("{diagnostic}\n --> src/lib.rs:10:5\nerror: could not compile `test` due to 1 previous error");
            assert!(is_code_publish_failure(&diagnostic, root, Lang::Rust), "{diagnostic}");
        }
        for (lang, diagnostic) in [
            (
                Lang::CSharp,
                "Lib.cs(10,5): error CS0103: The name 'missing' does not exist",
            ),
            (
                Lang::TypeScript,
                "src/index.ts(2,3): error TS2304: Cannot find name 'missing'",
            ),
        ] {
            assert!(is_code_publish_failure(diagnostic, root, lang), "{diagnostic}");
        }
        for root in [root, Path::new("target/llm-runs/test-relative-module")] {
            let source = std::path::absolute(root).unwrap().join("Lib.cs");
            let diagnostic = format!("{}(10,5): error CS0103: missing name", source.display());
            assert!(is_code_publish_failure(&diagnostic, root, Lang::CSharp), "{diagnostic}");
        }
        let rejected_migration = "Errors occurred:\nAdding a column enabled to table widget requires a default value annotation\n\nAborting publish due to required manual migration.";
        for lang in [Lang::Rust, Lang::CSharp, Lang::TypeScript] {
            assert!(is_code_publish_failure(rejected_migration, root, lang));
        }
        for diagnostic in [
            "HTTP 503 Service Unavailable",
            "authentication token expired",
            "No space left on device",
            "failed to download package",
            "command dotnet publish exited with code 1",
            "migration setup failed",
            "unknown publish error",
            "error: linking with `cc` failed",
            "error: could not compile `test` due to 1 previous error",
            "error[E0463]: can't find crate for `std`\n --> src/lib.rs:1:1",
            "error[E0425]: cannot find value\n --> /dependencies/src/lib.rs:1:1",
            "Lib.cs(1,1): error CS0006: Metadata file could not be found",
            "/dependencies/Lib.cs(1,1): error CS0103: missing name",
            "node_modules/package/index.ts(1,1): error TS2304: missing name",
        ] {
            for lang in [Lang::Rust, Lang::CSharp, Lang::TypeScript] {
                assert!(!is_code_publish_failure(diagnostic, root, lang), "{diagnostic}");
            }
        }
    }

    #[test]
    fn private_provider_errors_keep_their_retry_policy() {
        use crate::llm::clients::oa_compat::OACompatResp;
        for (error, expected) in [
            (serde_json::json!({"message": "rate limit private detail"}), Some(60)),
            (
                serde_json::json!({"code": "429", "message": "private detail"}),
                Some(60),
            ),
            (serde_json::json!({"code": 503, "message": "private detail"}), Some(30)),
            (serde_json::json!({"code": 402, "message": "private detail"}), None),
            (
                serde_json::json!({"message": "request timed out: private detail"}),
                Some(30),
            ),
        ] {
            let response: OACompatResp = serde_json::from_value(serde_json::json!({"error": error})).unwrap();
            let error = response.first_text().unwrap_err().context("model test");
            assert_eq!(provider_retry_delay(&error), expected);
            assert!(!format!("{error:?}").contains("private detail"));
        }
        let refusal: OACompatResp = serde_json::from_value(serde_json::json!({
            "id": "gen-429", "choices": [{"finish_reason":"content_filter", "message":{"content":null}}]
        }))
        .unwrap();
        assert_eq!(provider_retry_delay(&refusal.first_text().unwrap_err()), None);
        for (status, expected) in [(429, Some(60)), (503, Some(30)), (402, None)] {
            let error = crate::llm::clients::http::HttpResponseError {
                status,
                body: "private detail".into(),
            }
            .into();
            assert_eq!(provider_retry_delay(&error), expected);
            assert!(!format!("{error:?}").contains("private detail"));
        }
    }

    #[test]
    fn provider_failures_reject_the_whole_batch_but_model_failures_remain_scores() {
        let task = || TaskPaths {
            root: PathBuf::from("basics/t_001"),
            answers_csharp: PathBuf::new(),
            answers_rust: PathBuf::new(),
            answers_typescript: PathBuf::new(),
        };
        let route = ModelRoute::new("Test", crate::llm::types::Vendor::OpenAi, "test", None);
        let (outcomes, pending) = partition_results(
            vec![
                (
                    task(),
                    Err(RunOneError::WithOutput {
                        msg: "compile failed".into(),
                        llm_output: "invalid code".into(),
                    }),
                ),
                (task(), Err(RunOneError::Other(anyhow::anyhow!("402 Payment Required")))),
                (task(), Err(RunOneError::Other(
                    serde_json::from_value::<crate::llm::clients::oa_compat::OACompatResp>(json!({
                        "choices": [{"finish_reason": "content_filter", "message": {"content": null, "refusal": "provider refusal"}}]
                    })).unwrap().first_text().unwrap_err()
                ))),
            ],
            "rust",
            &route,
            "test",
        )
        .unwrap();
        assert_eq!(outcomes.len(), 1);
        assert_eq!(pending.len(), 2, "refusals and provider failures are not scored zeroes");
        assert_eq!(outcomes[0].passed_tests, 0);
        assert!(finalize_batch(outcomes.clone(), Vec::new()).is_ok());
        let error = finalize_batch(outcomes, pending).unwrap_err().to_string();
        assert!(error.contains("refusing to upload"), "{error}");
        assert!(error.contains("402 Payment Required"), "{error}");
        assert!(finalize_batch(
            Vec::new(),
            vec![PendingRetry {
                task: task(),
                error: anyhow::anyhow!("provider unavailable"),
            }]
        )
        .is_err());
    }

    #[tokio::test]
    async fn scorer_panic_preserves_generated_code_as_unscored_evidence() {
        struct PanickingScorer;
        impl Scorer for PanickingScorer {
            fn id(&self) -> &'static str {
                "panic"
            }
            fn score(&self, _: &str) -> ScoreDetails {
                panic!("test scorer failure")
            }
        }
        let error = score_async(vec![Box::new(PanickingScorer)], "generated code".into(), None)
            .await
            .err()
            .unwrap();
        match error {
            RunOneError::Infrastructure { msg, llm_output, .. } => {
                assert!(msg.contains("test scorer failure"));
                assert_eq!(llm_output.as_deref(), Some("generated code"));
            }
            other => panic!("expected unscored infrastructure error: {other:?}"),
        }
    }

    #[test]
    fn reference_cleanup_failure_preserves_unscored_evidence() {
        let completed: RunOutcome = serde_json::from_value(json!({
            "hash": "test", "task": "a", "lang": "csharp", "model_name": "test", "vendor": "test",
            "golden_published": true, "passed_tests": 1, "total_tests": 2, "llm_output": "candidate code",
            "input_tokens": 123, "scorer_details": {"parity": {"pass": false, "partial": 0.5, "notes": {"difference": "retained"}}}
        })).unwrap();
        let expected = serde_json::to_value(&completed).unwrap();
        match finish_reference_cleanup(Ok(completed), Err(anyhow!("delete failed"))) {
            Err(RunOneError::Infrastructure { completed_outcome, .. }) => {
                assert_eq!(serde_json::to_value(completed_outcome.unwrap()).unwrap(), expected);
            }
            other => panic!("completed scoring lost after cleanup failure: {other:?}"),
        }
        let failed_code = Err(RunOneError::WithOutput {
            msg: "compile failed".into(),
            llm_output: "candidate code".into(),
        });
        match finish_reference_cleanup(failed_code, Err(anyhow!("delete failed"))) {
            Err(RunOneError::Infrastructure { msg, llm_output, .. }) => {
                assert!(msg.contains("compile failed") && msg.contains("delete failed"));
                assert_eq!(llm_output.as_deref(), Some("candidate code"));
            }
            other => panic!("expected unscored cleanup failure: {other:?}"),
        }
        let refusal = serde_json::from_value::<crate::llm::clients::oa_compat::OACompatResp>(json!({
            "choices": [{"finish_reason":"content_filter", "message":{"content":null,"refusal":"private detail"}}]
        }))
        .unwrap()
        .first_text()
        .unwrap_err();
        match finish_reference_cleanup(Err(RunOneError::Other(refusal)), Err(anyhow!("delete failed"))) {
            Err(RunOneError::Infrastructure {
                msg, provider_error, ..
            }) => {
                assert!(msg.contains("delete failed"));
                assert!(!msg.contains("private detail"));
                let error = provider_error.unwrap();
                let evidence = error
                    .downcast_ref::<crate::llm::clients::oa_compat::ProviderRefusal>()
                    .unwrap();
                assert_eq!(evidence.refusal.as_deref(), Some("private detail"));
            }
            other => panic!("cleanup failure must stop the batch: {other:?}"),
        }
    }

    #[test]
    fn slow_scoring_does_not_block_health_checks() {
        use std::io::{Read, Write};
        use std::net::TcpListener;
        use std::sync::mpsc;

        struct SlowScorer(mpsc::Receiver<()>);
        impl Scorer for SlowScorer {
            fn id(&self) -> &'static str {
                "slow"
            }
            fn score(&self, output: &str) -> ScoreDetails {
                assert_eq!(output, "candidate");
                ScoreDetails {
                    pass: self.0.recv_timeout(Duration::from_secs(8)).is_ok(),
                    partial: 0.5,
                    notes: json!({"preserved": true}),
                }
            }
        }

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let host = format!("http://{}", listener.local_addr().unwrap());
        listener.set_nonblocking(true).unwrap();
        let server = std::thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(15);
            loop {
                match listener.accept() {
                    Ok((mut socket, _)) => {
                        socket.set_nonblocking(false).unwrap();
                        socket.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
                        let mut request = [0; 4096];
                        let n = socket.read(&mut request).unwrap();
                        assert!(String::from_utf8_lossy(&request[..n]).starts_with("GET /v1/ping "));
                        socket
                            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                            .unwrap();
                        break;
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        assert!(Instant::now() < deadline, "health check never reached mock server");
                        std::thread::sleep(Duration::from_millis(10));
                    }
                    Err(error) => panic!("mock server: {error}"),
                }
            }
        });
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let (release, wait) = mpsc::channel();
        let (scores, output, database) = runtime.block_on(async {
            // Poll both futures in one task, as buffer_unordered does in the runner.
            let scoring = score_async(vec![Box::new(SlowScorer(wait))], "candidate".into(), None);
            let probe = async {
                ensure_server_healthy(Some(&host)).await.unwrap();
                let _ = release.send(());
            };
            let (scores, ()) = futures::join!(scoring, probe);
            scores.unwrap()
        });
        server.join().unwrap();
        assert_eq!(output, "candidate");
        assert!(database.is_none());
        assert_eq!(scores[0].0, "slow");
        assert!(scores[0].1.pass, "scoring blocked the health check until its deadline");
        assert_eq!(scores[0].1.partial, 0.5);
        assert_eq!(scores[0].1.notes, json!({"preserved": true}));
    }

    #[test]
    fn preserves_sources_and_shared_files_after_success_and_failure() {
        let root = std::env::temp_dir().join(format!("llm-build-cleanup-{}", std::process::id()));
        for (lang, directories) in [
            (Lang::Rust, vec!["target"]),
            (Lang::CSharp, vec!["bin", "obj", ".nuget"]),
            (Lang::TypeScript, vec!["node_modules", "dist"]),
        ] {
            for succeeds in [true, false] {
                let project = root.join("project");
                fs::create_dir_all(&project).unwrap();
                fs::write(project.join("source"), "retain source").unwrap();
                fs::write(root.join("shared-cache"), "retain cache").unwrap();
                for directory in &directories {
                    fs::create_dir_all(project.join(directory)).unwrap();
                    fs::write(project.join(directory).join("artifact"), "build output").unwrap();
                }
                let result = with_build_cleanup(&project, lang, || {
                    assert!(directories.iter().all(|dir| project.join(dir).exists()));
                    if succeeds {
                        Ok(())
                    } else {
                        Err(anyhow!("build failed"))
                    }
                });
                assert_eq!(result.is_ok(), succeeds);
                if let Err(error) = result {
                    assert_eq!(error.to_string(), "build failed");
                }
                assert!(directories.iter().all(|dir| !project.join(dir).exists()));
                assert_eq!(fs::read_to_string(project.join("source")).unwrap(), "retain source");
                assert_eq!(fs::read_to_string(root.join("shared-cache")).unwrap(), "retain cache");
                with_build_cleanup(&project, lang, || Ok(())).unwrap();
            }
        }
        fs::remove_dir_all(root).unwrap();
    }
}

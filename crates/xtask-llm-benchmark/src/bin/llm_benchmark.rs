#![allow(clippy::disallowed_macros)]

use anyhow::{bail, Context, Result};
use futures::{StreamExt, TryStreamExt};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::{env, fs};
use tokio::runtime::Runtime;
use xtask_llm_benchmark::bench::bench_route_concurrency;
use xtask_llm_benchmark::bench::runner::{
    build_goldens_only_for_lang, ensure_goldens_built_once, run_selected_or_all_for_model_async_for_lang,
};
use xtask_llm_benchmark::bench::spacetime_guard::SpacetimeGuard;
use xtask_llm_benchmark::bench::types::{BenchModeContext, BenchRunContext, RouteRun, RunAllContext, RunConfig};
use xtask_llm_benchmark::context::constants::{
    results_path_details, results_path_run, results_path_summary, ALL_MODES,
};
use xtask_llm_benchmark::context::{build_context, compute_context_hash, docs_dir};
use xtask_llm_benchmark::eval::Lang;
use xtask_llm_benchmark::llm::types::Vendor;
use xtask_llm_benchmark::llm::{default_model_routes, make_provider_from_env, LlmProvider, ModelRoute};
use xtask_llm_benchmark::results::io::{update_golden_answers_on_disk, write_run, write_summary_from_details_file};
use xtask_llm_benchmark::results::{cmd_llm_benchmark_diff, load_run, BenchmarkRun, ModeRun, ModelRun};

fn main() -> Result<()> {
    let mut args = env::args().skip(1).collect::<Vec<_>>();
    if args.is_empty() {
        eprintln!(
            "Usage:
  llm run [--mode <docs|llms.md|cursor_rules|csv|...>] \
[--providers <openai,anthropic,google,xai,deepseek,meta>] \
[--models \"openai:gpt-5,gpt-4.1,o4-mini google:gemini-2.5-pro xai:grok-4 anthropic:claude-sonnet-4-5,claude-sonnet-4\"] \
[--tasks <list>] [--categories <csv>] [--hash-only] [--goldens-only] [--force]
  llm diff <base.json> <head.json>
  llm ci-check [--lang <rust|csharp>]

Options:
  --categories CSV of benchmark categories to run (e.g. basic,schema). If omitted, all categories are included.
  --providers   CSV of providers to include (e.g. openai,anthropic)
  --models      Space-separated groups of provider:model[,model...].
                Each model matches a route's api_model or display_name (case-insensitive).
                Examples:
                  anthropic:claude-sonnet-4-5,claude-sonnet-4
                  anthropic:\"Claude 4.5 Sonnet\"          (display name with spaces -> quote it)
                  openai:gpt-5,gpt-4.1,o4-mini
                  google:gemini-2.5-pro
                  xai:grok-4
  --tasks       Comma/space-separated selectors like 0 1 2 or 0,2,5,
                and/or task ids like t_001 t_020
  --hash-only   Only compute and print docs hash; do not run tasks
  --goldens-only
                Build/publish goldens only (skip LLM calls)
  --force       Re-run even if hashes match

Notes:
  • Anthropic ids: claude-sonnet-4-5, claude-sonnet-4, claude-3-7-sonnet-latest, claude-3-5-sonnet-latest
  • Base URLs must not include /v1; models must be valid for the chosen provider."
        );
    }

    let sub = args.remove(0);
    match sub.as_str() {
        "run" => cmd_run(&args),
        "diff" => {
            if args.len() != 2 {
                bail!("diff requires: <base.json> <head.json>");
            }
            let out = cmd_llm_benchmark_diff(&args[0], &args[1])?;
            println!("{out}");
            Ok(())
        }
        "ci-check" => cmd_ci_check(&args),
        "summary" => cmd_summary(&args),
        other => bail!("unknown subcommand {other}"),
    }
}

/* ------------------------------ run ------------------------------ */

fn parse_command_args(args: &[String]) -> Result<RunConfig> {
    let mut config = RunConfig {
        mode_flag: None,
        hash_only: false,
        goldens_only: false,
        lang: Lang::Rust,
        providers_filter: None,
        selectors: None,
        force: false,
        categories: None,
        model_filter: None,
    };

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--mode" => {
                i += 1;
                if i >= args.len() {
                    bail!("--mode needs a value");
                }
                config.mode_flag = Some(args[i].clone());
            }
            "--lang" => {
                i += 1;
                if i >= args.len() {
                    bail!("--lang needs a value");
                }
                config.lang = args[i].parse::<Lang>().map_err(anyhow::Error::msg)?;
            }
            "--hash-only" => config.hash_only = true,
            "--goldens-only" => config.goldens_only = true,
            "--providers" => {
                i += 1;
                if i >= args.len() {
                    bail!("--providers needs a value");
                }
                config.providers_filter = Some(parse_vendors_csv(&args[i])?);
            }
            "--models" => {
                i += 1;
                if i >= args.len() {
                    bail!("--models needs a value");
                }
                config.model_filter = Some(parse_models_arg(&args[i])?);
            }
            "--force" => config.force = true,
            "--categories" => {
                i += 1;
                let csv = args.get(i).context("--categories requires a CSV value")?;
                let set = csv
                    .split(',')
                    .map(|s| s.trim().to_ascii_lowercase())
                    .filter(|s| !s.is_empty())
                    .collect::<HashSet<_>>();
                config.categories = Some(set);
            }
            "--tasks" => {
                i += 1;
                if i >= args.len() {
                    bail!("--tasks needs a value");
                }
                let list: Vec<String> = args[i]
                    .split(|c: char| c == ',' || c.is_ascii_whitespace())
                    .map(|s| s.trim())
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string())
                    .collect();
                config.selectors = if list.is_empty() { None } else { Some(list) };
            }
            _ => {}
        }
        i += 1;
    }

    if config.hash_only && config.goldens_only {
        bail!("--hash-only and --goldens-only are mutually exclusive");
    }

    Ok(config)
}

fn parse_models_arg(raw: &str) -> Result<HashMap<Vendor, HashSet<String>>> {
    let mut out: HashMap<Vendor, HashSet<String>> = HashMap::new();

    for (i, group) in raw.split_whitespace().enumerate() {
        let group = group.trim();
        if group.is_empty() {
            continue;
        }

        let (prov_str, models_str) = group
            .split_once(':')
            .with_context(|| format!("model group must be provider:models — got '{group}' (pos {i})"))?;

        let vendor = Vendor::parse(prov_str)
            .ok_or_else(|| anyhow::anyhow!("unknown provider in --models: '{}'", prov_str.trim()))?;

        let mut set = out.remove(&vendor).unwrap_or_default();

        if models_str.trim().is_empty() {
            bail!("empty models list for provider '{}' at group {}", prov_str.trim(), i);
        }

        for (j, m) in models_str.split(',').enumerate() {
            let m = m.trim();
            if m.is_empty() {
                bail!("empty model name in group {} (entry {})", i, j);
            }
            set.insert(m.to_ascii_lowercase());
        }

        out.insert(vendor, set);
    }

    if out.is_empty() {
        bail!("--models parsed to an empty set");
    }

    Ok(out)
}

pub struct RuntimeInit {
    pub runtime: Option<Runtime>,
    pub provider: Option<Arc<dyn LlmProvider>>,
}

fn initialize_runtime_and_provider(hash_only: bool, goldens_only: bool) -> Result<RuntimeInit> {
    if hash_only {
        return Ok(RuntimeInit {
            runtime: None,
            provider: None,
        });
    }

    let _spacetime = SpacetimeGuard::acquire()?;
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().build()?;

    if goldens_only {
        return Ok(RuntimeInit {
            runtime: Some(runtime),
            provider: None,
        });
    }

    let llm_provider = make_provider_from_env()?;
    Ok(RuntimeInit {
        runtime: Some(runtime),
        provider: Some(llm_provider),
    })
}

fn process_mode(
    mode: &str,
    lang: Lang,
    config: &RunConfig,
    run: &mut BenchmarkRun,
    bench_root: &Path,
    runtime: Option<&Runtime>,
    llm_provider: Option<&Arc<dyn LlmProvider>>,
) -> Result<()> {
    let lang_str = lang.as_str();
    let context = build_context(mode)?;
    let hash = compute_context_hash(mode).with_context(|| format!("compute docs hash for `{mode}`/{}", lang_str))?;

    let mode_result_index = run.modes.iter().position(|m| m.mode == mode && m.lang == lang_str);

    let mut ctx = BenchModeContext {
        mode,
        lang_str,
        hash: &hash,
        config,
        results: run,
        bench_root,
        context: &context,
        lang,
        runtime,
        llm_provider,
    };

    match mode_result_index {
        Some(index) => update_existing_mode(index, &mut ctx)?,
        None => add_new_mode(&mut ctx)?,
    }

    Ok(())
}

fn update_existing_mode(index: usize, ctx: &mut BenchModeContext<'_>) -> Result<()> {
    let run = &mut ctx.results;
    let previous_hash = run.modes[index].hash.clone();
    let hash_changed = previous_hash != ctx.hash;

    if hash_changed {
        println!(
            "{:<12} [{:<10}] hash changed: {} -> {}",
            ctx.mode,
            ctx.lang_str,
            short_hash(&previous_hash),
            short_hash(ctx.hash)
        );
    } else {
        println!(
            "{:<12} [{:<10}] hash unchanged ({})",
            ctx.mode,
            ctx.lang_str,
            short_hash(ctx.hash)
        );
    }

    run.modes[index].hash = ctx.hash.to_string();

    if ctx.config.goldens_only {
        let rt = ctx.runtime.expect("runtime required for --goldens-only");
        let sels = ctx.config.selectors.as_deref();

        rt.block_on(build_goldens_only_for_lang(ctx.bench_root, ctx.lang, sels))?;
        println!("{:<12} [{:<10}] goldens-only build complete", ctx.mode, ctx.lang_str);
        return Ok(());
    }

    if !hash_changed && !ctx.config.force {
        println!(
            "{:<12} [{:<10}] hash unchanged ({}), skipped (use --force to rerun)",
            ctx.mode,
            ctx.lang_str,
            short_hash(ctx.hash)
        );
        return Ok(());
    }

    if !ctx.config.hash_only && !ctx.config.goldens_only {
        let runtime = ctx.runtime.expect("runtime required for normal runs");
        let llm_provider = ctx.llm_provider.expect("llm provider required for normal runs");

        let run_ctx = RunAllContext {
            rt: runtime,
            bench_root: ctx.bench_root,
            mode: ctx.mode,
            context: ctx.context,
            hash: ctx.hash,
            lang: ctx.lang,
            llm: llm_provider.as_ref(),
            providers_filter: ctx.config.providers_filter.as_ref(),
            selectors: ctx.config.selectors.as_deref(),
            model_filter: ctx.config.model_filter.as_ref(),
        };

        let models = run_all_routes_for_mode(&run_ctx)?;
        run.modes[index].models = models;
    }

    Ok(())
}

fn add_new_mode(ctx: &mut BenchModeContext<'_>) -> Result<()> {
    println!(
        "{:<12} [{:<10}] added with hash {}",
        ctx.mode,
        ctx.lang_str,
        short_hash(ctx.hash)
    );

    if ctx.config.goldens_only {
        let rt = ctx.runtime.expect("runtime required for --goldens-only");
        let sels = ctx.config.selectors.as_deref();

        rt.block_on(build_goldens_only_for_lang(ctx.bench_root, ctx.lang, sels))?;
        println!("{:<12} [{:<10}] goldens-only build complete", ctx.mode, ctx.lang_str);

        ctx.results.modes.push(ModeRun {
            mode: ctx.mode.to_string(),
            lang: ctx.lang_str.to_string(),
            hash: ctx.hash.to_string(),
            models: Vec::new(),
        });

        return Ok(());
    }

    let models = if ctx.config.hash_only || ctx.config.goldens_only {
        Vec::new()
    } else {
        let runtime = ctx.runtime.expect("runtime required for normal runs");
        let llm_provider = ctx.llm_provider.expect("llm provider required for normal runs");

        let run_ctx = RunAllContext {
            rt: runtime,
            bench_root: ctx.bench_root,
            mode: ctx.mode,
            context: ctx.context,
            hash: ctx.hash,
            lang: ctx.lang,
            llm: llm_provider.as_ref(),
            providers_filter: ctx.config.providers_filter.as_ref(),
            selectors: ctx.config.selectors.as_deref(),
            model_filter: ctx.config.model_filter.as_ref(),
        };

        run_all_routes_for_mode(&run_ctx)?
    };

    ctx.results.modes.push(ModeRun {
        mode: ctx.mode.to_string(),
        lang: ctx.lang_str.to_string(),
        hash: ctx.hash.to_string(),
        models,
    });

    Ok(())
}

fn cmd_run(args: &[String]) -> Result<()> {
    let mut config = parse_command_args(args)?;
    let bench_root = find_bench_root();

    let modes: Vec<String> = match config.mode_flag {
        Some(ref mode_list) => mode_list
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect(),
        None => ALL_MODES.iter().map(|s| s.to_string()).collect(),
    };

    let mut run: BenchmarkRun = load_run(results_path_run()).unwrap_or_default();

    let RuntimeInit {
        runtime,
        provider: llm_provider,
    } = initialize_runtime_and_provider(config.hash_only, config.goldens_only)?;

    config.selectors = apply_category_filter(&bench_root, config.categories.as_ref(), config.selectors.as_deref())?;

    let selectors: Option<Vec<String>> = config.selectors.clone();
    let selectors_ref: Option<&[String]> = selectors.as_deref();

    if !config.goldens_only {
        let rt = runtime.as_ref().expect("failed to initialize runtime for goldens");
        rt.block_on(ensure_goldens_built_once(&bench_root, config.lang, selectors_ref))?;
    }

    for mode in modes {
        process_mode(
            &mode,
            config.lang,
            &config,
            &mut run,
            &bench_root,
            runtime.as_ref(),
            llm_provider.as_ref(),
        )?;
    }

    if !config.goldens_only {
        run.generated_at = chrono::Utc::now().to_rfc3339();
        fs::create_dir_all(docs_dir().join("llms"))?;

        write_run(&run)?;

        update_golden_answers_on_disk(
            &results_path_details(), // the merged JSON
            &bench_root,
            /*all=*/ true,
            /*overwrite=*/ true,
        )?;

        write_summary_from_details_file(results_path_details(), results_path_summary())?;
    }

    Ok(())
}

/* --------------------------- ci-check --------------------------- */

fn cmd_ci_check(args: &[String]) -> Result<()> {
    // Check-only:
    //  - Verifies the required mode exists for each language
    //  - Computes the current context hash and compares against the saved run hash
    //  - Does NOT run any providers/models or build goldens
    //
    // Required per language:
    //   Rust   → "rustdoc_json"
    //   CSharp → "docs"
    //
    // Optional: --lang <rust|csharp>

    // Parse --lang (optional)
    let mut langs: Vec<Lang> = Vec::new();
    let mut i = 0;
    while i < args.len() {
        if args[i].as_str() == "--lang" {
            i += 1;
            if i >= args.len() {
                bail!("--lang needs a value");
            }
            let l = args[i].parse::<Lang>().map_err(anyhow::Error::msg)?;
            langs.push(l);
        }
        i += 1;
    }
    if langs.is_empty() {
        langs = vec![Lang::Rust, Lang::CSharp];
    }

    // Required mode per language (use this everywhere)
    let required_mode = |lang: Lang| -> &'static str {
        match lang {
            Lang::Rust => "rustdoc_json",
            Lang::CSharp => "docs",
        }
    };

    // Debug hint for how to (re)generate entries
    let hint_for = |lang: Lang| -> &'static str {
        match lang {
            Lang::Rust => "cargo llm run --mode rustdoc_json --lang rust",
            Lang::CSharp => "cargo llm run --mode docs --lang csharp",
        }
    };

    // Load prior run to compare hashes against
    let run: BenchmarkRun =
        load_run(results_path_run()).with_context(|| format!("load prior run file at {:?}", results_path_run()))?;

    for lang in langs {
        let mode = required_mode(lang);
        let lang_str = lang.as_str();

        // Ensure mode exists (non-empty paths)
        match xtask_llm_benchmark::context::resolve_mode_paths(mode) {
            Ok(paths) if !paths.is_empty() => {}
            Ok(_) => bail!(
                "CI check FAILED: {}/{} resolved to 0 paths.\n→ Try: {}",
                mode,
                lang_str,
                hint_for(lang)
            ),
            Err(e) => bail!(
                "CI check FAILED: {}/{} not available: {}.\n→ Try: {}",
                mode,
                lang_str,
                e,
                hint_for(lang)
            ),
        }

        // Compute current context hash
        let current_hash =
            compute_context_hash(mode).with_context(|| format!("compute context hash for `{mode}`/{lang_str}"))?;

        // Find saved hash
        let idx = match run.modes.iter().position(|m| m.mode == mode && m.lang == lang_str) {
            Some(i) => i,
            None => bail!(
                "CI check FAILED: no saved run entry for {}/{}.\n→ Generate it with: {}",
                mode,
                lang_str,
                hint_for(lang)
            ),
        };

        let saved_hash = &run.modes[idx].hash;
        if *saved_hash != current_hash {
            bail!(
                "CI check FAILED: hash mismatch for {}/{}: saved={} current={}.\n→ Re-run to refresh: {}",
                mode,
                lang_str,
                short_hash(saved_hash.as_str()),
                short_hash(&current_hash),
                hint_for(lang)
            );
        }

        println!("CI check OK: {}/{} hash {}", mode, lang_str, short_hash(&current_hash));
    }

    Ok(())
}

/* --------------------------- helpers --------------------------- */

fn short_hash(s: &str) -> &str {
    &s[..s.len().min(12)]
}

fn run_all_routes_for_mode(cfg: &RunAllContext<'_>) -> Result<Vec<ModelRun>> {
    let routes: Vec<ModelRoute> = default_model_routes()
        .iter()
        .filter(|r| cfg.providers_filter.is_none_or(|f| f.contains(&r.vendor)))
        .filter(|r| {
            if let Some(map) = cfg.model_filter {
                if let Some(allowed) = map.get(&r.vendor) {
                    let api = r.api_model.to_ascii_lowercase();
                    let dn = r.display_name.to_ascii_lowercase();
                    return allowed.contains(&api) || allowed.contains(&dn);
                }
            }
            true
        })
        .cloned()
        .collect();

    // Run each route
    let models: Vec<ModelRun> = cfg.rt.block_on(async {
        use futures::{stream, TryStreamExt};

        stream::iter(routes.iter().map(|route| {
            // Build per-route context
            let per = BenchRunContext {
                bench_root: cfg.bench_root,
                mode: cfg.mode,
                hash: cfg.hash,
                route,
                context: cfg.context,
                llm: cfg.llm,
                lang: cfg.lang,
                selectors: cfg.selectors,
            };

            async move {
                // Run (selected-or-all) for this route
                let outcomes = run_selected_or_all_for_model_async_for_lang(&per).await?;

                // Compute summary for ModelRun
                let total: u32 = outcomes.iter().map(|o| o.total_tests).sum();
                let passed: u32 = outcomes.iter().map(|o| o.passed_tests).sum();
                let pct = if total == 0 {
                    0.0
                } else {
                    (passed as f32 / total as f32) * 100.0
                };

                Ok::<ModelRun, anyhow::Error>(ModelRun {
                    name: route.display_name.into(),
                    score: Some(pct),
                })
            }
        }))
        .buffer_unordered(bench_route_concurrency())
        .try_collect()
        .await
    })?;

    Ok(models)
}

pub async fn run_many_routes_for_lang(cfg: &BenchRunContext<'_>, routes: &[ModelRoute]) -> Result<Vec<RouteRun>> {
    let rbuf = bench_route_concurrency();

    let bench_root = cfg.bench_root;
    let mode = cfg.mode;
    let hash = cfg.hash;
    let context = cfg.context;
    let llm = cfg.llm;
    let lang = cfg.lang;
    let selectors = cfg.selectors;

    futures::stream::iter(routes.iter().cloned().map(move |route| {
        let bench_root = bench_root;
        let mode = mode;
        let hash = hash;
        let context = context;
        let llm = llm;
        let lang = lang;
        let selectors = selectors;

        async move {
            println!("→ running {}", route.display_name);

            let per = BenchRunContext {
                bench_root,
                mode,
                hash,
                route: &route,
                context,
                llm,
                lang,
                selectors,
            };

            let outcomes = run_selected_or_all_for_model_async_for_lang(&per).await?;

            let total: u32 = outcomes.iter().map(|o| o.total_tests).sum();
            let passed: u32 = outcomes.iter().map(|o| o.passed_tests).sum();
            let pct = if total == 0 {
                0.0
            } else {
                (passed as f32 / total as f32) * 100.0
            };

            println!("   ↳ {}: {}/{} passed ({:.1}%)", route.display_name, passed, total, pct);

            Ok::<_, anyhow::Error>(RouteRun {
                route_name: route.display_name.to_string(),
                api_model: route.api_model.to_string(),
                outcomes,
            })
        }
    }))
    .buffer_unordered(rbuf)
    .try_collect::<Vec<_>>()
    .await
}

fn parse_vendors_csv(s: &str) -> Result<HashSet<Vendor>> {
    let mut out = HashSet::new();

    for (i, raw) in s.split(',').enumerate() {
        let tok = raw.trim();
        if tok.is_empty() {
            bail!("empty vendor at position {}", i);
        }
        let v = Vendor::parse(tok).ok_or_else(|| anyhow::anyhow!("unknown provider: {}", tok))?;
        out.insert(v);
    }

    if out.is_empty() {
        bail!("no vendors provided");
    }

    Ok(out)
}

fn find_bench_root() -> PathBuf {
    let start = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    for dir in start.ancestors() {
        let cand = dir.join("src").join("benchmarks");
        if cand.is_dir() {
            return cand;
        }
    }
    start.join("src").join("benchmarks")
}

fn collect_task_numbers_in_categories(bench_root: &Path, cats: &HashSet<String>) -> Result<HashSet<u32>> {
    let mut nums = HashSet::new();
    for c in cats {
        let dir = bench_root.join(c);
        if !dir.is_dir() {
            continue;
        }
        for entry in fs::read_dir(&dir).with_context(|| format!("read_dir {}", dir.display()))? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            if let Some(rest) = name.strip_prefix("t_") {
                if let Some((num_str, _)) = rest.split_once('_') {
                    if num_str.len() == 3 {
                        if let Ok(n) = num_str.parse::<u32>() {
                            nums.insert(n);
                        }
                    }
                }
            }
        }
    }
    Ok(nums)
}

fn normalize_numeric_selectors(raw: &[String]) -> Vec<u32> {
    raw.iter()
        .filter(|s| !s.is_empty() && s.chars().all(|c| c.is_ascii_digit()))
        .filter_map(|s| s.parse::<u32>().ok())
        .collect()
}

fn apply_category_filter(
    bench_root: &Path,
    categories: Option<&HashSet<String>>,
    selectors: Option<&[String]>,
) -> Result<Option<Vec<String>>> {
    match categories {
        None => {
            // No category filter; keep selectors as-is
            Ok(selectors.map(|s| s.to_vec()))
        }
        Some(cats) => {
            let allowed = collect_task_numbers_in_categories(bench_root, cats)?;
            let out_nums: Vec<u32> = match selectors {
                Some(user) => {
                    let nums = normalize_numeric_selectors(user);
                    nums.into_iter().filter(|n| allowed.contains(n)).collect()
                }
                None => {
                    let mut v: Vec<u32> = allowed.into_iter().collect();
                    v.sort_unstable();
                    v
                }
            };
            if out_nums.is_empty() {
                Ok(None)
            } else {
                Ok(Some(out_nums.into_iter().map(|n| n.to_string()).collect()))
            }
        }
    }
}

fn cmd_summary(args: &[String]) -> Result<()> {
    // Accept 0–2 positional args:
    //   llm summary
    //   llm summary <details.json>
    //   llm summary <details.json> <summary.json>
    let (in_path, out_path) = match args.len() {
        0 => (results_path_details(), results_path_summary()),
        1 => (PathBuf::from(&args[0]), results_path_summary()),
        _ => (PathBuf::from(&args[0]), PathBuf::from(&args[1])),
    };

    if let Some(parent) = out_path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("create dir {}", parent.display()))?;
    }

    write_summary_from_details_file(in_path, out_path)
}

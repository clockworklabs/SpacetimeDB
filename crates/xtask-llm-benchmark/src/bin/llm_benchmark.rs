use anyhow::{bail, Context, Result};
use futures::{stream, StreamExt, TryStreamExt};
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
use xtask_llm_benchmark::bench::types::RouteRun;
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
    let _ = dotenvy::dotenv();

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
        other => bail!("unknown subcommand {other}"),
    }
}

/* ------------------------------ run ------------------------------ */

struct RunConfig {
    mode_flag: Option<String>,
    hash_only: bool,
    goldens_only: bool,
    lang: Lang,
    providers_filter: Option<HashSet<Vendor>>,
    selectors: Option<Vec<String>>,
    force: bool,
    categories: Option<HashSet<String>>,
    model_filter: Option<HashMap<Vendor, HashSet<String>>>,
}

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

fn initialize_runtime_and_provider(
    hash_only: bool,
    goldens_only: bool,
) -> Result<(Option<Runtime>, Option<Arc<dyn LlmProvider>>)> {
    if hash_only {
        return Ok((None, None));
    }

    let _spacetime = SpacetimeGuard::acquire()?;
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().build()?;

    if goldens_only {
        return Ok((Some(runtime), None));
    }

    let llm_provider = make_provider_from_env()?;
    Ok((Some(runtime), Some(llm_provider)))
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

    match mode_result_index {
        Some(index) => {
            update_existing_mode(
                index,
                mode,
                lang_str,
                &hash,
                config,
                run,
                bench_root,
                &context,
                lang,
                runtime,
                llm_provider,
            )?;
        }
        None => {
            add_new_mode(
                mode,
                lang_str,
                &hash,
                config,
                run,
                bench_root,
                &context,
                lang,
                runtime,
                llm_provider,
            )?;
        }
    }

    Ok(())
}

fn update_existing_mode(
    index: usize,
    mode: &str,
    lang_str: &str,
    hash: &str,
    config: &RunConfig,
    run: &mut BenchmarkRun,
    bench_root: &Path,
    context: &str,
    lang: Lang,
    runtime: Option<&Runtime>,
    llm_provider: Option<&Arc<dyn LlmProvider>>,
) -> Result<()> {
    let previous_hash = run.modes[index].hash.clone();
    let hash_changed = previous_hash != hash;

    if hash_changed {
        println!(
            "{:<12} [{:<10}] hash changed: {} -> {}",
            mode,
            lang_str,
            short_hash(&previous_hash),
            short_hash(hash)
        );
    } else {
        println!("{:<12} [{:<10}] hash unchanged ({})", mode, lang_str, short_hash(hash));
    }

    run.modes[index].hash = hash.to_string();

    if config.goldens_only {
        let rt = runtime.expect("runtime required for --goldens-only");
        let sels = config.selectors.as_deref();
        rt.block_on(build_goldens_only_for_lang(bench_root, lang, sels))?;
        println!("{:<12} [{:<10}] goldens-only build complete", mode, lang_str);
        return Ok(());
    }

    if !hash_changed && !config.force {
        println!(
            "{:<12} [{:<10}] hash unchanged ({}), skipped (use --force to rerun)",
            mode,
            lang_str,
            short_hash(hash)
        );
        return Ok(());
    }

    if !config.hash_only && !config.goldens_only {
        let models = run_all_routes_for_mode(
            runtime.unwrap(),
            bench_root,
            mode,
            context,
            hash,
            lang,
            llm_provider.unwrap().as_ref(),
            config.providers_filter.as_ref(),
            config.selectors.as_deref(),
            config.model_filter.as_ref(),
        )?;
        run.modes[index].models = models;
    }

    Ok(())
}

fn add_new_mode(
    mode: &str,
    lang_str: &str,
    hash: &str,
    config: &RunConfig,
    results: &mut BenchmarkRun,
    bench_root: &Path,
    context: &str,
    lang: Lang,
    runtime: Option<&Runtime>,
    llm_provider: Option<&Arc<dyn LlmProvider>>,
) -> Result<()> {
    println!("{:<12} [{:<10}] added with hash {}", mode, lang_str, short_hash(hash));

    if config.goldens_only {
        let rt = runtime.expect("runtime required for --goldens-only");
        let sels = config.selectors.as_deref();
        rt.block_on(build_goldens_only_for_lang(bench_root, lang, sels))?;
        println!("{:<12} [{:<10}] goldens-only build complete", mode, lang_str);

        results.modes.push(ModeRun {
            mode: mode.to_string(),
            lang: lang_str.to_string(),
            hash: hash.to_string(),
            models: Vec::new(),
        });
        return Ok(());
    }

    let models = if config.hash_only || config.goldens_only {
        Vec::new()
    } else {
        run_all_routes_for_mode(
            runtime.unwrap(),
            bench_root,
            mode,
            context,
            hash,
            lang,
            llm_provider.unwrap().as_ref(),
            config.providers_filter.as_ref(),
            config.selectors.as_deref(),
            config.model_filter.as_ref(),
        )?
    };

    results.modes.push(ModeRun {
        mode: mode.to_string(),
        lang: lang_str.to_string(),
        hash: hash.to_string(),
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

    let (runtime, llm_provider) = initialize_runtime_and_provider(config.hash_only, config.goldens_only)?;

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
        fs::create_dir_all(PathBuf::from(docs_dir()).join("llms"))?;

        write_run(&run)?;

        update_golden_answers_on_disk(
            &*results_path_details(), // the merged JSON
            &*bench_root,
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
        match args[i].as_str() {
            "--lang" => {
                i += 1;
                if i >= args.len() {
                    bail!("--lang needs a value");
                }
                let l = args[i].parse::<Lang>().map_err(anyhow::Error::msg)?;
                langs.push(l);
            }
            _ => {}
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

fn run_all_routes_for_mode(
    rt: &Runtime,
    bench_root: &Path,
    mode: &str,
    context: &str,
    hash: &str,
    lang: Lang,
    llm: &dyn LlmProvider,
    providers_filter: Option<&HashSet<Vendor>>,
    selectors: Option<&[String]>,
    model_filter: Option<&HashMap<Vendor, HashSet<String>>>,
) -> Result<Vec<ModelRun>> {
    let routes: Vec<ModelRoute> = default_model_routes()
        .iter()
        .cloned()
        .filter(|r| providers_filter.map_or(true, |f| f.contains(&r.vendor)))
        .filter(|r| {
            if let Some(map) = model_filter {
                if let Some(allowed) = map.get(&r.vendor) {
                    let api = r.api_model.to_ascii_lowercase();
                    let dn = r.display_name.to_ascii_lowercase();
                    return allowed.contains(&api) || allowed.contains(&dn);
                }
            }
            true
        })
        .collect();

    let runs = rt.block_on(run_many_routes_for_lang(
        bench_root, mode, hash, &routes, context, llm, lang, selectors,
    ))?;

    let mut models = Vec::with_capacity(runs.len());
    for r in runs {
        let total: u32 = r.outcomes.iter().map(|o| o.total_tests).sum();
        let passed: u32 = r.outcomes.iter().map(|o| o.passed_tests).sum();
        let pct = if total == 0 {
            0.0
        } else {
            (passed as f32 / total as f32) * 100.0
        };
        models.push(ModelRun {
            name: r.route_name,
            score: Some(pct),
        });
    }

    Ok(models)
}

pub async fn run_many_routes_for_lang(
    bench_root: &Path,
    mode: &str,
    hash: &str,
    routes: &[ModelRoute],
    context: &str,
    llm: &dyn LlmProvider,
    lang: Lang,
    selectors: Option<&[String]>,
) -> Result<Vec<RouteRun>> {
    let rbuf = bench_route_concurrency();

    stream::iter(routes.iter().cloned().map(|route| async move {
        println!("→ running {}", route.display_name);
        let selectors_ref = selectors.as_deref();
        let outcomes = run_selected_or_all_for_model_async_for_lang(
            bench_root,
            mode,
            hash,
            &route,
            context,
            llm,
            lang,
            selectors_ref,
        )
        .await?;
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
            Ok(selectors.map(|s| s.iter().cloned().collect()))
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

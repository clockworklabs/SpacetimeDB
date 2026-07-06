#![allow(clippy::disallowed_macros, clippy::type_complexity, clippy::enum_variant_names)]

use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand, ValueEnum};
use futures::{StreamExt, TryStreamExt};
use spacetimedb_data_structures::map::{HashCollectionExt as _, HashMap, HashSet};
use spacetimedb_guard::SpacetimeDbGuard;
use std::fs;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Arc;
use tokio::runtime::Runtime;
use xtask_llm_benchmark::api::ApiClient;
use xtask_llm_benchmark::bench::bench_route_concurrency;
use xtask_llm_benchmark::bench::runner::{
    build_goldens_only_for_lang, ensure_goldens_built_once, run_selected_or_all_for_model_async_for_lang,
};
use xtask_llm_benchmark::bench::types::{BenchRunContext, RouteRun, RunConfig, RunOutcome};
use xtask_llm_benchmark::context::constants::ALL_MODES;
use xtask_llm_benchmark::context::{build_context, compute_processed_context_hash};
use xtask_llm_benchmark::eval::Lang;
use xtask_llm_benchmark::llm::types::Vendor;
use xtask_llm_benchmark::llm::{default_model_routes, make_provider_from_env, LlmProvider, ModelRoute};

#[derive(Clone, Debug)]
struct ModelGroup {
    vendor: Vendor,
    models: Vec<String>,
}

impl std::str::FromStr for ModelGroup {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        let s = s.trim();
        let (prov_str, models_str) = s
            .split_once(':')
            .ok_or_else(|| format!("expected provider:model[,model...], got '{s}'"))?;

        let vendor =
            Vendor::parse(prov_str.trim()).ok_or_else(|| format!("unknown provider: '{}'", prov_str.trim()))?;

        let mut models: Vec<String> = Vec::new();
        for m in models_str.split(',').map(|m| m.trim()).filter(|m| !m.is_empty()) {
            if m.contains(':') {
                return Err(format!(
                    "model name '{m}' contains ':'. Did you mean to pass another group? \
             Use: --models openai:gpt-5 google:gemini-2.5-pro"
                ));
            }
            models.push(m.to_ascii_lowercase());
        }

        if models.is_empty() {
            return Err(format!("empty model list for provider '{}'", prov_str.trim()));
        }

        Ok(Self { vendor, models })
    }
}

#[derive(Parser, Debug)]
#[command(
    name = "llm",
    about = "LLM benchmark runner",
    arg_required_else_help = true,
    after_help = "Notes:\n  • Anthropic ids: claude-sonnet-4-5, claude-sonnet-4, claude-3-7-sonnet-latest, claude-3-5-sonnet-latest\n  • Base URLs must not include /v1; models must be valid for the chosen provider.\n"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum ModelSource {
    Static,
    Remote,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Run benchmarks / build goldens / compute hashes.
    Run(RunArgs),

    /// Run AI analysis on existing benchmark failures from the database.
    Analyze(AnalyzeArgs),
}

#[derive(Args, Debug, Clone)]
struct RunArgs {
    /// Comma-separated list of modes (default: all modes)
    #[arg(long, value_delimiter = ',')]
    modes: Option<Vec<String>>,

    /// Language to benchmark
    #[arg(long, default_value = "rust")]
    lang: Lang,

    /// Only compute/print docs hash; do not run tasks
    #[arg(long, conflicts_with = "goldens_only")]
    hash_only: bool,

    /// Build/publish goldens only (skip LLM calls)
    #[arg(long, conflicts_with = "hash_only")]
    goldens_only: bool,

    /// Re-run even if hashes match
    #[arg(long)]
    force: bool,

    /// Comma separated or space separated list of benchmark categories (e.g. basic,schema)
    #[arg(long, num_args = 1.., value_delimiter = ',')]
    categories: Option<Vec<String>>,

    /// Comma separated or space separated list like 0,2,5 and/or task ids like t_001
    #[arg(long, num_args = 1.., value_delimiter = ',')]
    tasks: Option<Vec<String>>,

    /// Comma separated or space separated list of providers to include (e.g. openai,anthropic)
    #[arg(long, num_args = 1.., value_delimiter = ',')]
    providers: Option<Vec<VendorArg>>,

    /// Model groups, repeatable. Each group is provider:model[,model...]
    /// You can pass multiple groups after one `--models`, or repeat `--models`.
    ///
    /// Examples:
    ///   --models openai:gpt-5,gpt-4.1,o4-mini google:gemini-2.5-pro
    ///   --models "anthropic:Claude 4.5 Sonnet"
    ///   --models "anthropic:Claude 4.5 Sonnet" --models openai:gpt-5
    #[arg(long, num_args = 1..)]
    models: Option<Vec<ModelGroup>>,

    /// Where to resolve models when --models is not provided
    #[arg(long, value_enum, default_value_t = ModelSource::Static)]
    model_source: ModelSource,

    /// Run benchmarks without uploading results
    #[arg(long)]
    dry_run: bool,

    /// When used with --dry-run, also generate local markdown analysis files
    #[arg(long, requires = "dry_run")]
    local_analysis: bool,

    #[arg(skip)]
    route_overrides: Option<Vec<ModelRoute>>,
}

#[derive(Args, Debug, Clone)]
struct AnalyzeArgs {
    /// Filter by language (e.g. rust, csharp, typescript)
    #[arg(long)]
    lang: Option<String>,

    /// Filter by mode (e.g. guidelines, no_context, docs)
    #[arg(long)]
    mode: Option<String>,

    /// Filter by model name (e.g. "Claude Sonnet 4.6")
    #[arg(long)]
    model: Option<String>,

    /// Run date (YYYY-MM-DD). If omitted, lists available dates.
    #[arg(long)]
    date: Option<String>,

    /// Print analysis to stdout instead of uploading
    #[arg(long)]
    dry_run: bool,
}

/// Local wrapper so we can parse Vendor without orphan-rule issues.
#[derive(Clone, Debug)]
struct VendorArg(pub Vendor);

impl FromStr for VendorArg {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        Vendor::parse(s.trim())
            .map(VendorArg)
            .ok_or_else(|| format!("unknown provider: {}", s.trim()))
    }
}

/// Load `.env` from workspace root (repo) first, then current directory.
/// Uses from_path_override so .env always wins over existing env vars.
/// Runs once at startup; each new process loads .env fresh (no in-process reload).
fn load_dotenv() {
    let workspace_env = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .map(|p| p.join(".env"))
        .filter(|p| p.is_file());
    let cwd_env = std::env::current_dir()
        .ok()
        .map(|c| c.join(".env"))
        .filter(|p| p.is_file());
    let path = workspace_env.or(cwd_env);
    if let Some(p) = path {
        match dotenvy::from_path_override(&p) {
            Ok(()) => eprintln!("[env] loaded .env from {}", p.display()),
            Err(e) => eprintln!("[env] failed to load .env from {}: {}", p.display(), e),
        }
    } else {
        eprintln!("[env] no .env found (tried workspace root and cwd)");
    }
}

fn main() -> Result<()> {
    // Load .env from current directory or workspace root so API keys and settings are available.
    load_dotenv();

    let cli = Cli::parse();

    match cli.command {
        Commands::Run(args) => cmd_run(args),
        Commands::Analyze(args) => cmd_analyze(args),
    }
}

/* ------------------------------ run ------------------------------ */

fn cmd_run(args: RunArgs) -> Result<()> {
    run_benchmarks(args)?;
    Ok(())
}

/// Core benchmark runner used by both `run` and `ci-quickfix`
fn run_benchmarks(args: RunArgs) -> Result<()> {
    let dry_run = args.dry_run;
    let local_analysis = args.local_analysis;
    let dry_run_id = dry_run.then(|| chrono::Utc::now().format("%Y-%m-%d_%H%M%S").to_string());
    let should_fetch_remote_routes = should_fetch_remote_routes(&args);

    let needs_api_client = should_fetch_remote_routes || !dry_run;
    let api_client = if needs_api_client {
        ApiClient::from_env().context("failed to initialize API client")?
    } else {
        None
    };
    let upload_client = if dry_run { None } else { api_client.clone() };

    if upload_client.is_none() && !dry_run {
        eprintln!("[warn] LLM_BENCHMARK_UPLOAD_URL not set; results will not be uploaded");
    }

    let mut config = RunConfig {
        modes: args.modes,
        hash_only: args.hash_only,
        goldens_only: args.goldens_only,
        lang: args.lang,
        providers_filter: args.providers.map(|v| v.into_iter().map(|vv| vv.0).collect()),
        selectors: args.tasks.as_ref().map(|v| v.to_vec()),
        force: args.force,
        categories: categories_to_set(args.categories),
        model_filter: model_filter_from_groups(args.models),
        host: None,
        api_client: upload_client.clone(),
        dry_run,
        local_analysis,
        dry_run_id: dry_run_id.clone(),
        route_overrides: args.route_overrides,
    };

    if should_fetch_remote_routes {
        let api = api_client
            .as_ref()
            .context("LLM_BENCHMARK_UPLOAD_URL required when --model-source remote is used")?;
        config.route_overrides = Some(api.fetch_model_routes()?);
    }

    let bench_root = find_bench_root();

    // Upload task catalog before running benchmarks
    if let Some(ref api) = upload_client
        && let Err(e) = api.upload_task_catalog(&bench_root)
    {
        eprintln!("[warn] failed to upload task catalog: {e}");
    }

    let RuntimeInit { runtime, guard } = initialize_runtime(config.hash_only)?;

    config.host = guard.as_ref().map(|g| g.host_url.clone());

    config.selectors = apply_category_filter(&bench_root, config.categories.as_ref(), config.selectors.as_deref())?;

    let selectors: Option<Vec<String>> = config.selectors.clone();
    let selectors_ref: Option<&[String]> = selectors.as_deref();

    let modes = config
        .modes
        .clone()
        .unwrap_or_else(|| ALL_MODES.iter().map(|s| s.to_string()).collect());

    if config.goldens_only {
        let rt = runtime.as_ref().expect("runtime required for --goldens-only");
        rt.block_on(build_goldens_only_for_lang(
            config.host.clone(),
            &bench_root,
            config.lang,
            selectors_ref,
        ))?;
        println!("[{}] goldens-only build complete", config.lang.as_str());
        return Ok(());
    }

    let llm_provider = if !config.goldens_only && !config.hash_only {
        let rt = runtime.as_ref().expect("failed to initialize runtime for goldens");
        rt.block_on(ensure_goldens_built_once(
            config.host.clone(),
            &bench_root,
            config.lang,
            selectors_ref,
        ))?;

        let provider = make_provider_from_env()?;
        let rt = runtime.as_ref().expect("failed to initialize runtime for preflight");
        let routes = filter_routes(&config);
        preflight_llm_routes(rt, provider.as_ref(), &routes, &modes)?;
        Some(provider)
    } else {
        None
    };

    let mut all_outcomes: Vec<RunOutcome> = Vec::new();

    for mode in modes {
        let outcomes = run_mode_benchmarks(
            &mode,
            config.lang,
            &config,
            &bench_root,
            runtime.as_ref(),
            llm_provider.as_ref(),
        )?;
        all_outcomes.extend(outcomes);
    }

    // Write local run log on --dry-run so results aren't lost
    if dry_run && !all_outcomes.is_empty() {
        let runs_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("runs");
        let _ = fs::create_dir_all(&runs_dir);
        let run_id = dry_run_id.as_deref().unwrap_or("dry-run");
        let log_path = runs_dir.join(format!("run-{run_id}.json"));
        match serde_json::to_string_pretty(&all_outcomes) {
            Ok(json) => {
                if let Err(e) = fs::write(&log_path, json) {
                    eprintln!("[warn] failed to write run log: {e}");
                } else {
                    println!("Run log: {}", log_path.display());
                }
            }
            Err(e) => eprintln!("[warn] failed to serialize run log: {e}"),
        }
    }

    Ok(())
}

/* ------------------------------ analyze ------------------------------ */

fn cmd_analyze(args: AnalyzeArgs) -> Result<()> {
    let api = ApiClient::from_env()
        .context("failed to initialize API client")?
        .context("LLM_BENCHMARK_UPLOAD_URL required for analyze")?;

    // If no date specified, list available dates and exit
    if args.date.is_none() {
        let dates = api.fetch_run_dates(args.lang.as_deref(), args.mode.as_deref())?;
        if dates.is_empty() {
            println!("No run dates found.");
        } else {
            println!("Available run dates:");
            for d in &dates {
                println!("  {}", d);
            }
            println!("\nUse --date YYYY-MM-DD to analyze a specific run.");
        }
        return Ok(());
    }

    let date = args.date.as_deref().unwrap();

    // Fetch failures from the API
    let (failures, run_date) = api.fetch_failures(
        args.lang.as_deref(),
        args.mode.as_deref(),
        args.model.as_deref(),
        Some(date),
    )?;

    let run_date = run_date.unwrap_or_else(|| date.to_string());

    if failures.is_empty() {
        println!("No failures found for date {}.", run_date);
        return Ok(());
    }

    // Group failures by (lang, mode, model)
    let mut groups: std::collections::BTreeMap<(String, String, String), Vec<&serde_json::Value>> =
        std::collections::BTreeMap::new();
    for f in &failures {
        let lang = f["lang"].as_str().unwrap_or("unknown").to_string();
        let mode = f["mode"].as_str().unwrap_or("unknown").to_string();
        let model = f["modelName"].as_str().unwrap_or("unknown").to_string();
        groups.entry((lang, mode, model)).or_default().push(f);
    }

    println!(
        "Found {} failures across {} (lang, mode, model) groups for date {}",
        failures.len(),
        groups.len(),
        run_date
    );

    // Initialize LLM provider for analysis
    let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build()?;
    let provider = make_provider_from_env()?;

    let analysis_route = ModelRoute::new(
        "gpt-5.4-mini",
        xtask_llm_benchmark::llm::types::Vendor::OpenAi,
        "gpt-5.4-mini",
        Some("openai/gpt-5.4-mini"),
    );

    for ((lang, mode, model), group_failures) in &groups {
        println!(
            "\nAnalyzing {}/{}/{} ({} failures)...",
            lang,
            mode,
            model,
            group_failures.len()
        );

        // Build prompt from the JSON failure data
        let prompt = build_analysis_prompt_from_json(lang, mode, model, group_failures);

        let built = xtask_llm_benchmark::llm::prompt::BuiltPrompt {
            system: Some(xtask_llm_benchmark::bench::analysis::system_prompt()),
            static_prefix: None,
            segments: vec![xtask_llm_benchmark::llm::segmentation::Segment::new("user", prompt)],
            search_enabled: false,
        };

        let analysis = runtime.block_on(provider.generate(&analysis_route, &built))?.text;

        if args.dry_run {
            // Save locally
            let runs_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("runs");
            let _ = fs::create_dir_all(&runs_dir);
            let safe_model = model.replace([' ', '/'], "_");
            let path = runs_dir.join(format!("analysis-{lang}-{mode}-{safe_model}-{run_date}.md"));
            if let Err(e) = fs::write(&path, &analysis) {
                eprintln!("[warn] failed to write analysis: {e}");
            } else {
                println!("Analysis written to: {}", path.display());
            }
        } else {
            api.upload_analysis(lang, mode, model, &analysis, &run_date)?;
        }
    }

    println!("\nDone.");
    Ok(())
}

fn build_analysis_prompt_from_json(lang: &str, mode: &str, model: &str, failures: &[&serde_json::Value]) -> String {
    // Reuse the shared prompt builder for the intro + instructions,
    // but we need to build the failure list from JSON values instead of RunOutcome.
    use xtask_llm_benchmark::bench::analysis::analysis_instructions;

    // Reuse the same context description logic as bench::analysis
    let lang_display = match lang {
        "rust" => "Rust",
        "csharp" => "C#",
        "typescript" => "TypeScript",
        _ => lang,
    };

    let ctx_desc = match mode {
        "guidelines" => "the SpacetimeDB AI guidelines (concise cheat-sheets for code generation)",
        "docs" => "SpacetimeDB markdown documentation",
        "rustdoc_json" => "SpacetimeDB rustdoc JSON (auto-generated API reference)",
        "llms.md" => "the SpacetimeDB llms.md file",
        "no_context" | "none" | "no_guidelines" => "no documentation (testing base model knowledge only)",
        "search" => "web search results (no local docs)",
        _ => "unspecified context",
    };

    let mut prompt = format!(
        "{model} was given {ctx_desc} and asked to generate {lang_display} SpacetimeDB modules. \
         It failed {count} tasks.\n\n",
        count = failures.len(),
    );

    for f in failures.iter().take(15) {
        let task_id = f["taskId"].as_str().unwrap_or("?");
        let passed = f["passedTests"].as_u64().unwrap_or(0);
        let total = f["totalTests"].as_u64().unwrap_or(0);

        prompt.push_str(&format!("### {} ({}/{})\n", task_id, passed, total));

        if let Some(details) = f["scorerDetails"].as_object() {
            let reasons: Vec<String> = details
                .iter()
                .filter_map(|(name, score)| {
                    if score["pass"].as_bool() == Some(true) {
                        return None;
                    }
                    let notes = &score["notes"];
                    let error = notes["error"]
                        .as_str()
                        .or_else(|| notes["stderr"].as_str())
                        .or_else(|| notes["diff"].as_str())
                        .unwrap_or("failed");
                    Some(format!("{}: {}", name, &error[..error.len().min(150)]))
                })
                .collect();
            if !reasons.is_empty() {
                prompt.push_str(&format!("Error: {}\n", reasons.join("; ")));
            }
        }

        if let Some(output) = f["llmOutput"].as_str() {
            let truncated = match output.char_indices().nth(1500) {
                Some((i, _)) => &output[..i],
                None => output,
            };
            prompt.push_str(&format!("```{}\n{}\n```\n", lang, truncated));
        }
        prompt.push('\n');
    }

    if failures.len() > 15 {
        prompt.push_str(&format!("({} more failures not shown)\n\n", failures.len() - 15));
    }

    prompt.push_str(&analysis_instructions(mode));
    prompt
}

fn model_filter_from_groups(groups: Option<Vec<ModelGroup>>) -> Option<HashMap<Vendor, HashSet<String>>> {
    let groups = groups?;
    let mut out: HashMap<Vendor, HashSet<String>> = HashMap::new();

    for g in groups {
        out.entry(g.vendor).or_default().extend(g.models.into_iter());
    }
    Some(out)
}

/* --------------------------- helpers --------------------------- */

fn short_hash(s: &str) -> &str {
    &s[..s.len().min(12)]
}

fn should_fetch_remote_routes(args: &RunArgs) -> bool {
    args.model_source == ModelSource::Remote
        && args.models.is_none()
        && args.route_overrides.is_none()
        && !args.hash_only
        && !args.goldens_only
}

fn preflight_llm_routes(
    runtime: &Runtime,
    llm_provider: &dyn LlmProvider,
    routes: &[ModelRoute],
    modes: &[String],
) -> Result<()> {
    if routes.is_empty() {
        return Ok(());
    }

    let mut search_flags = Vec::new();
    if modes.iter().any(|mode| mode == "search") {
        search_flags.push(true);
    }
    if modes.iter().any(|mode| mode != "search") {
        search_flags.push(false);
    }

    let mut failures = Vec::new();
    for route in routes {
        for search_enabled in &search_flags {
            let mode_label = if *search_enabled {
                "search/OpenRouter online"
            } else {
                "standard"
            };

            if let Err(err) = runtime.block_on(llm_provider.preflight_route(route, *search_enabled)) {
                let msg = format!("{} ({mode_label}): {err:#}", route.display_name);
                eprintln!("[preflight] FAILED {msg}");
                failures.push(msg);
            }
        }
    }

    if !failures.is_empty() {
        anyhow::bail!(
            "LLM provider preflight failed before benchmark run:\n  - {}",
            failures.join("\n  - ")
        );
    }

    Ok(())
}

/// Run benchmarks for a single mode.
fn run_mode_benchmarks(
    mode: &str,
    lang: Lang,
    config: &RunConfig,
    bench_root: &Path,
    runtime: Option<&Runtime>,
    llm_provider: Option<&Arc<dyn LlmProvider>>,
) -> Result<Vec<RunOutcome>> {
    let lang_str = lang.as_str();
    let context = build_context(mode, Some(lang))?;
    // Use processed context hash so each lang/mode combination has its own unique hash
    let hash = compute_processed_context_hash(mode, lang)
        .with_context(|| format!("compute processed context hash for `{mode}`/{}", lang_str))?;

    println!("{:<12} [{:<10}] hash: {}", mode, lang_str, short_hash(&hash));

    if config.hash_only {
        return Ok(Vec::new());
    }

    // Run benchmarks for all matching routes
    let routes = filter_routes(config);

    if routes.is_empty() {
        println!("{:<12} [{:<10}] no matching models to run", mode, lang_str);
        return Ok(Vec::new());
    }

    let runtime = runtime.expect("runtime required for normal runs");
    let llm_provider = llm_provider.expect("llm provider required for normal runs");

    let route_runs = runtime.block_on(run_many_routes_for_mode(
        bench_root,
        mode,
        &hash,
        &context,
        lang,
        config,
        llm_provider.as_ref(),
        &routes,
    ))?;

    // Print summary sorted by pass rate descending
    let mut summary: Vec<(&str, u32, u32, f32)> = route_runs
        .iter()
        .map(|rr| {
            let total: u32 = rr.outcomes.iter().map(|o| o.total_tests).sum();
            let passed: u32 = rr.outcomes.iter().map(|o| o.passed_tests).sum();
            let pct = if total == 0 {
                0.0
            } else {
                (passed as f32 / total as f32) * 100.0
            };
            (rr.route_name.as_str(), passed, total, pct)
        })
        .collect();
    summary.sort_by(|a, b| b.3.partial_cmp(&a.3).unwrap_or(std::cmp::Ordering::Equal));
    for (name, passed, total, pct) in &summary {
        println!("   ↳ {}: {}/{} passed ({:.1}%)", name, passed, total, pct);
    }

    let all_outcomes: Vec<RunOutcome> = route_runs.into_iter().flat_map(|rr| rr.outcomes).collect();
    Ok(all_outcomes)
}

/// Routes to run: when `model_filter` is set (from --models), only routes whose vendor and
/// model are in that filter are included; vendors not in the filter are excluded.
///
/// When explicit `openrouter:vendor/model` entries are passed they won't appear in
/// `default_model_routes`, so we synthesize ad-hoc routes for them here.
fn filter_routes(config: &RunConfig) -> Vec<ModelRoute> {
    let base_routes: Vec<ModelRoute> = config
        .route_overrides
        .clone()
        .unwrap_or_else(|| default_model_routes().to_vec());

    let mut routes: Vec<ModelRoute> = base_routes
        .iter()
        .filter(|r| config.providers_filter.as_ref().is_none_or(|f| f.contains(&r.vendor)))
        .filter(|r| match &config.model_filter {
            None => true,
            Some(allowed_by_vendor) => match allowed_by_vendor.get(&r.vendor) {
                None => false,
                Some(allowed) => {
                    let api = r.api_model.to_ascii_lowercase();
                    let dn = r.display_name.to_ascii_lowercase();
                    let or_id = r.openrouter_model.as_ref().map(|m| m.to_ascii_lowercase());
                    allowed.contains(&api)
                        || allowed.contains(&dn)
                        || or_id.as_ref().map(|m| allowed.contains(m)).unwrap_or(false)
                }
            },
        })
        .cloned()
        .collect();

    // Synthesize ad-hoc routes for any vendor:model that isn't in the static list.
    // This lets callers pass arbitrary model IDs (e.g. new models, openrouter paths)
    // without having to add them to default_model_routes() first.
    if let Some(mf) = &config.model_filter {
        for (vendor, model_ids) in mf {
            for model_id in model_ids {
                let already_matched = routes.iter().any(|r| {
                    r.vendor == *vendor
                        && (r.api_model == model_id.as_str()
                            || r.display_name.to_ascii_lowercase() == model_id.as_str()
                            || r.openrouter_model.as_deref() == Some(model_id.as_str()))
                });
                if !already_matched {
                    routes.push(ModelRoute::new(model_id, *vendor, model_id, None));
                }
            }
        }
    }

    routes
}

#[allow(clippy::too_many_arguments)]
async fn run_many_routes_for_mode(
    bench_root: &Path,
    mode: &str,
    hash: &str,
    context: &str,
    lang: Lang,
    config: &RunConfig,
    llm: &dyn LlmProvider,
    routes: &[ModelRoute],
) -> Result<Vec<RouteRun>> {
    let rbuf = bench_route_concurrency();
    let selectors = config.selectors.as_deref();
    let host = config.host.clone();
    let api_client = config.api_client.clone();
    let dry_run = config.dry_run;
    let local_analysis = config.local_analysis;
    let dry_run_id = config.dry_run_id.clone();

    futures::stream::iter(routes.iter().map(|route| {
        let host = host.clone();
        let api_client = api_client.clone();
        let dry_run_id = dry_run_id.clone();

        async move {
            println!("\u{2192} running {}", route.display_name);

            let per = BenchRunContext {
                bench_root,
                mode,
                hash,
                route,
                context,
                llm,
                lang,
                selectors,
                host,
                api_client,
                dry_run,
                local_analysis,
                dry_run_id,
            };

            let outcomes = run_selected_or_all_for_model_async_for_lang(&per).await?;

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

fn categories_to_set(v: Option<Vec<String>>) -> Option<HashSet<String>> {
    let v = v?;
    let set: HashSet<String> = v
        .into_iter()
        .map(|s| s.trim().to_ascii_lowercase())
        .filter(|s| !s.is_empty())
        .collect();
    (!set.is_empty()).then_some(set)
}

pub struct RuntimeInit {
    pub runtime: Option<Runtime>,
    pub guard: Option<SpacetimeDbGuard>,
}

fn initialize_runtime(hash_only: bool) -> Result<RuntimeInit> {
    if hash_only {
        return Ok(RuntimeInit {
            runtime: None,
            guard: None,
        });
    }

    // Use locally built SpacetimeDB so server supports spacetime:sys@2.0 (required by local TypeScript SDK).
    let spacetime = SpacetimeDbGuard::spawn_in_temp_data_dir();

    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().build()?;

    Ok(RuntimeInit {
        runtime: Some(runtime),
        guard: Some(spacetime),
    })
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

fn collect_task_names_in_categories(bench_root: &Path, cats: &HashSet<String>) -> Result<HashSet<String>> {
    let mut tasks = HashSet::new();
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
            tasks.insert(entry.file_name().to_string_lossy().to_ascii_lowercase());
        }
    }
    Ok(tasks)
}

fn task_selector_matches_any(selector: &str, allowed_tasks: &HashSet<String>) -> bool {
    allowed_tasks.iter().any(|task| task.starts_with(selector))
}

fn normalize_task_filter_selector(raw: &str) -> Result<String> {
    let s = raw.trim().to_ascii_lowercase();
    if s.is_empty() {
        anyhow::bail!("empty task selector");
    }
    if let Some(rest) = s.strip_prefix("t_") {
        if rest.chars().all(|c| c.is_ascii_digit()) {
            let n: u32 = rest.parse()?;
            return Ok(format!("t_{:03}", n));
        }
        if rest.chars().next().is_some_and(|c| c.is_ascii_digit())
            && rest.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        {
            return Ok(s);
        }
        anyhow::bail!("invalid task selector: {raw}");
    }
    if s.chars().all(|c| c.is_ascii_digit()) {
        let n: u32 = s.parse()?;
        return Ok(format!("t_{:03}", n));
    }
    anyhow::bail!("invalid task selector: {raw}")
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
            let allowed = collect_task_names_in_categories(bench_root, cats)?;
            let mut out: Vec<String> = match selectors {
                Some(user) => {
                    let mut selected = Vec::new();
                    for selector in user {
                        let normalized = normalize_task_filter_selector(selector)?;
                        if task_selector_matches_any(&normalized, &allowed) {
                            selected.push(normalized);
                        }
                    }
                    selected
                }
                None => {
                    let mut v: Vec<String> = allowed.into_iter().collect();
                    v.sort_unstable();
                    v
                }
            };
            out.sort();
            out.dedup();
            if out.is_empty() {
                anyhow::bail!("no tasks matched category/task filters");
            }
            Ok(Some(out))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_run_args() -> RunArgs {
        RunArgs {
            modes: None,
            lang: Lang::Rust,
            hash_only: false,
            goldens_only: false,
            force: false,
            categories: None,
            tasks: None,
            providers: None,
            models: None,
            model_source: ModelSource::Static,
            dry_run: false,
            local_analysis: false,
            route_overrides: None,
        }
    }

    fn base_config(route_overrides: Option<Vec<ModelRoute>>) -> RunConfig {
        RunConfig {
            modes: None,
            hash_only: false,
            goldens_only: false,
            lang: Lang::Rust,
            providers_filter: None,
            selectors: None,
            force: false,
            categories: None,
            model_filter: None,
            host: None,
            api_client: None,
            dry_run: false,
            local_analysis: false,
            dry_run_id: None,
            route_overrides,
        }
    }

    #[test]
    fn explicit_models_bypass_remote_model_source() {
        let mut args = base_run_args();
        args.model_source = ModelSource::Remote;
        assert!(should_fetch_remote_routes(&args));

        args.models = Some(vec![ModelGroup {
            vendor: Vendor::OpenAi,
            models: vec!["gpt-test".to_string()],
        }]);
        assert!(!should_fetch_remote_routes(&args));

        args.dry_run = true;
        assert!(!should_fetch_remote_routes(&args));
    }

    #[test]
    fn filter_routes_uses_remote_route_override() {
        let remote_route = ModelRoute::new(
            "Remote Model",
            Vendor::OpenRouter,
            "openai/remote-model",
            Some("openai/remote-model"),
        );
        let config = base_config(Some(vec![remote_route]));

        let routes = filter_routes(&config);
        assert_eq!(routes.len(), 1);
        assert_eq!(routes[0].display_name, "Remote Model");
        assert_eq!(routes[0].api_model, "openai/remote-model");
    }

    #[test]
    fn filter_routes_does_not_synthesize_duplicate_for_display_name_match() {
        let remote_route = ModelRoute::new(
            "DeepSeek V4 Flash",
            Vendor::DeepSeek,
            "deepseek-v4-flash",
            Some("deepseek/deepseek-v4-flash"),
        );
        let mut config = base_config(Some(vec![remote_route]));
        let mut allowed = HashSet::new();
        allowed.insert("deepseek v4 flash".to_string());
        let mut filter = HashMap::new();
        filter.insert(Vendor::DeepSeek, allowed);
        config.model_filter = Some(filter);

        let routes = filter_routes(&config);
        assert_eq!(routes.len(), 1);
        assert_eq!(routes[0].display_name, "DeepSeek V4 Flash");
        assert_eq!(routes[0].api_model, "deepseek-v4-flash");
    }

    #[test]
    fn category_filter_accepts_full_task_ids() {
        let root = std::env::temp_dir().join(format!(
            "llm-benchmark-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(root.join("basics").join("t_001_basic_tables")).unwrap();
        fs::create_dir_all(root.join("schema").join("t_012_product_type")).unwrap();

        let mut categories = HashSet::new();
        categories.insert("basics".to_string());
        let selectors = vec!["t_001_basic_tables".to_string(), "t_012_product_type".to_string()];

        let filtered = apply_category_filter(&root, Some(&categories), Some(&selectors)).unwrap();
        fs::remove_dir_all(&root).unwrap();

        assert_eq!(filtered, Some(vec!["t_001_basic_tables".to_string()]));
    }
}

#![allow(clippy::disallowed_macros)]

use anyhow::{bail, Context, Result};
use clap::{Args, Parser, Subcommand};
use futures::{StreamExt, TryStreamExt};
use spacetimedb_guard::SpacetimeDbGuard;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Arc;
use tokio::runtime::Runtime;
use xtask_llm_benchmark::bench::bench_route_concurrency;
use xtask_llm_benchmark::bench::runner::{
    build_goldens_only_for_lang, ensure_goldens_built_once, run_selected_or_all_for_model_async_for_lang,
};
use xtask_llm_benchmark::bench::types::{BenchRunContext, RouteRun, RunConfig};
use xtask_llm_benchmark::context::constants::{
    docs_benchmark_comment, docs_benchmark_details, docs_benchmark_summary, llm_comparison_details,
    llm_comparison_summary, ALL_MODES,
};
use xtask_llm_benchmark::context::{build_context, compute_processed_context_hash, docs_dir};
use xtask_llm_benchmark::eval::Lang;
use xtask_llm_benchmark::llm::types::Vendor;
use xtask_llm_benchmark::llm::{default_model_routes, make_provider_from_env, LlmProvider, ModelRoute};
use xtask_llm_benchmark::results::io::{update_golden_answers_on_disk, write_summary_from_details_file};
use xtask_llm_benchmark::results::{load_summary, Summary};

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

#[derive(Subcommand, Debug)]
enum Commands {
    /// Run benchmarks / build goldens / compute hashes.
    Run(RunArgs),

    /// Check-only: ensure required mode exists per language and hashes match saved run.
    CiCheck(CiCheckArgs),

    /// Quickfix CI by running a minimal OpenAI model set.
    CiQuickfix,

    /// Generate markdown comment for GitHub PR (compares against master baseline).
    CiComment(CiCommentArgs),

    /// Regenerate summary.json from details.json (optionally custom paths).
    Summary(SummaryArgs),

    /// Analyze benchmark failures and generate a human-readable markdown report.
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
}

#[derive(Args, Debug, Clone)]
struct CiCheckArgs {
    /// Optional: one or more languages (default: rust,csharp)
    #[arg(long, num_args = 1.., value_delimiter = ',')]
    lang: Option<Vec<Lang>>,
}

#[derive(Args, Debug, Clone)]
struct SummaryArgs {
    /// Optional input details.json (default: results_path_details())
    details: Option<PathBuf>,

    /// Optional output summary.json (default: results_path_summary())
    summary: Option<PathBuf>,
}

#[derive(Args, Debug, Clone)]
struct AnalyzeArgs {
    /// Input details.json file (default: docs-benchmark-details.json)
    #[arg(long)]
    details: Option<PathBuf>,

    /// Output markdown file (default: docs-benchmark-analysis.md)
    #[arg(long, short)]
    output: Option<PathBuf>,

    /// Only analyze failures for a specific language (rust, csharp)
    #[arg(long)]
    lang: Option<Lang>,
}

#[derive(Args, Debug, Clone)]
struct CiCommentArgs {
    /// Output markdown file (default: docs-benchmark-comment.md)
    #[arg(long, short)]
    output: Option<PathBuf>,

    /// Git ref to compare against for baseline (default: origin/master)
    #[arg(long, default_value = "origin/master")]
    baseline_ref: String,
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

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Run(args) => cmd_run(args),
        Commands::CiCheck(args) => cmd_ci_check(args),
        Commands::CiQuickfix => cmd_ci_quickfix(),
        Commands::CiComment(args) => cmd_ci_comment(args),
        Commands::Summary(args) => cmd_summary(args),
        Commands::Analyze(args) => cmd_analyze(args),
    }
}

/* ------------------------------ run ------------------------------ */

fn cmd_run(args: RunArgs) -> Result<()> {
    // Run command writes to llm-comparison files (for comparing LLM performance)
    let details_path = llm_comparison_details();
    let summary_path = llm_comparison_summary();

    run_benchmarks(args, &details_path, &summary_path)
}

/// Core benchmark runner used by both `run` and `ci-quickfix`
fn run_benchmarks(args: RunArgs, details_path: &Path, summary_path: &Path) -> Result<()> {
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
        details_path: details_path.to_path_buf(),
    };

    let bench_root = find_bench_root();

    let modes = config
        .modes
        .clone()
        .unwrap_or_else(|| ALL_MODES.iter().map(|s| s.to_string()).collect());

    let RuntimeInit {
        runtime,
        provider: llm_provider,
        guard,
    } = initialize_runtime_and_provider(config.hash_only, config.goldens_only)?;

    config.host = guard.as_ref().map(|g| g.host_url.clone());

    config.selectors = apply_category_filter(&bench_root, config.categories.as_ref(), config.selectors.as_deref())?;

    let selectors: Option<Vec<String>> = config.selectors.clone();
    let selectors_ref: Option<&[String]> = selectors.as_deref();

    if !config.goldens_only && !config.hash_only {
        let rt = runtime.as_ref().expect("failed to initialize runtime for goldens");
        rt.block_on(ensure_goldens_built_once(
            config.host.clone(),
            &bench_root,
            config.lang,
            selectors_ref,
        ))?;
    }

    for mode in modes {
        run_mode_benchmarks(
            &mode,
            config.lang,
            &config,
            &bench_root,
            runtime.as_ref(),
            llm_provider.as_ref(),
        )?;
    }

    if !config.goldens_only && !config.hash_only {
        fs::create_dir_all(docs_dir().join("llms"))?;

        update_golden_answers_on_disk(details_path, &bench_root, /*all=*/ true, /*overwrite=*/ true)?;

        write_summary_from_details_file(details_path, summary_path)?;
        println!("Results written to:");
        println!("  Details: {}", details_path.display());
        println!("  Summary: {}", summary_path.display());
    }

    Ok(())
}

/* --------------------------- ci-check --------------------------- */

fn cmd_ci_check(args: CiCheckArgs) -> Result<()> {
    // Check-only:
    //  - Verifies the required modes exist for each language
    //  - Computes the current context hash and compares against the saved summary hash
    //  - Does NOT run any providers/models or build goldens
    //
    // Required mode/lang combinations:
    //   Rust   → "rustdoc_json"
    //   Rust   → "docs"
    //   CSharp → "docs"

    let langs = args.lang.unwrap_or_else(|| vec![Lang::Rust, Lang::CSharp]);

    // Build the list of (lang, mode) combinations to check
    let mut checks: Vec<(Lang, &'static str)> = Vec::new();
    for lang in &langs {
        match lang {
            Lang::Rust => {
                checks.push((Lang::Rust, "rustdoc_json"));
                checks.push((Lang::Rust, "docs"));
            }
            Lang::CSharp => {
                checks.push((Lang::CSharp, "docs"));
            }
            Lang::TypeScript => {
                checks.push((Lang::TypeScript, "docs"));
            }
        }
    }

    // De-dupe, preserve order
    let mut seen = HashSet::new();
    checks.retain(|(lang, mode)| seen.insert((lang.as_str().to_string(), mode.to_string())));

    // Debug hint for how to (re)generate entries
    let hint_for = |_lang: Lang| -> &'static str { "Check DEVELOP.md for instructions on how to proceed." };

    // Load docs-benchmark summary to compare hashes against
    let summary_path = docs_benchmark_summary();
    let summary: Summary =
        load_summary(&summary_path).with_context(|| format!("load summary file at {:?}", summary_path))?;

    for (lang, mode) in checks {
        let lang_str = lang.as_str();

        // Ensure mode exists (non-empty paths)
        match xtask_llm_benchmark::context::resolve_mode_paths(mode) {
            Ok(paths) if !paths.is_empty() => {}
            Ok(_) => bail!(
                "CI check FAILED: {}/{} resolved to 0 paths.\n→ {}",
                mode,
                lang_str,
                hint_for(lang)
            ),
            Err(e) => bail!(
                "CI check FAILED: {}/{} not available: {}.\n→ {}",
                mode,
                lang_str,
                e,
                hint_for(lang)
            ),
        }

        // Compute current context hash (using processed context for lang-specific hash)
        let current_hash = compute_processed_context_hash(mode, lang)
            .with_context(|| format!("compute processed context hash for `{mode}`/{lang_str}"))?;

        // Find saved hash in summary
        let saved_hash = summary
            .by_language
            .get(lang_str)
            .and_then(|lang_sum| lang_sum.modes.get(mode))
            .map(|mode_sum| &mode_sum.hash);

        let saved_hash = match saved_hash {
            Some(h) => h,
            None => bail!(
                "CI check FAILED: no saved entry for {}/{}.\n→ Generate it with: {}",
                mode,
                lang_str,
                hint_for(lang)
            ),
        };

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

fn model_filter_from_groups(groups: Option<Vec<ModelGroup>>) -> Option<HashMap<Vendor, HashSet<String>>> {
    let groups = groups?;
    let mut out: HashMap<Vendor, HashSet<String>> = HashMap::new();

    for g in groups {
        out.entry(g.vendor).or_default().extend(g.models.into_iter());
    }
    Some(out)
}

fn cmd_ci_quickfix() -> Result<()> {
    // CI quickfix writes to docs-benchmark files (for testing documentation quality)
    let details_path = docs_benchmark_details();
    let summary_path = docs_benchmark_summary();

    println!("Running CI quickfix (GPT-5 only) for docs-benchmark...");

    // Run Rust benchmarks with rustdoc_json mode
    let rust_rustdoc_args = RunArgs {
        modes: Some(vec!["rustdoc_json".to_string()]),
        lang: Lang::Rust,
        hash_only: false,
        goldens_only: false,
        force: true,
        categories: None,
        tasks: None,
        providers: Some(vec![VendorArg(Vendor::OpenAi)]),
        models: Some(vec![ModelGroup {
            vendor: Vendor::OpenAi,
            models: vec!["gpt-5".to_string()],
        }]),
    };
    run_benchmarks(rust_rustdoc_args, &details_path, &summary_path)?;

    // Run Rust benchmarks with docs mode (markdown documentation)
    let rust_docs_args = RunArgs {
        modes: Some(vec!["docs".to_string()]),
        lang: Lang::Rust,
        hash_only: false,
        goldens_only: false,
        force: true,
        categories: None,
        tasks: None,
        providers: Some(vec![VendorArg(Vendor::OpenAi)]),
        models: Some(vec![ModelGroup {
            vendor: Vendor::OpenAi,
            models: vec!["gpt-5".to_string()],
        }]),
    };
    run_benchmarks(rust_docs_args, &details_path, &summary_path)?;

    // Run C# benchmarks with docs mode
    let csharp_args = RunArgs {
        modes: Some(vec!["docs".to_string()]),
        lang: Lang::CSharp,
        hash_only: false,
        goldens_only: false,
        force: true,
        categories: None,
        tasks: None,
        providers: Some(vec![VendorArg(Vendor::OpenAi)]),
        models: Some(vec![ModelGroup {
            vendor: Vendor::OpenAi,
            models: vec!["gpt-5".to_string()],
        }]),
    };
    run_benchmarks(csharp_args, &details_path, &summary_path)?;

    println!("CI quickfix complete. Results written to:");
    println!("  Details: {}", details_path.display());
    println!("  Summary: {}", summary_path.display());

    Ok(())
}

/* --------------------------- ci-comment --------------------------- */

fn cmd_ci_comment(args: CiCommentArgs) -> Result<()> {
    use std::process::Command;

    let summary_path = docs_benchmark_summary();
    let output_path = args.output.unwrap_or_else(docs_benchmark_comment);

    // Load current summary
    let summary: Summary =
        load_summary(&summary_path).with_context(|| format!("load summary file at {:?}", summary_path))?;

    // Try to load baseline from git ref
    let baseline: Option<Summary> = {
        let relative_path = "docs/llms/docs-benchmark-summary.json";
        let output = Command::new("git")
            .args(["show", &format!("{}:{}", args.baseline_ref, relative_path)])
            .output();

        match output {
            Ok(out) if out.status.success() => {
                let json = String::from_utf8_lossy(&out.stdout);
                match serde_json::from_str(&json) {
                    Ok(s) => {
                        println!("Loaded baseline from {}", args.baseline_ref);
                        Some(s)
                    }
                    Err(e) => {
                        println!("Warning: Could not parse baseline JSON: {}", e);
                        None
                    }
                }
            }
            Ok(out) => {
                let stderr = String::from_utf8_lossy(&out.stderr);
                println!(
                    "Note: Could not load baseline from {} (file may not exist yet): {}",
                    args.baseline_ref,
                    stderr.trim()
                );
                None
            }
            Err(e) => {
                println!("Warning: git command failed: {}", e);
                None
            }
        }
    };

    // Generate markdown
    let markdown = generate_comment_markdown(&summary, baseline.as_ref());

    // Write to file
    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&output_path, &markdown)?;
    println!("Comment markdown written to: {}", output_path.display());

    Ok(())
}

/// Generate the markdown comment for GitHub PR.
fn generate_comment_markdown(summary: &Summary, baseline: Option<&Summary>) -> String {
    // Rust with rustdoc_json mode
    let rust_rustdoc_results = summary
        .by_language
        .get("rust")
        .and_then(|l| l.modes.get("rustdoc_json"))
        .and_then(|m| m.models.get("GPT-5"));
    // Rust with docs mode (markdown documentation)
    let rust_docs_results = summary
        .by_language
        .get("rust")
        .and_then(|l| l.modes.get("docs"))
        .and_then(|m| m.models.get("GPT-5"));
    // C# with docs mode
    let csharp_results = summary
        .by_language
        .get("csharp")
        .and_then(|l| l.modes.get("docs"))
        .and_then(|m| m.models.get("GPT-5"));

    let rust_rustdoc_baseline = baseline
        .and_then(|b| b.by_language.get("rust"))
        .and_then(|l| l.modes.get("rustdoc_json"))
        .and_then(|m| m.models.get("GPT-5"));
    let rust_docs_baseline = baseline
        .and_then(|b| b.by_language.get("rust"))
        .and_then(|l| l.modes.get("docs"))
        .and_then(|m| m.models.get("GPT-5"));
    let csharp_baseline = baseline
        .and_then(|b| b.by_language.get("csharp"))
        .and_then(|l| l.modes.get("docs"))
        .and_then(|m| m.models.get("GPT-5"));

    fn format_pct(val: f32) -> String {
        format!("{:.1}%", val)
    }

    fn format_diff(current: f32, baseline: Option<f32>) -> String {
        match baseline {
            Some(b) => {
                let diff = current - b;
                if diff.abs() < 0.1 {
                    String::new()
                } else {
                    let sign = if diff > 0.0 { "+" } else { "" };
                    let arrow = if diff > 0.0 { "⬆️" } else { "⬇️" };
                    format!(" {} {}{:.1}%", arrow, sign, diff)
                }
            }
            None => String::new(),
        }
    }

    let mut md = String::new();
    md.push_str("## LLM Benchmark Results (ci-quickfix)\n\n");
    md.push_str("| Language | Mode | Category | Tests Passed | Task Pass % |\n");
    md.push_str("|----------|------|----------|--------------|-------------|\n");

    // Rust with rustdoc_json mode
    if let Some(results) = rust_rustdoc_results {
        let base_cats = rust_rustdoc_baseline.map(|b| &b.categories);

        if let Some(c) = results.categories.get("basics") {
            let b = base_cats.and_then(|cats| cats.get("basics"));
            let diff = format_diff(c.task_pass_pct, b.map(|x| x.task_pass_pct));
            md.push_str(&format!(
                "| Rust | rustdoc_json | basics | {}/{} | {}{} |\n",
                c.passed_tests,
                c.total_tests,
                format_pct(c.task_pass_pct),
                diff
            ));
        }
        if let Some(c) = results.categories.get("schema") {
            let b = base_cats.and_then(|cats| cats.get("schema"));
            let diff = format_diff(c.task_pass_pct, b.map(|x| x.task_pass_pct));
            md.push_str(&format!(
                "| Rust | rustdoc_json | schema | {}/{} | {}{} |\n",
                c.passed_tests,
                c.total_tests,
                format_pct(c.task_pass_pct),
                diff
            ));
        }
        let diff = format_diff(
            results.totals.task_pass_pct,
            rust_rustdoc_baseline.map(|b| b.totals.task_pass_pct),
        );
        md.push_str(&format!(
            "| Rust | rustdoc_json | **total** | {}/{} | **{}**{} |\n",
            results.totals.passed_tests,
            results.totals.total_tests,
            format_pct(results.totals.task_pass_pct),
            diff
        ));
    }

    // Rust with docs mode
    if let Some(results) = rust_docs_results {
        let base_cats = rust_docs_baseline.map(|b| &b.categories);

        if let Some(c) = results.categories.get("basics") {
            let b = base_cats.and_then(|cats| cats.get("basics"));
            let diff = format_diff(c.task_pass_pct, b.map(|x| x.task_pass_pct));
            md.push_str(&format!(
                "| Rust | docs | basics | {}/{} | {}{} |\n",
                c.passed_tests,
                c.total_tests,
                format_pct(c.task_pass_pct),
                diff
            ));
        }
        if let Some(c) = results.categories.get("schema") {
            let b = base_cats.and_then(|cats| cats.get("schema"));
            let diff = format_diff(c.task_pass_pct, b.map(|x| x.task_pass_pct));
            md.push_str(&format!(
                "| Rust | docs | schema | {}/{} | {}{} |\n",
                c.passed_tests,
                c.total_tests,
                format_pct(c.task_pass_pct),
                diff
            ));
        }
        let diff = format_diff(
            results.totals.task_pass_pct,
            rust_docs_baseline.map(|b| b.totals.task_pass_pct),
        );
        md.push_str(&format!(
            "| Rust | docs | **total** | {}/{} | **{}**{} |\n",
            results.totals.passed_tests,
            results.totals.total_tests,
            format_pct(results.totals.task_pass_pct),
            diff
        ));
    }

    // C# with docs mode
    if let Some(results) = csharp_results {
        let base_cats = csharp_baseline.map(|b| &b.categories);

        if let Some(c) = results.categories.get("basics") {
            let b = base_cats.and_then(|cats| cats.get("basics"));
            let diff = format_diff(c.task_pass_pct, b.map(|x| x.task_pass_pct));
            md.push_str(&format!(
                "| C# | docs | basics | {}/{} | {}{} |\n",
                c.passed_tests,
                c.total_tests,
                format_pct(c.task_pass_pct),
                diff
            ));
        }
        if let Some(c) = results.categories.get("schema") {
            let b = base_cats.and_then(|cats| cats.get("schema"));
            let diff = format_diff(c.task_pass_pct, b.map(|x| x.task_pass_pct));
            md.push_str(&format!(
                "| C# | docs | schema | {}/{} | {}{} |\n",
                c.passed_tests,
                c.total_tests,
                format_pct(c.task_pass_pct),
                diff
            ));
        }
        let diff = format_diff(
            results.totals.task_pass_pct,
            csharp_baseline.map(|b| b.totals.task_pass_pct),
        );
        md.push_str(&format!(
            "| C# | docs | **total** | {}/{} | **{}**{} |\n",
            results.totals.passed_tests,
            results.totals.total_tests,
            format_pct(results.totals.task_pass_pct),
            diff
        ));
    }

    if baseline.is_some() {
        md.push_str("\n_Compared against master branch baseline_\n");
    }
    md.push_str(&format!("\n<sub>Generated at: {}</sub>\n", summary.generated_at));

    md
}

/* --------------------------- helpers --------------------------- */

fn short_hash(s: &str) -> &str {
    &s[..s.len().min(12)]
}

/// Run benchmarks for a single mode. Results are merged into the details file.
fn run_mode_benchmarks(
    mode: &str,
    lang: Lang,
    config: &RunConfig,
    bench_root: &Path,
    runtime: Option<&Runtime>,
    llm_provider: Option<&Arc<dyn LlmProvider>>,
) -> Result<()> {
    let lang_str = lang.as_str();
    let context = build_context(mode, Some(lang))?;
    // Use processed context hash so each lang/mode combination has its own unique hash
    let hash = compute_processed_context_hash(mode, lang)
        .with_context(|| format!("compute processed context hash for `{mode}`/{}", lang_str))?;

    println!("{:<12} [{:<10}] hash: {}", mode, lang_str, short_hash(&hash));

    if config.hash_only {
        return Ok(());
    }

    if config.goldens_only {
        let rt = runtime.expect("runtime required for --goldens-only");
        let sels = config.selectors.as_deref();

        rt.block_on(build_goldens_only_for_lang(config.host.clone(), bench_root, lang, sels))?;
        println!("{:<12} [{:<10}] goldens-only build complete", mode, lang_str);
        return Ok(());
    }

    // Run benchmarks for all matching routes
    let routes = filter_routes(config);

    if routes.is_empty() {
        println!("{:<12} [{:<10}] no matching models to run", mode, lang_str);
        return Ok(());
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

    // Print summary
    for rr in &route_runs {
        let total: u32 = rr.outcomes.iter().map(|o| o.total_tests).sum();
        let passed: u32 = rr.outcomes.iter().map(|o| o.passed_tests).sum();
        let pct = if total == 0 {
            0.0
        } else {
            (passed as f32 / total as f32) * 100.0
        };
        println!("   ↳ {}: {}/{} passed ({:.1}%)", rr.route_name, passed, total, pct);
    }

    Ok(())
}

fn filter_routes(config: &RunConfig) -> Vec<ModelRoute> {
    default_model_routes()
        .iter()
        .filter(|r| config.providers_filter.as_ref().is_none_or(|f| f.contains(&r.vendor)))
        .filter(|r| {
            if let Some(map) = &config.model_filter {
                if let Some(allowed) = map.get(&r.vendor) {
                    let api = r.api_model.to_ascii_lowercase();
                    let dn = r.display_name.to_ascii_lowercase();
                    return allowed.contains(&api) || allowed.contains(&dn);
                }
            }
            true
        })
        .cloned()
        .collect()
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
    let details_path = config.details_path.clone();

    futures::stream::iter(routes.iter().cloned().map(|route| {
        let host = host.clone();
        let details_path = details_path.clone();

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
                host,
                details_path,
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
    pub provider: Option<Arc<dyn LlmProvider>>,
    pub guard: Option<SpacetimeDbGuard>,
}

fn initialize_runtime_and_provider(hash_only: bool, goldens_only: bool) -> Result<RuntimeInit> {
    if hash_only {
        return Ok(RuntimeInit {
            runtime: None,
            provider: None,
            guard: None,
        });
    }

    let spacetime = SpacetimeDbGuard::spawn_in_temp_data_dir_use_cli();

    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().build()?;

    if goldens_only {
        return Ok(RuntimeInit {
            runtime: Some(runtime),
            provider: None,
            guard: Some(spacetime),
        });
    }

    let llm_provider = make_provider_from_env()?;
    Ok(RuntimeInit {
        runtime: Some(runtime),
        provider: Some(llm_provider),
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

fn cmd_summary(args: SummaryArgs) -> Result<()> {
    // Default to llm-comparison files (the full benchmark suite)
    let in_path = args.details.unwrap_or_else(llm_comparison_details);
    let out_path = args.summary.unwrap_or_else(llm_comparison_summary);

    if let Some(parent) = out_path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("create dir {}", parent.display()))?;
    }

    write_summary_from_details_file(&in_path, &out_path)?;
    println!("Summary written to: {}", out_path.display());
    Ok(())
}

fn cmd_analyze(args: AnalyzeArgs) -> Result<()> {
    use xtask_llm_benchmark::results::schema::Results;

    let details_path = args.details.unwrap_or_else(docs_benchmark_details);
    let output_path = args.output.unwrap_or_else(|| {
        details_path
            .parent()
            .unwrap_or(Path::new("."))
            .join("docs-benchmark-analysis.md")
    });

    println!("Analyzing benchmark results from: {}", details_path.display());

    // Load the details file
    let content =
        fs::read_to_string(&details_path).with_context(|| format!("Failed to read {}", details_path.display()))?;
    let results: Results = serde_json::from_str(&content).with_context(|| "Failed to parse details.json")?;

    // Collect failures
    let mut failures: Vec<FailureInfo> = Vec::new();

    for lang_entry in &results.languages {
        // Skip if filtering by language
        if let Some(filter_lang) = &args.lang {
            if lang_entry.lang != filter_lang.as_str() {
                continue;
            }
        }

        let golden_answers = &lang_entry.golden_answers;

        for mode_entry in &lang_entry.modes {
            for model_entry in &mode_entry.models {
                for (task_id, outcome) in &model_entry.tasks {
                    if outcome.passed_tests < outcome.total_tests {
                        // This task has failures
                        let golden = golden_answers
                            .get(task_id)
                            .or_else(|| {
                                // Try with category prefix stripped
                                task_id.split('/').next_back().and_then(|t| golden_answers.get(t))
                            })
                            .map(|g| g.answer.clone());

                        failures.push(FailureInfo {
                            lang: lang_entry.lang.clone(),
                            mode: mode_entry.mode.clone(),
                            model: model_entry.name.clone(),
                            task: task_id.clone(),
                            passed: outcome.passed_tests,
                            total: outcome.total_tests,
                            llm_output: outcome.llm_output.clone(),
                            golden_answer: golden,
                            scorer_details: outcome.scorer_details.clone(),
                        });
                    }
                }
            }
        }
    }

    if failures.is_empty() {
        println!("No failures found!");
        fs::write(
            &output_path,
            "# Benchmark Analysis\n\nNo failures found. All tests passed!",
        )?;
        println!("Analysis written to: {}", output_path.display());
        return Ok(());
    }

    println!("Found {} failing test(s). Generating analysis...", failures.len());

    // Build prompt for LLM
    let prompt = build_analysis_prompt(&failures);

    // Initialize runtime and LLM provider
    let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build()?;
    let provider = make_provider_from_env()?;

    // Use a fast model for analysis
    let route = ModelRoute {
        display_name: "gpt-4o-mini",
        api_model: "gpt-4o-mini",
        vendor: Vendor::OpenAi,
    };

    use xtask_llm_benchmark::llm::prompt::BuiltPrompt;

    let built_prompt = BuiltPrompt {
        system: Some(
            "You are an expert at analyzing SpacetimeDB benchmark failures. \
            Analyze the test failures and provide actionable insights in markdown format."
                .to_string(),
        ),
        static_prefix: None,
        segments: vec![xtask_llm_benchmark::llm::segmentation::Segment::new("user", prompt)],
    };

    let analysis = runtime.block_on(provider.generate(&route, &built_prompt))?;

    // Write markdown output
    let markdown = format!(
        "# Benchmark Failure Analysis\n\n\
        Generated from: `{}`\n\n\
        ## Summary\n\n\
        - **Total failures analyzed**: {}\n\n\
        ---\n\n\
        {}\n",
        details_path.display(),
        failures.len(),
        analysis
    );

    fs::write(&output_path, markdown)?;
    println!("Analysis written to: {}", output_path.display());

    Ok(())
}

#[allow(dead_code)]
struct FailureInfo {
    lang: String,
    mode: String,
    model: String,
    task: String,
    passed: u32,
    total: u32,
    llm_output: Option<String>,
    golden_answer: Option<String>,
    scorer_details: Option<HashMap<String, xtask_llm_benchmark::eval::ScoreDetails>>,
}

/// Extract concise failure reasons from scorer_details using typed extraction.
fn extract_failure_reasons(details: &HashMap<String, xtask_llm_benchmark::eval::ScoreDetails>) -> Vec<String> {
    details
        .iter()
        .filter_map(|(scorer_name, score)| {
            score
                .failure_reason()
                .map(|reason| format!("{}: {}", scorer_name, reason))
        })
        .collect()
}

/// Categorize a failure by its type based on scorer details.
fn categorize_failure(f: &FailureInfo) -> &'static str {
    let reasons = f
        .scorer_details
        .as_ref()
        .map(extract_failure_reasons)
        .unwrap_or_default();

    let reasons_str = reasons.join(" ");
    if reasons_str.contains("tables differ") {
        "table_naming"
    } else if reasons_str.contains("timed out") {
        "timeout"
    } else if reasons_str.contains("publish failed") || reasons_str.contains("compile") {
        "compile"
    } else {
        "other"
    }
}

/// Build the analysis section for failures of a specific language/mode combination.
fn build_mode_section(lang: &str, mode: &str, failures: &[&FailureInfo], prompt: &mut String) {
    let lang_display = match lang {
        "rust" => "Rust",
        "csharp" => "C#",
        _ => lang,
    };

    prompt.push_str(&format!(
        "# {} / {} Failures ({} total)\n\n",
        lang_display,
        mode,
        failures.len()
    ));

    // Group by failure type
    let table_naming: Vec<_> = failures
        .iter()
        .filter(|f| categorize_failure(f) == "table_naming")
        .collect();
    let compile: Vec<_> = failures.iter().filter(|f| categorize_failure(f) == "compile").collect();
    let timeout: Vec<_> = failures.iter().filter(|f| categorize_failure(f) == "timeout").collect();
    let other: Vec<_> = failures.iter().filter(|f| categorize_failure(f) == "other").collect();

    // Table naming issues - show detailed examples
    if !table_naming.is_empty() {
        prompt.push_str(&format!("## Table Naming Issues ({} failures)\n\n", table_naming.len()));
        prompt.push_str("The LLM is using incorrect table names. Examples:\n\n");

        // Show up to 3 detailed examples
        for f in table_naming.iter().take(3) {
            let reasons = f
                .scorer_details
                .as_ref()
                .map(extract_failure_reasons)
                .unwrap_or_default();
            prompt.push_str(&format!("### {}\n", f.task));
            prompt.push_str(&format!("**Failure**: {}\n\n", reasons.join(", ")));

            if let Some(llm_out) = &f.llm_output {
                let truncated = truncate_str(llm_out, 1200);
                prompt.push_str(&format!("**LLM Output**:\n```\n{}\n```\n\n", truncated));
            }

            if let Some(golden) = &f.golden_answer {
                let truncated = truncate_str(golden, 1200);
                prompt.push_str(&format!("**Expected**:\n```\n{}\n```\n\n", truncated));
            }
        }
        if table_naming.len() > 3 {
            prompt.push_str(&format!(
                "**Additional similar failures**: {}\n\n",
                table_naming
                    .iter()
                    .skip(3)
                    .map(|f| f.task.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
    }

    // Compile/publish errors - show detailed examples with full error messages
    if !compile.is_empty() {
        prompt.push_str(&format!("## Compile/Publish Errors ({} failures)\n\n", compile.len()));

        for f in compile.iter().take(3) {
            let reasons = f
                .scorer_details
                .as_ref()
                .map(extract_failure_reasons)
                .unwrap_or_default();
            prompt.push_str(&format!("### {}\n", f.task));
            prompt.push_str(&format!("**Error**: {}\n\n", reasons.join(", ")));

            if let Some(llm_out) = &f.llm_output {
                let truncated = truncate_str(llm_out, 1500);
                prompt.push_str(&format!("**LLM Output**:\n```\n{}\n```\n\n", truncated));
            }

            if let Some(golden) = &f.golden_answer {
                let truncated = truncate_str(golden, 1500);
                prompt.push_str(&format!("**Expected (golden)**:\n```\n{}\n```\n\n", truncated));
            }
        }
        if compile.len() > 3 {
            prompt.push_str(&format!(
                "**Additional compile failures**: {}\n\n",
                compile
                    .iter()
                    .skip(3)
                    .map(|f| f.task.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
    }

    // Timeout issues
    if !timeout.is_empty() {
        prompt.push_str(&format!("## Timeout Issues ({} failures)\n\n", timeout.len()));
        prompt.push_str("These tasks timed out during execution:\n");
        for f in &timeout {
            prompt.push_str(&format!("- {}\n", f.task));
        }
        prompt.push('\n');
    }

    // Other failures - show detailed examples
    if !other.is_empty() {
        prompt.push_str(&format!("## Other Failures ({} failures)\n\n", other.len()));

        // Show up to 5 detailed examples for "other" since they're varied
        for f in other.iter().take(5) {
            let reasons = f
                .scorer_details
                .as_ref()
                .map(extract_failure_reasons)
                .unwrap_or_default();
            prompt.push_str(&format!("### {} - {}/{} tests passed\n", f.task, f.passed, f.total));
            prompt.push_str(&format!("**Failure reason**: {}\n\n", reasons.join(", ")));

            if let Some(llm_out) = &f.llm_output {
                let truncated = truncate_str(llm_out, 1200);
                prompt.push_str(&format!("**LLM Output**:\n```\n{}\n```\n\n", truncated));
            }

            if let Some(golden) = &f.golden_answer {
                let truncated = truncate_str(golden, 1200);
                prompt.push_str(&format!("**Expected (golden)**:\n```\n{}\n```\n\n", truncated));
            }
        }
        if other.len() > 5 {
            prompt.push_str(&format!(
                "**Additional failures**: {}\n\n",
                other
                    .iter()
                    .skip(5)
                    .map(|f| f.task.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
    }
}

/// Truncate a string to max_len chars, adding "..." if truncated.
fn truncate_str(s: &str, max_len: usize) -> String {
    if s.len() > max_len {
        format!("{}...", &s[..max_len])
    } else {
        s.to_string()
    }
}

fn build_analysis_prompt(failures: &[FailureInfo]) -> String {
    let mut prompt = String::from(
        "Analyze the following SpacetimeDB benchmark test failures, organized by language and mode.\n\n\
        IMPORTANT: For each failure you analyze, you MUST include the actual code examples inline to illustrate the problem.\n\
        Show what the LLM generated vs what was expected, highlighting the specific differences.\n\n\
        Focus on SPECIFIC, ACTIONABLE documentation changes.\n\n",
    );

    // Group failures by language AND mode
    let rust_rustdoc_failures: Vec<_> = failures
        .iter()
        .filter(|f| f.lang == "rust" && f.mode == "rustdoc_json")
        .collect();
    let rust_docs_failures: Vec<_> = failures
        .iter()
        .filter(|f| f.lang == "rust" && f.mode == "docs")
        .collect();
    let csharp_failures: Vec<_> = failures
        .iter()
        .filter(|f| f.lang == "csharp" && f.mode == "docs")
        .collect();

    // Build sections for each language/mode combination
    if !rust_rustdoc_failures.is_empty() {
        build_mode_section("rust", "rustdoc_json", &rust_rustdoc_failures, &mut prompt);
    }

    if !rust_docs_failures.is_empty() {
        build_mode_section("rust", "docs", &rust_docs_failures, &mut prompt);
    }

    if !csharp_failures.is_empty() {
        build_mode_section("csharp", "docs", &csharp_failures, &mut prompt);
    }

    prompt.push_str(
        "\n---\n\n## Instructions for your analysis:\n\n\
        For EACH failure or group of similar failures:\n\n\
        1. **The generated code**: The actual LLM-generated code\n\
        2. **The golden example**: The expected golden answer\n\
        3. **The error**: The error message or failure reason (if provided above)\n\
        4. **Explain the difference**: What specific API/syntax was wrong and caused the failure?\n\
        5. **Root cause**: What's missing or unclear in the documentation?\n\
        6. **Recommendation**: Specific fix\n\n\
        Group similar failures together (e.g., if multiple tests fail due to the same issue).\n\
        Use code blocks with syntax highlighting (```rust or ```csharp).\n",
    );

    prompt
}

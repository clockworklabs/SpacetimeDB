use std::time::{SystemTime, UNIX_EPOCH};

use clap::{Args, Parser, Subcommand};
use spacetimedb_dst::{
    config::RunConfig,
    seed::DstSeed,
    targets::descriptor::{RelationalDbConcurrentDescriptor, TargetDescriptor},
};

#[derive(Parser, Debug)]
#[command(name = "spacetimedb-dst")]
#[command(about = "Run deterministic simulation targets")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    Run(RunArgs),
}

#[derive(Args, Debug)]
struct RunArgs {
    #[arg(long, help = "Seed for generated choices. Defaults to wall-clock time.")]
    seed: Option<u64>,
    #[arg(
        long,
        help = "Wall-clock soak budget such as 500ms, 10s, 5m, or 1h. Use --max-interactions for exact replay."
    )]
    duration: Option<String>,
    #[arg(long, help = "Deterministic interaction budget. Preferred for replayable failures.")]
    max_interactions: Option<usize>,
}

fn main() -> anyhow::Result<()> {
    init_tracing();
    match Cli::parse().command {
        Command::Run(args) => run_command(args),
    }
}

fn init_tracing() {
    use tracing_subscriber::{fmt, EnvFilter};

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let _ = fmt()
        .with_env_filter(filter)
        .with_target(false)
        .with_thread_ids(false)
        .with_thread_names(false)
        .compact()
        .try_init();
}

fn run_command(args: RunArgs) -> anyhow::Result<()> {
    let seed = resolve_seed(args.seed);
    let config = build_config(args.duration.as_deref(), args.max_interactions)?;

    run_prepared_target::<RelationalDbConcurrentDescriptor>(seed, (), config)
}

fn run_prepared_target<D: TargetDescriptor>(
    seed: DstSeed,
    scenario: D::Scenario,
    config: RunConfig,
) -> anyhow::Result<()>
where
    D: 'static,
    D::Scenario: Send + 'static,
{
    D::prepare(seed, &scenario, &config)?;
    std::thread::spawn(move || {
        let mut runtime = spacetimedb_dst::sim::Runtime::new(seed)?;
        runtime.block_on(run_target::<D>(seed, scenario, config))
    })
    .join()
    .unwrap_or_else(|payload| std::panic::resume_unwind(payload))
}

fn resolve_seed(seed: Option<u64>) -> DstSeed {
    seed.map(DstSeed).unwrap_or_else(|| {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time went backwards")
            .as_nanos() as u64;
        DstSeed(nanos)
    })
}

fn build_config(duration: Option<&str>, max_interactions: Option<usize>) -> anyhow::Result<RunConfig> {
    Ok(match (duration, max_interactions) {
        (Some(duration), Some(max_interactions)) => RunConfig {
            max_interactions: Some(max_interactions),
            max_duration_ms: Some(spacetimedb_dst::config::parse_duration_spec(duration)?.as_millis() as u64),
            ..Default::default()
        },
        (Some(duration), None) => RunConfig::with_duration_spec(duration)?,
        (None, Some(max_interactions)) => RunConfig::with_max_interactions(max_interactions),
        (None, None) => RunConfig::with_max_interactions(1_000),
    })
}

#[allow(clippy::disallowed_macros)]
async fn run_target<D: TargetDescriptor>(
    seed: DstSeed,
    scenario: D::Scenario,
    config: RunConfig,
) -> anyhow::Result<()> {
    let line = D::run_streaming(seed, scenario, config).await?;
    println!("{line}");
    Ok(())
}

use std::time::{SystemTime, UNIX_EPOCH};

use clap::{Args, Parser, Subcommand};
use spacetimedb_dst::{
    config::{CommitlogFaultProfile, RunConfig},
    targets::descriptor::{RelationalDbCommitlogDescriptor, TargetDescriptor},
    workload::table_ops::TableScenarioId,
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
    #[arg(long, default_value = RelationalDbCommitlogDescriptor::NAME, help = "Target to run.")]
    target: String,
    #[arg(long, help = "Seed for generated choices. Defaults to wall-clock time.")]
    seed: Option<u64>,
    #[arg(
        long,
        help = "Wall-clock soak budget such as 500ms, 10s, 5m, or 1h. Use --max-interactions for exact replay."
    )]
    duration: Option<String>,
    #[arg(long, help = "Deterministic interaction budget. Preferred for replayable failures.")]
    max_interactions: Option<usize>,
    #[arg(long, help = "Scenario to run [default: random-crud]")]
    scenario: Option<String>,
    #[arg(
        long,
        default_value = "default",
        help = "Commitlog fault profile: off, light, default, or aggressive."
    )]
    commitlog_fault_profile: String,
    #[arg(
        long,
        default_value = "30s",
        help = "Virtual-time watchdog for one harness phase, such as 500ms, 30s, or off."
    )]
    harness_phase_timeout: String,
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
    resolve_target(&args.target)?;
    let seed = resolve_seed(args.seed);
    let profile = CommitlogFaultProfile::parse(&args.commitlog_fault_profile)?;
    let phase_timeout_ms = parse_optional_duration_spec(&args.harness_phase_timeout)?;
    let config = build_config(
        args.duration.as_deref(),
        args.max_interactions,
        profile,
        phase_timeout_ms,
    )?;
    let scenario = resolve_scenario(args.scenario.as_deref())?;

    run_prepared_target::<RelationalDbCommitlogDescriptor>(seed, scenario, config)
}

fn run_prepared_target<D>(seed: u64, scenario: D::Scenario, config: RunConfig) -> anyhow::Result<()>
where
    D: TargetDescriptor + 'static,
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

fn resolve_seed(seed: Option<u64>) -> u64 {
    seed.unwrap_or_else(|| {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time went backwards")
            .as_nanos() as u64
    })
}

fn resolve_target(target: &str) -> anyhow::Result<()> {
    if target == RelationalDbCommitlogDescriptor::NAME {
        Ok(())
    } else {
        anyhow::bail!(
            "unsupported target: {target}; expected: {}",
            RelationalDbCommitlogDescriptor::NAME
        )
    }
}

fn resolve_scenario(scenario: Option<&str>) -> anyhow::Result<TableScenarioId> {
    match scenario {
        Some(value) => TableScenarioId::parse(value),
        None => Ok(TableScenarioId::default()),
    }
}

fn parse_optional_duration_spec(spec: &str) -> anyhow::Result<Option<u64>> {
    match spec {
        "off" | "none" => Ok(None),
        _ => Ok(Some(
            spacetimedb_dst::config::parse_duration_spec(spec)?.as_millis() as u64
        )),
    }
}

fn build_config(
    duration: Option<&str>,
    max_interactions: Option<usize>,
    commitlog_fault_profile: CommitlogFaultProfile,
    harness_phase_timeout_ms: Option<u64>,
) -> anyhow::Result<RunConfig> {
    let mut config = match (duration, max_interactions) {
        (Some(duration), Some(max_interactions)) => RunConfig {
            max_interactions: Some(max_interactions),
            max_duration_ms: Some(spacetimedb_dst::config::parse_duration_spec(duration)?.as_millis() as u64),
            harness_phase_timeout_ms,
            commitlog_fault_profile,
        },
        (Some(duration), None) => RunConfig::with_duration_spec(duration)?,
        (None, Some(max_interactions)) => RunConfig::with_max_interactions(max_interactions),
        (None, None) => RunConfig::with_max_interactions(1_000),
    };
    config.commitlog_fault_profile = commitlog_fault_profile;
    config.harness_phase_timeout_ms = harness_phase_timeout_ms;
    Ok(config)
}

#[allow(clippy::disallowed_macros)]
async fn run_target<D: TargetDescriptor>(seed: u64, scenario: D::Scenario, config: RunConfig) -> anyhow::Result<()> {
    let line = D::run_streaming(seed, scenario, config).await?;
    println!("{line}");
    Ok(())
}

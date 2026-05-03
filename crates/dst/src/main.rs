use std::{
    future::Future,
    time::{SystemTime, UNIX_EPOCH},
};

use clap::{Args, Parser, Subcommand, ValueEnum};
use spacetimedb_dst::{
    config::{CommitlogFaultProfile, RunConfig},
    seed::DstSeed,
    targets::descriptor::{RelationalDbCommitlogDescriptor, StandaloneHostDescriptor, TargetDescriptor},
    workload::{module_ops::HostScenarioId, table_ops::TableScenarioId},
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

#[derive(Args, Debug, Clone)]
struct TargetArgs {
    #[arg(long, value_enum, default_value_t = TargetKind::RelationalDbCommitlog)]
    target: TargetKind,
    #[arg(long, value_enum, default_value_t = ScenarioKind::RandomCrud)]
    scenario: ScenarioKind,
}

#[derive(Args, Debug)]
struct RunArgs {
    #[command(flatten)]
    target: TargetArgs,
    #[arg(long, help = "Seed for generated choices. Defaults to wall-clock time.")]
    seed: Option<u64>,
    #[arg(
        long,
        help = "Wall-clock soak budget such as 500ms, 10s, 5m, or 1h. Use --max-interactions for exact replay."
    )]
    duration: Option<String>,
    #[arg(long, help = "Deterministic interaction budget. Preferred for replayable failures.")]
    max_interactions: Option<usize>,
    #[arg(
        long,
        value_enum,
        default_value_t = CommitlogFaultProfileKind::Default,
        help = "Commitlog disk-fault profile for commitlog-backed targets."
    )]
    commitlog_fault_profile: CommitlogFaultProfileKind,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
enum TargetKind {
    RelationalDbCommitlog,
    StandaloneHost,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
enum ScenarioKind {
    RandomCrud,
    IndexedRanges,
    Banking,
    HostSmoke,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
enum CommitlogFaultProfileKind {
    Off,
    Light,
    Default,
    Aggressive,
}

impl From<CommitlogFaultProfileKind> for CommitlogFaultProfile {
    fn from(profile: CommitlogFaultProfileKind) -> Self {
        match profile {
            CommitlogFaultProfileKind::Off => Self::Off,
            CommitlogFaultProfileKind::Light => Self::Light,
            CommitlogFaultProfileKind::Default => Self::Default,
            CommitlogFaultProfileKind::Aggressive => Self::Aggressive,
        }
    }
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
    let config = build_config(
        args.duration.as_deref(),
        args.max_interactions,
        args.commitlog_fault_profile,
    )?;

    match args.target.target {
        TargetKind::RelationalDbCommitlog => {
            let scenario = map_table_scenario(args.target.scenario)?;
            run_prepared_target::<RelationalDbCommitlogDescriptor>(seed, scenario, config)
        }
        TargetKind::StandaloneHost => {
            let scenario = map_host_scenario(args.target.scenario)?;
            run_prepared_target::<StandaloneHostDescriptor>(seed, scenario, config)
        }
    }
}

fn run_prepared_target<D: TargetDescriptor>(
    seed: DstSeed,
    scenario: D::Scenario,
    config: RunConfig,
) -> anyhow::Result<()> {
    D::prepare(seed, &scenario, &config)?;
    run_in_runtime(seed, run_target::<D>(seed, scenario, config))
}

#[cfg(madsim)]
fn run_in_runtime<F, T>(seed: DstSeed, future: F) -> anyhow::Result<T>
where
    F: Future<Output = anyhow::Result<T>>,
{
    let mut runtime = madsim::runtime::Runtime::with_seed_and_config(seed.0, madsim::Config::default());
    runtime.set_allow_system_thread(true);
    runtime.block_on(future)
}

#[cfg(not(madsim))]
fn run_in_runtime<F, T>(_seed: DstSeed, future: F) -> anyhow::Result<T>
where
    F: Future<Output = anyhow::Result<T>>,
{
    tokio::runtime::Runtime::new()?.block_on(future)
}

fn map_table_scenario(scenario: ScenarioKind) -> anyhow::Result<TableScenarioId> {
    match scenario {
        ScenarioKind::RandomCrud => Ok(TableScenarioId::RandomCrud),
        ScenarioKind::IndexedRanges => Ok(TableScenarioId::IndexedRanges),
        ScenarioKind::Banking => Ok(TableScenarioId::Banking),
        ScenarioKind::HostSmoke => anyhow::bail!("scenario host-smoke is only valid for --target standalone-host"),
    }
}

fn map_host_scenario(scenario: ScenarioKind) -> anyhow::Result<HostScenarioId> {
    match scenario {
        ScenarioKind::HostSmoke => Ok(HostScenarioId::HostSmoke),
        _ => anyhow::bail!("target standalone-host only supports --scenario host-smoke"),
    }
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

fn build_config(
    duration: Option<&str>,
    max_interactions: Option<usize>,
    commitlog_fault_profile: CommitlogFaultProfileKind,
) -> anyhow::Result<RunConfig> {
    let config = match (duration, max_interactions) {
        (Some(duration), Some(max_interactions)) => RunConfig {
            max_interactions: Some(max_interactions),
            max_duration_ms: Some(spacetimedb_dst::config::parse_duration_spec(duration)?.as_millis() as u64),
            ..Default::default()
        },
        (Some(duration), None) => RunConfig::with_duration_spec(duration)?,
        (None, Some(max_interactions)) => RunConfig::with_max_interactions(max_interactions),
        (None, None) => RunConfig::with_max_interactions(1_000),
    };
    Ok(config.with_commitlog_fault_profile(commitlog_fault_profile.into()))
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

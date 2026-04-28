use std::time::{SystemTime, UNIX_EPOCH};

use clap::{Args, Parser, Subcommand, ValueEnum};
use spacetimedb_dst::{
    config::RunConfig,
    seed::DstSeed,
    targets::descriptor::{DatastoreDescriptor, RelationalDbCommitlogDescriptor, StandaloneHostDescriptor, TargetDescriptor},
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
    #[arg(long, value_enum, default_value_t = TargetKind::Datastore)]
    target: TargetKind,
    #[arg(long, value_enum, default_value_t = ScenarioKind::RandomCrud)]
    scenario: ScenarioKind,
}

#[derive(Args, Debug)]
struct RunArgs {
    #[command(flatten)]
    target: TargetArgs,
    #[arg(long)]
    seed: Option<u64>,
    #[arg(long)]
    duration: Option<String>,
    #[arg(long)]
    max_interactions: Option<usize>,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
enum TargetKind {
    Datastore,
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

    match args.target.target {
        TargetKind::Datastore => {
            let scenario = map_table_scenario(args.target.scenario)?;
            run_target::<DatastoreDescriptor>(seed, scenario, config)
        }
        TargetKind::RelationalDbCommitlog => {
            let scenario = map_table_scenario(args.target.scenario)?;
            run_target::<RelationalDbCommitlogDescriptor>(seed, scenario, config)
        }
        TargetKind::StandaloneHost => {
            let scenario = map_host_scenario(args.target.scenario)?;
            run_target::<StandaloneHostDescriptor>(seed, scenario, config)
        }
    }
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

fn build_config(duration: Option<&str>, max_interactions: Option<usize>) -> anyhow::Result<RunConfig> {
    match (duration, max_interactions) {
        (Some(duration), Some(max_interactions)) => Ok(RunConfig {
            max_interactions: Some(max_interactions),
            max_duration_ms: Some(spacetimedb_dst::config::parse_duration_spec(duration)?.as_millis() as u64),
        }),
        (Some(duration), None) => RunConfig::with_duration_spec(duration),
        (None, Some(max_interactions)) => Ok(RunConfig::with_max_interactions(max_interactions)),
        (None, None) => Ok(RunConfig::with_max_interactions(1_000)),
    }
}

fn run_target<D: TargetDescriptor>(
    seed: DstSeed,
    scenario: D::Scenario,
    config: RunConfig,
) -> anyhow::Result<()> {
    let line = D::run_streaming(seed, scenario, config)?;
    println!("{line}");
    Ok(())
}

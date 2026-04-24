use std::time::{SystemTime, UNIX_EPOCH};

use clap::{Args, Parser, Subcommand, ValueEnum};
use spacetimedb_dst::{
    config::RunConfig,
    seed::DstSeed,
    targets::descriptor::{DatastoreDescriptor, RelationalDbCommitlogDescriptor, TargetDescriptor},
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
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
enum ScenarioKind {
    RandomCrud,
    IndexedRanges,
    Banking,
}

impl From<ScenarioKind> for TableScenarioId {
    fn from(value: ScenarioKind) -> Self {
        match value {
            ScenarioKind::RandomCrud => TableScenarioId::RandomCrud,
            ScenarioKind::IndexedRanges => TableScenarioId::IndexedRanges,
            ScenarioKind::Banking => TableScenarioId::Banking,
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
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
    let scenario = TableScenarioId::from(args.target.scenario);

    match args.target.target {
        TargetKind::Datastore => run_target::<DatastoreDescriptor>(seed, scenario, config),
        TargetKind::RelationalDbCommitlog => run_target::<RelationalDbCommitlogDescriptor>(seed, scenario, config),
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

fn run_target<D: TargetDescriptor<Scenario = TableScenarioId>>(
    seed: DstSeed,
    scenario: TableScenarioId,
    config: RunConfig,
) -> anyhow::Result<()> {
    let line = D::run_streaming(seed, scenario, config)?;
    println!("{line}");
    Ok(())
}

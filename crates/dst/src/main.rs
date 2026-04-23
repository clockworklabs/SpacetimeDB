use std::{
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use clap::{Args, Parser, Subcommand, ValueEnum};
use spacetimedb_dst::{
    config::RunConfig,
    seed::DstSeed,
    targets::{datastore, relational_db, relational_db_commitlog},
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
    Replay(ReplayArgs),
    Shrink(ShrinkArgs),
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
    #[arg(long)]
    save_case: Option<PathBuf>,
}

#[derive(Args, Debug)]
struct ReplayArgs {
    #[command(flatten)]
    target: TargetArgs,
    path: PathBuf,
}

#[derive(Args, Debug)]
struct ShrinkArgs {
    #[command(flatten)]
    target: TargetArgs,
    path: PathBuf,
    #[arg(long)]
    save_shrunk: Option<PathBuf>,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
enum TargetKind {
    Datastore,
    RelationalDb,
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

fn main() -> anyhow::Result<()> {
    init_tracing();
    match Cli::parse().command {
        Command::Run(args) => run_command(args),
        Command::Replay(args) => replay_command(args),
        Command::Shrink(args) => shrink_command(args),
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
        TargetKind::Datastore => run_datastore(seed, scenario, config, args.save_case),
        TargetKind::RelationalDb => run_relational(seed, scenario, config, args.save_case),
        TargetKind::RelationalDbCommitlog => run_relational_commitlog(seed, scenario, config, args.save_case),
    }
}

fn replay_command(args: ReplayArgs) -> anyhow::Result<()> {
    match args.target.target {
        TargetKind::Datastore => replay_datastore(&args.path),
        TargetKind::RelationalDb => replay_relational(&args.path),
        TargetKind::RelationalDbCommitlog => replay_relational_commitlog(&args.path),
    }
}

fn shrink_command(args: ShrinkArgs) -> anyhow::Result<()> {
    match args.target.target {
        TargetKind::Datastore => shrink_datastore(&args.path, args.save_shrunk.as_ref()),
        TargetKind::RelationalDb => shrink_relational(&args.path, args.save_shrunk.as_ref()),
        TargetKind::RelationalDbCommitlog => shrink_relational_commitlog(&args.path, args.save_shrunk.as_ref()),
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

fn run_datastore(
    seed: DstSeed,
    scenario: TableScenarioId,
    config: RunConfig,
    save_case: Option<PathBuf>,
) -> anyhow::Result<()> {
    if save_case.is_some() {
        anyhow::bail!("save-case is not supported in streaming run mode");
    }
    let outcome = datastore::run_generated_with_config_and_scenario(seed, scenario, config)?;
    println!(
        "ok target=datastore seed={} tables={} row_counts={:?}",
        seed.0,
        outcome.final_rows.len(),
        outcome.final_row_counts
    );
    Ok(())
}

fn run_relational(
    seed: DstSeed,
    scenario: TableScenarioId,
    config: RunConfig,
    save_case: Option<PathBuf>,
) -> anyhow::Result<()> {
    if save_case.is_some() {
        anyhow::bail!("save-case is not supported in streaming run mode");
    }
    let outcome = relational_db::run_generated_with_config_and_scenario(seed, scenario, config)?;
    println!(
        "ok target=relational_db seed={} tables={} row_counts={:?}",
        seed.0,
        outcome.final_rows.len(),
        outcome.final_row_counts
    );
    Ok(())
}

fn run_relational_commitlog(
    seed: DstSeed,
    scenario: TableScenarioId,
    config: RunConfig,
    save_case: Option<PathBuf>,
) -> anyhow::Result<()> {
    if save_case.is_some() {
        anyhow::bail!("save-case is not supported in streaming run mode");
    }
    let outcome = relational_db_commitlog::run_generated_with_config_and_scenario(seed, scenario, config)?;
    println!(
        "ok target=relational_db_commitlog seed={} steps={} durable_commits={} replay_tables={}",
        seed.0, outcome.applied_steps, outcome.durable_commit_count, outcome.replay_table_count
    );
    Ok(())
}

fn replay_datastore(path: &Path) -> anyhow::Result<()> {
    let case = datastore::load_case(path)?;
    replay_datastore_case(&case)
}

fn replay_relational(path: &Path) -> anyhow::Result<()> {
    let case = relational_db::load_case(path)?;
    replay_relational_case(&case)
}

fn replay_relational_commitlog(path: &Path) -> anyhow::Result<()> {
    let case = relational_db_commitlog::load_case(path)?;
    replay_relational_commitlog_case(&case)
}

fn replay_datastore_case(case: &datastore::DatastoreSimulatorCase) -> anyhow::Result<()> {
    match datastore::run_case_detailed(case) {
        Ok(_) => {
            println!(
                "ok target=datastore seed={} steps={}",
                case.seed.0,
                case.interactions.len()
            );
            Ok(())
        }
        Err(failure) => {
            println!(
                "fail target=datastore seed={} step={} reason={}",
                case.seed.0, failure.step_index, failure.reason
            );
            anyhow::bail!("datastore case failed")
        }
    }
}

fn replay_relational_case(case: &relational_db::RelationalDbSimulatorCase) -> anyhow::Result<()> {
    match relational_db::run_case_detailed(case) {
        Ok(_) => {
            println!(
                "ok target=relational_db seed={} steps={}",
                case.seed.0,
                case.interactions.len()
            );
            Ok(())
        }
        Err(failure) => {
            println!(
                "fail target=relational_db seed={} step={} reason={}",
                case.seed.0, failure.step_index, failure.reason
            );
            anyhow::bail!("relational_db case failed")
        }
    }
}

fn replay_relational_commitlog_case(case: &relational_db_commitlog::RelationalDbCommitlogCase) -> anyhow::Result<()> {
    match relational_db_commitlog::run_case_detailed(case) {
        Ok(outcome) => {
            println!(
                "ok target=relational_db_commitlog seed={} steps={} durable_commits={} replay_tables={}",
                case.seed.0, outcome.applied_steps, outcome.durable_commit_count, outcome.replay_table_count
            );
            Ok(())
        }
        Err(failure) => {
            println!(
                "fail target=relational_db_commitlog seed={} step={} reason={}",
                case.seed.0, failure.step_index, failure.reason
            );
            anyhow::bail!("relational_db_commitlog case failed")
        }
    }
}

fn shrink_datastore(path: &Path, save_shrunk: Option<&PathBuf>) -> anyhow::Result<()> {
    let case = datastore::load_case(path)?;
    let failure = datastore::run_case_detailed(&case).expect_err("shrink needs failing datastore case");
    let shrunk = datastore::shrink_failure(&case, &failure)?;
    let out = shrunk_path(path, save_shrunk);
    datastore::save_case(&out, &shrunk)?;
    println!("shrunk_case={}", out.display());
    Ok(())
}

fn shrink_relational(path: &Path, save_shrunk: Option<&PathBuf>) -> anyhow::Result<()> {
    let case = relational_db::load_case(path)?;
    let failure = relational_db::run_case_detailed(&case).expect_err("shrink needs failing relational_db case");
    let shrunk = relational_db::shrink_failure(&case, &failure)?;
    let out = shrunk_path(path, save_shrunk);
    relational_db::save_case(&out, &shrunk)?;
    println!("shrunk_case={}", out.display());
    Ok(())
}

fn shrink_relational_commitlog(path: &Path, save_shrunk: Option<&PathBuf>) -> anyhow::Result<()> {
    let case = relational_db_commitlog::load_case(path)?;
    let failure = relational_db_commitlog::run_case_detailed(&case)
        .expect_err("shrink needs failing relational_db_commitlog case");
    let shrunk = relational_db_commitlog::shrink_failure(&case, &failure)?;
    let out = shrunk_path(path, save_shrunk);
    relational_db_commitlog::save_case(&out, &shrunk)?;
    println!("shrunk_case={}", out.display());
    Ok(())
}

fn shrunk_path(default_input: &Path, explicit: Option<&PathBuf>) -> PathBuf {
    explicit.cloned().unwrap_or_else(|| {
        let mut path = default_input.as_os_str().to_os_string();
        path.push(".shrunk.json");
        PathBuf::from(path)
    })
}

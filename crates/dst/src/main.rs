use std::time::{SystemTime, UNIX_EPOCH};

use clap::{Args, Parser, Subcommand};
use spacetimedb_runtime::sim::Rng;

mod engine;
mod schema;
mod sim;
mod traits;

use crate::{engine::EngineTest, traits::TestSuite};

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
    #[arg(long, help = "Deterministic interaction budget.")]
    max_interactions: Option<usize>,
}

fn main() -> anyhow::Result<()> {
    init_tracing();
    match Cli::parse().command {
        Command::Run(args) => run_command(args),
    }
}

fn init_tracing() {}

fn run_command(args: RunArgs) -> anyhow::Result<()> {
    let seed = resolve_seed(args.seed);
    let config = RunConfig {
        max_interactions: args.max_interactions,
        seed,
    };

    eprintln!("seed: {}", config.seed);

    // Generate schema from seed.
    let rng = Rng::new(config.seed);

    let test = EngineTest {};
    test.run(rng)?;
    Ok(())
}

fn resolve_seed(seed: Option<u64>) -> u64 {
    seed.unwrap_or_else(|| {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time went backwards")
            .as_nanos() as u64
    })
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RunConfig {
    pub max_interactions: Option<usize>,
    pub seed: u64,
}

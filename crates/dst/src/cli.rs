use std::time::{SystemTime, UNIX_EPOCH};

use clap::{Args, Parser, Subcommand};
use spacetimedb_runtime::{
    sim::{Rng, Runtime as SimRuntime},
    sim_std,
};

use crate::{engine::EngineTest, traits::TestSuite};

#[derive(Parser, Debug)]
#[command(name = "spacetimedb-dst")]
#[command(about = "Run deterministic simulation targets")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    Run(RunArgs),
}

#[derive(Args, Debug)]
pub struct RunArgs {
    #[arg(long, help = "Seed for generated choices. Defaults to wall-clock time.")]
    pub seed: Option<u64>,
    #[arg(long, help = "Deterministic interaction budget.")]
    pub max_interactions: Option<usize>,
}

pub fn run_command_blocking(args: RunArgs) -> anyhow::Result<()> {
    let seed = resolve_seed(args.seed);
    let mut runtime = SimRuntime::new(seed);
    sim_std::block_on(
        &mut runtime,
        run_command(RunArgs {
            seed: Some(seed),
            max_interactions: args.max_interactions,
        }),
    )
}

pub async fn run_command(args: RunArgs) -> anyhow::Result<()> {
    let seed = resolve_seed(args.seed);
    let config = RunConfig {
        max_interactions: args.max_interactions,
        seed,
    };

    tracing::info!(?config, "initial run config");

    let rng = Rng::new(config.seed);
    let max_interactions = config.max_interactions.unwrap_or(usize::MAX);

    let test = EngineTest;
    test.run(rng, max_interactions).await?;
    Ok(())
}

pub fn resolve_seed(seed: Option<u64>) -> u64 {
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

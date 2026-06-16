use std::time::{SystemTime, UNIX_EPOCH};

use clap::{Args, Parser, Subcommand};
use spacetimedb_runtime::sim::Runtime;

mod core;
mod target;

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
    #[arg(long, default_value = Engine::NAME, help = "Target to run.")]
    target: String,
    #[arg(long, help = "Seed for generated choices. Defaults to wall-clock time.")]
    seed: Option<u64>,
    #[arg(long, help = "Deterministic interaction budget. Preferred for replayable failures.")]
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
    //resolve_target(&args.target)?;
    let seed = resolve_seed(args.seed);
    let config = RunConfig {
        max_interactions: args.max_interactions,
        seed,
    };

    run_prepared_target::<Engine>(config)
}

fn run_prepared_target<T>(config: RunConfig) -> anyhow::Result<()>
where
    T: Target + 'static,
{
    T::prepare(&config)?;
    std::thread::spawn(move || {
        let mut runtime = Runtime::new(config.seed);
        runtime.block_on(run_target::<T>(config))
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

//fn resolve_target(target: &str) -> anyhow::Result<()> {
//    if target == RelationalDbCommitlogDescriptor::NAME {
//        Ok(())
//    } else {
//        anyhow::bail!(
//            "unsupported target: {target}; expected: {}",
//            RelationalDbCommitlogDescriptor::NAME
//        )
//    }
//}
//
//
async fn run_target<T: Target>(config: RunConfig) -> anyhow::Result<()> {
    let line = T::run_streaming(config).await?;
    println!("{line}");
    Ok(())
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RunConfig {
    /// Hard cap on generated interactions. `None` means no interaction budget.
    ///
    /// This is the preferred budget for exact seed replay: the same target,
    /// scenario, seed, max-interactions value, and fault profile should produce
    /// the same generated interaction stream.
    pub max_interactions: Option<usize>,

    pub seed: u64,
}

struct Engine;

impl Target for Engine {
    const NAME: &'static str = "engine";

    fn prepare(config: &RunConfig) {
        todo!()
    }

    fn run_streaming(config: RunConfig) {
        todo!()
    }
}

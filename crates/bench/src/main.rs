use clap::{Parser, Subcommand};
use spacetimedb_bench::prelude::*;

/// Bencher for SpacetimeDB
#[derive(Debug, Parser)]
#[command(name = "bench")]
struct Cli {
    #[arg(long)]
    db: DbEngine,
    #[command(subcommand)]
    command: Commands,
}

// Note: Reflex this same benchmarks in `benches/db.rs`
#[derive(Debug, Subcommand)]
enum Commands {
    /// Generate insert, each generate a transaction
    Insert {
        /// How many rows
        #[arg(value_enum)]
        rows: Option<Runs>,
    },
    /// Generate insert in bulk enclosed in a single transaction
    InsertBulk {
        /// How many rows
        #[arg(value_enum)]
        rows: Option<Runs>,
    },
    /// Run queries without a index
    SelectNoIndex {
        /// How many rows
        #[arg(value_enum)]
        rows: Option<Runs>,
    },
}

macro_rules! bench_fn {
    ($cli:ident, $fun:ident, $run:expr, $prefill:literal) => {{
        let run = $run;

        match $cli.db {
            DbEngine::Sqlite => {
                let mut pool = Pool::new($prefill)?;
                sqlite::$fun(&mut pool, run)
            }
            DbEngine::Spacetime => {
                let mut pool = Pool::new($prefill)?;
                spacetime::$fun(&mut pool, run)
            }
        }
    }};
}

fn main() -> ResultBench<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Insert { rows } => {
            bench_fn!(cli, insert_tx_per_row, rows.unwrap_or(Runs::Tiny), false)
        }
        Commands::InsertBulk { rows } => {
            bench_fn!(cli, insert_tx, rows.unwrap_or(Runs::Small), true)
        }
        Commands::SelectNoIndex { rows } => {
            bench_fn!(cli, select_no_index, rows.unwrap_or(Runs::Tiny), true)
        }
    }
}

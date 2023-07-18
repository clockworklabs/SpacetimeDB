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
        /// Which DB to select ie: 1.db, 2.db, etc...
        db: usize,
        /// How many rows
        #[arg(value_enum)]
        rows: Option<Runs>,
    },
    /// Generate insert in bulk enclosed in a single transaction
    InsertBulk {
        /// Which DB to select ie: 1.db, 2.db, etc...
        db: usize,
        /// How many rows
        #[arg(value_enum)]
        rows: Option<Runs>,
    },
    /// Run queries without a index
    SelectNoIndex {
        /// Which DB to select ie: 1.db, 2.db, etc...
        db: usize,
        /// How many rows
        #[arg(value_enum)]
        rows: Option<Runs>,
    },
    /// Create a database and return the path (so we can skip this expensive setup in bench's)
    CreateDb {
        /// How many DBs to pre-create ie: 3 = 1.db, 2.db, 3.db
        total_dbs: usize,
    },
}

macro_rules! bench_fn {
    ($cli:ident, $db:ident, $fun:ident, $run:expr) => {{
        let run = $run;
        let db_instance = $db;

        match $cli.db {
            DbEngine::Sqlite => {
                let path = sqlite::db_path_instance(db_instance);
                let mut conn = sqlite::open_conn(&path)?;
                sqlite::$fun(&mut conn, run)
            }
            DbEngine::Spacetime => {
                let path = spacetime::db_path(db_instance);
                let (conn, _tmp_dir, table_id) = spacetime::open_conn(&path)?;
                spacetime::$fun(&conn, table_id, run)
            }
        }
    }};
}

fn main() -> ResultBench<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Insert { db, rows } => {
            bench_fn!(cli, db, insert_tx_per_row, rows.unwrap_or(Runs::Tiny))
        }
        Commands::InsertBulk { db, rows } => {
            // bench_fn!(cli, insert_tx, rows.unwrap_or(Runs::Small), true)
            Ok(())
        }
        Commands::SelectNoIndex { db, rows } => {
            // bench_fn!(cli, select_no_index, rows.unwrap_or(Runs::Tiny), true)
            Ok(())
        }
        Commands::CreateDb { total_dbs } => match cli.db {
            DbEngine::Sqlite => {
                sqlite::create_db(total_dbs)?;
                Ok(())
            }
            DbEngine::Spacetime => {
                spacetime::create_db(total_dbs)?;
                Ok(())
            }
        },
    }
}

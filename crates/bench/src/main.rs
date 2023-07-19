use clap::{Parser, Subcommand};
use rusqlite::Connection;
use spacetimedb::db::relational_db::RelationalDB;
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
    /// Create a database and return the path (so we can skip this expensive setup in bench's)
    CreateDb {
        /// How many DBs to pre-create ie: 3 = 1.db, 2.db, 3.db
        total_dbs: usize,
    },
}

fn bench_fn<Space, Sqlite>(cli: Cli, space: Space, sqlite: Sqlite, run: Runs) -> ResultBench<()>
where
    Space: Fn(&RelationalDB, u32, Runs) -> ResultBench<()>,
    Sqlite: Fn(&mut Connection, Runs) -> ResultBench<()>,
{
    match cli.db {
        DbEngine::Sqlite => {
            let db_instance = sqlite::get_counter()?;
            let path = sqlite::db_path_instance(db_instance);
            let mut conn = sqlite::open_conn(&path)?;
            sqlite(&mut conn, run)?;
            sqlite::set_counter(db_instance + 1)?;
            Ok(())
        }
        DbEngine::Spacetime => {
            let db_instance = spacetime::get_counter()?;
            let path = spacetime::db_path(db_instance);
            let (conn, _tmp_dir, table_id) = spacetime::open_conn(&path)?;
            space(&conn, table_id, run)?;
            spacetime::set_counter(db_instance + 1)?;
            Ok(())
        }
    }
}

/// The workflow for running the bench without interference of creation of the database on disk
/// that is expensive and generate noise specially in the case of spacetime that create many folder/files is:
///
/// - Run `--db ENGINE create-db $total_create` for pre-create the dbs like `0.db, 1.db...$total_create`
/// - Execute the bench with `--db spacetime NAME`
///
/// For picking which db to use, this hack is implemented:
///
/// - Save with `set_counter(0)` a file with the start of the "iteration"
/// - Load with `get_counter()` the file with the current `db id`
/// - Execute the bench
/// - Save the `db id` of the next iteration with `set_counter(db_id + 1)`
///
/// NOTE: This is workaround for `hyperfine` where is not possible to know which run is the one this command is invoked
/// see https://github.com/sharkdp/hyperfine/issues/667
fn main() -> ResultBench<()> {
    // Note: Mirror this benchmarks in `benches/db.rs`
    let cli = Cli::parse();
    match cli.command {
        Commands::Insert { rows } => bench_fn(
            cli,
            spacetime::insert_tx_per_row,
            sqlite::insert_tx_per_row,
            rows.unwrap_or(Runs::Tiny),
        ),
        Commands::InsertBulk { rows } => bench_fn(
            cli,
            spacetime::insert_tx,
            sqlite::insert_tx,
            rows.unwrap_or(Runs::Small),
        ),
        Commands::SelectNoIndex { rows } => bench_fn(
            cli,
            spacetime::select_no_index,
            sqlite::select_no_index,
            rows.unwrap_or(Runs::Tiny),
        ),
        Commands::CreateDb { total_dbs } => match cli.db {
            DbEngine::Sqlite => {
                sqlite::create_dbs(total_dbs)?;
                Ok(())
            }
            DbEngine::Spacetime => {
                spacetime::create_dbs(total_dbs)?;
                Ok(())
            }
        },
    }
}

use std::time::{SystemTime, UNIX_EPOCH};

use clap::{Args, Parser, Subcommand};
use spacetimedb_runtime::sim::Rng;

pub mod core;
pub mod source;
pub mod target;

use source::schema_gen::{SchemaGenerator, SchemaProfile};
use source::table_ops::{Interaction, InteractionGen, Model};
use target::engine::EngineTarget;

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
    let schema = SchemaGenerator::new(&rng, SchemaProfile::default()).gen_schema();

    eprintln!("generated {} tables:", schema.tables.len());
    for table in &schema.tables {
        eprintln!("  {} ({} columns)", table.name, table.columns.len());
    }

    // Open engine and create tables.
    let engine = EngineTarget::prepare(&config, schema.clone())?;
    eprintln!("engine ready");

    // Generate and execute interactions.
    let budget = config.max_interactions.unwrap_or(100);
    let source = InteractionGen::new(&rng, &schema);
    let mut model = Model::new(&schema);

    let mut inserts = 0u64;
    let mut deletes = 0u64;
    let mut counts = 0u64;

    for _ in 0..budget {
        let ix = source.next_interaction(&model);
        let expected = model.apply(&ix);
        let got = engine.execute(&ix).unwrap();
        assert_eq!(expected, got, "model mismatch");
        match &ix {
            Interaction::Insert { .. } => inserts += 1,
            Interaction::Delete { .. } => deletes += 1,
            Interaction::Count { .. } => counts += 1,
        }
    }

    eprintln!("done: {inserts} inserts, {deletes} deletes, {counts} counts, {budget} total");

    // Final verification: model row counts match engine.
    for (i, table) in schema.tables.iter().enumerate() {
        let table_id = engine.db().with_auto_commit(
            spacetimedb_datastore::execution_context::Workload::Internal,
            |tx| {
                engine
                    .db()
                    .table_id_from_name_mut(tx, &table.name)
                    .map(|t| t.unwrap())
            },
        )?;
        let actual = engine.db().with_auto_commit(
            spacetimedb_datastore::execution_context::Workload::Internal,
            |tx| engine.db().iter_mut(tx, table_id).map(|it| it.count() as u64),
        )?;
        assert_eq!(
            model.row_count(i),
            actual,
            "table '{}': model={} engine={}",
            table.name,
            model.row_count(i),
            actual,
        );
    }
    eprintln!("model consistency verified");

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

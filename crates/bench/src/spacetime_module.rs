use std::path::Path;

use spacetimedb::db::{Config, Storage};
use spacetimedb_lib::{
    sats::{product, ArrayValue},
    AlgebraicValue,
};
use spacetimedb_paths::RootDir;
use spacetimedb_primitives::ColId;
use spacetimedb_testing::modules::{start_runtime, CompilationMode, CompiledModule, LoggerRecord, ModuleHandle};
use tokio::runtime::Runtime;

use crate::{
    database::BenchDatabase,
    schemas::{table_name, BenchTable},
    ResultBench,
};
use criterion::async_executor::AsyncExecutor;

lazy_static::lazy_static! {
    pub static ref BENCHMARKS_MODULE: CompiledModule = {
        if std::env::var_os("STDB_BENCH_CS").is_some() {
            CompiledModule::compile("benchmarks-cs", CompilationMode::Release)
        } else {
            // Temporarily add CARGO_TARGET_DIR override to avoid conflicts with main target dir.
            // Otherwise for some reason Cargo will mark all dependencies with build scripts as
            // fresh - but only if running benchmarks (if modules are built in release mode).
            // See https://github.com/clockworklabs/SpacetimeDB/issues/401.
            std::env::set_var("CARGO_TARGET_DIR", concat!(env!("CARGO_MANIFEST_DIR"), "/target"));
            let module = CompiledModule::compile("benchmarks", CompilationMode::Release);
            std::env::remove_var("CARGO_TARGET_DIR");
            module
        }
    };
}

/// A benchmark backend that invokes a spacetime module.
///
/// This is tightly tied to the file `modules/benchmarks/src/lib.rs`;
/// all of the implementations of `BenchDatabase` methods just invoke reducers
/// in that module.
///
/// See the doc comment there for information on the formatting expected for
/// table and reducer names.
pub struct SpacetimeModule {
    // Module must be dropped BEFORE runtime otherwise there is a deadlock!
    // In Rust, struct fields are guaranteed to drop in declaration order, so don't reorder this field.
    pub module: ModuleHandle,
    runtime: Runtime,
}

// Note: we use block_on for the methods here. It adds about 70ns of overhead.
// This isn't currently a problem. Overhead to call an empty reducer is currently 20_000 ns.

impl AsyncExecutor for &SpacetimeModule {
    fn block_on<T>(&self, future: impl std::future::Future<Output = T>) -> T {
        self.runtime.block_on(future)
    }
}

// It's easier to do it this way because async traits are a mess.
impl BenchDatabase for SpacetimeModule {
    fn name() -> &'static str {
        "stdb_module"
    }

    type TableId = TableId;

    fn build(in_memory: bool) -> ResultBench<Self>
    where
        Self: Sized,
    {
        let runtime = start_runtime();
        let config = Config {
            storage: if in_memory { Storage::Memory } else { Storage::Disk },
        };

        let module = runtime.block_on(async {
            // We keep a saved database at "crates/bench/.spacetime".
            // This is mainly used for caching wasmtime native artifacts.
            let path = RootDir(Path::new(env!("CARGO_MANIFEST_DIR")).join(".spacetime"));
            BENCHMARKS_MODULE.load_module(config, Some(&path)).await
        });

        for table in module.client.module.info.module_def.tables() {
            log::trace!("SPACETIME_MODULE: LOADED TABLE: {:?}", table);
        }
        for reducer in module.client.module.info.module_def.reducers() {
            log::trace!("SPACETIME_MODULE: LOADED REDUCER: {:?}", reducer);
        }
        Ok(SpacetimeModule { runtime, module })
    }

    fn create_table<T: BenchTable>(
        &mut self,
        table_style: crate::schemas::IndexStrategy,
    ) -> ResultBench<Self::TableId> {
        // Noop. All tables are built into the "benchmarks" module.
        Ok(TableId {
            pascal_case: table_name::<T>(table_style),
            snake_case: table_name::<T>(table_style),
        })
    }

    fn clear_table(&mut self, table_id: &Self::TableId) -> ResultBench<()> {
        let SpacetimeModule { runtime, module } = self;
        runtime.block_on(async move {
            // FIXME: this doesn't work. delete is unimplemented!!
            /*
            let name = format!("clear_table_{}", table_id.snake_case);
            module.call_reducer_binary(&name, ProductValue::new(&[])).await?;
            */
            // workaround for now
            module.client.module.clear_table(&table_id.pascal_case)?;
            Ok(())
        })
    }

    // Implemented by calling a reducer that logs, then looking for the resulting
    // message in the log.
    // This implementation will not work if other people are concurrently interacting with our module.
    fn count_table(&mut self, table_id: &Self::TableId) -> ResultBench<u32> {
        let SpacetimeModule { runtime, module } = self;

        let count = runtime.block_on(async move {
            let name = format!("count_{}", table_id.snake_case);
            module.call_reducer_binary(&name, &[].into()).await?;
            let logs = module.read_log(Some(1)).await;
            let message = serde_json::from_str::<LoggerRecord>(&logs)?;
            if !message.message.starts_with("COUNT: ") {
                anyhow::bail!("Improper count message format: {:?}", message.message);
            }

            let count = message.message["COUNT: ".len()..].parse::<u32>()?;
            Ok(count)
        })?;
        Ok(count)
    }

    fn empty_transaction(&mut self) -> ResultBench<()> {
        let SpacetimeModule { runtime, module } = self;

        runtime.block_on(async move {
            module.call_reducer_binary("empty", &[].into()).await?;
            Ok(())
        })
    }

    fn insert_bulk<T: BenchTable>(&mut self, table_id: &Self::TableId, rows: Vec<T>) -> ResultBench<()> {
        let rows = rows.into_iter().map(|row| row.into_product_value()).collect();
        let args = product![ArrayValue::Product(rows)];
        let SpacetimeModule { runtime, module } = self;
        let reducer_name = format!("insert_bulk_{}", table_id.snake_case);

        runtime.block_on(async move {
            module.call_reducer_binary(&reducer_name, &args).await?;
            Ok(())
        })
    }

    fn update_bulk<T: BenchTable>(&mut self, table_id: &Self::TableId, row_count: u32) -> ResultBench<()> {
        let args = product![row_count];
        let SpacetimeModule { runtime, module } = self;
        let reducer_name = format!("update_bulk_{}", table_id.snake_case);

        runtime.block_on(async move {
            module.call_reducer_binary(&reducer_name, &args).await?;
            Ok(())
        })
    }

    fn iterate(&mut self, table_id: &Self::TableId) -> ResultBench<()> {
        let SpacetimeModule { runtime, module } = self;
        let reducer_name = format!("iterate_{}", table_id.snake_case);

        runtime.block_on(async move {
            module.call_reducer_binary(&reducer_name, &[].into()).await?;
            Ok(())
        })
    }

    fn filter<T: BenchTable>(
        &mut self,
        table_id: &Self::TableId,
        col_id: impl Into<ColId>,
        value: AlgebraicValue,
    ) -> ResultBench<()> {
        let SpacetimeModule { runtime, module } = self;

        let product_type = T::product_type();
        let column_name = product_type.elements[col_id.into().idx()].name.as_ref().unwrap();
        let reducer_name = format!("filter_{}_by_{}", table_id.snake_case, column_name);

        runtime.block_on(async move {
            module.call_reducer_binary(&reducer_name, &[value].into()).await?;
            Ok(())
        })
    }
}

#[derive(Debug, Clone)]
pub struct TableId {
    pascal_case: String,
    snake_case: String,
}

use spacetimedb::db::{Config, FsyncPolicy, Storage};
use spacetimedb_lib::{sats::ArrayValue, AlgebraicValue, ProductValue};
use spacetimedb_testing::modules::{start_runtime, CompiledModule, ModuleHandle};
use tokio::runtime::Runtime;

use crate::{
    database::BenchDatabase,
    schemas::{snake_case_table_name, table_name, BenchTable},
    ResultBench,
};

lazy_static::lazy_static! {
    pub static ref BENCHMARKS_MODULE: CompiledModule =
        CompiledModule::compile("benchmarks");
}

pub struct SpacetimeModule {
    runtime: Runtime,
    // it's necessary for this to be dropped BEFORE the runtime.
    // this is always Some when drop isn't running.
    module: Option<ModuleHandle>,
}

impl Drop for SpacetimeModule {
    fn drop(&mut self) {
        // enforce module being dropped
        drop(self.module.take());
    }
}

// Note: we use block_on for the methods here. It adds about 70ns of overhead.
// This isn't currently a problem. Overhead to call an empty reducer is currently 20_000 ns.
// It's easier to do it this way because async trades are a mess.
impl BenchDatabase for SpacetimeModule {
    fn name() -> &'static str {
        "stdb_module"
    }

    type TableId = TableId;

    fn build(in_memory: bool, fsync: bool) -> ResultBench<Self>
    where
        Self: Sized,
    {
        let runtime = start_runtime();
        let config = Config {
            fsync: if fsync {
                FsyncPolicy::EveryTx
            } else {
                FsyncPolicy::Never
            },
            storage: if in_memory { Storage::Memory } else { Storage::Disk },
        };
        let module = runtime.block_on(async { BENCHMARKS_MODULE.load_module(config).await });

        for thing in module.client.module.catalog().iter() {
            log::trace!("SPACETIME_MODULE: LOADED: {} {:?}", thing.0, thing.1.ty());
        }
        Ok(SpacetimeModule {
            runtime,
            module: Some(module),
        })
    }

    fn create_table<T: BenchTable>(&mut self, table_style: crate::schemas::TableStyle) -> ResultBench<Self::TableId> {
        // Noop. All tables are built into the "benchmarks" module.
        Ok(TableId {
            pascal_case: table_name::<T>(table_style),
            snake_case: snake_case_table_name::<T>(table_style),
        })
    }

    fn clear_table(&mut self, table_id: &Self::TableId) -> ResultBench<()> {
        let SpacetimeModule { runtime, module } = self;
        let module = module.as_mut().unwrap();
        runtime.block_on(async move {
            // FIXME: this doesn't work. delete is unimplemented!!
            /*
            let name = format!("clear_table_{}", table_id.snake_case);
            module.call_reducer_binary(&name, ProductValue::new(&[])).await?;
            */
            // workaround for now
            module.client.module.clear_table(table_id.pascal_case.clone()).await?;
            Ok(())
        })
    }

    // This implementation will not work if other people are interacting with our module.
    fn count_table(&mut self, table_id: &Self::TableId) -> ResultBench<u32> {
        let SpacetimeModule { runtime, module } = self;
        let module = module.as_mut().unwrap();

        let count = runtime.block_on(async move {
            let name = format!("count_{}", table_id.snake_case);
            module.call_reducer_binary(&name, ProductValue::new(&[])).await?;
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
        let module = module.as_mut().unwrap();

        runtime.block_on(async move {
            module.call_reducer_binary("empty", ProductValue::new(&[])).await?;
            Ok(())
        })
    }

    type PreparedInsert<T> = PreparedQuery;
    #[inline(never)]
    fn prepare_insert<T: BenchTable>(&mut self, table_id: &Self::TableId) -> ResultBench<Self::PreparedInsert<T>> {
        Ok(PreparedQuery {
            reducer_name: format!("insert_{}", table_id.snake_case),
        })
    }

    fn insert<T: BenchTable>(&mut self, prepared: &Self::PreparedInsert<T>, row: T) -> ResultBench<()> {
        let SpacetimeModule { runtime, module } = self;
        let module = module.as_mut().unwrap();

        runtime.block_on(async move {
            module
                .call_reducer_binary(&prepared.reducer_name, row.into_product_value())
                .await?;
            Ok(())
        })
    }

    type PreparedInsertBulk<T> = PreparedQuery;
    #[inline(never)]
    fn prepare_insert_bulk<T: BenchTable>(
        &mut self,
        table_id: &Self::TableId,
    ) -> ResultBench<Self::PreparedInsertBulk<T>> {
        Ok(PreparedQuery {
            reducer_name: format!("insert_bulk_{}", table_id.snake_case),
        })
    }

    fn insert_bulk<T: BenchTable>(&mut self, prepared: &Self::PreparedInsertBulk<T>, rows: Vec<T>) -> ResultBench<()> {
        // unfortunately, the marshalling time here is included in the benchmark.
        // At least it doesn't need to reallocate the strings.
        let args = ProductValue {
            elements: vec![AlgebraicValue::Builtin(spacetimedb_lib::sats::BuiltinValue::Array {
                val: ArrayValue::Product(rows.into_iter().map(|row| row.into_product_value()).collect()),
            })],
        };
        let SpacetimeModule { runtime, module } = self;
        let module = module.as_mut().unwrap();

        runtime.block_on(async move {
            module.call_reducer_binary(&prepared.reducer_name, args).await?;
            Ok(())
        })
    }

    type PreparedInterate = PreparedQuery;
    #[inline(never)]
    fn prepare_iterate<T: BenchTable>(&mut self, table_id: &Self::TableId) -> ResultBench<Self::PreparedInterate> {
        Ok(PreparedQuery {
            reducer_name: format!("iterate_{}", table_id.snake_case),
        })
    }
    #[inline(never)]
    fn iterate(&mut self, prepared: &Self::PreparedInterate) -> ResultBench<()> {
        let SpacetimeModule { runtime, module } = self;
        let module = module.as_mut().unwrap();

        runtime.block_on(async move {
            module
                .call_reducer_binary(&prepared.reducer_name, ProductValue::new(&[]))
                .await?;
            Ok(())
        })
    }

    type PreparedFilter = PreparedQuery;
    #[inline(never)]
    fn prepare_filter<T: BenchTable>(
        &mut self,
        table_id: &Self::TableId,
        column_id: u32,
    ) -> ResultBench<Self::PreparedFilter> {
        let product_type = T::product_type();
        let column_name = product_type.elements[column_id as usize].name.as_ref().unwrap();
        Ok(PreparedQuery {
            reducer_name: format!("filter_{}_by_{}", table_id.snake_case, column_name),
        })
    }
    #[inline(never)]
    fn filter(&mut self, prepared: &Self::PreparedFilter, value: AlgebraicValue) -> ResultBench<()> {
        let SpacetimeModule { runtime, module } = self;
        let module = module.as_mut().unwrap();

        runtime.block_on(async move {
            module
                .call_reducer_binary(&prepared.reducer_name, ProductValue { elements: vec![value] })
                .await?;
            Ok(())
        })
    }
}

#[derive(Clone)]
pub struct TableId {
    pascal_case: String,
    snake_case: String,
}

#[derive(Clone)]
pub struct PreparedQuery {
    reducer_name: String,
}

#[allow(unused)]
/// Sync with: `core::database_logger::Record`.
#[derive(serde::Deserialize)]
struct LoggerRecord {
    target: Option<String>,
    filename: Option<String>,
    line_number: Option<u32>,
    message: String,
}

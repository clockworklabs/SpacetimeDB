use crate::error::DBError;
use crate::subscription::websocket_building::{BuildableWebsocketFormat, RowListBuilder as _, RowListBuilderSource};
use anyhow::Result;
use metrics::QueryMetrics;
use module_subscription_manager::Plan;
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use spacetimedb_client_api_messages::websocket::common::ByteListLen as _;
use spacetimedb_client_api_messages::websocket::v1::{self as ws_v1};
pub use spacetimedb_engine::metrics::ExecutionCounters;
use spacetimedb_execution::{pipelined::PipelinedProject, Datastore, DeltaStore, Row};
use spacetimedb_lib::metrics::ExecutionMetrics;
use spacetimedb_physical_plan::plan::ParamResolver;
use spacetimedb_primitives::{ColList, TableId};
use spacetimedb_sats::bsatn::ToBsatn;
use spacetimedb_sats::Serialize;
use spacetimedb_schema::table_name::TableName;
use std::sync::Arc;

pub mod delta;
pub mod execution_unit;
pub mod metrics;
pub mod module_subscription_actor;
pub mod module_subscription_manager;
pub mod query;
pub mod row_list_builder_pool;
#[allow(clippy::module_inception)] // it's right this isn't ideal :/
pub mod subscription;
pub mod tx;
pub mod websocket_building;

/// Execute subscription query fragments over a view.
pub fn execute_plan_for_view<'p, F>(
    plan_fragments: impl IntoIterator<Item = &'p PipelinedProject>,
    num_cols: usize,
    num_private_cols: usize,
    tx: &(impl Datastore + DeltaStore),
    params: &impl ParamResolver,
    rlb_pool: &impl RowListBuilderSource<F>,
) -> Result<(F::List, u64, ExecutionMetrics)>
where
    F: BuildableWebsocketFormat,
{
    build_list_with_executor(rlb_pool, |metrics, add| {
        let col_list = ColList::from_iter(num_private_cols..num_cols);
        for fragment in plan_fragments {
            fragment.execute(tx, params, metrics, &mut |row| match row {
                Row::Ptr(ptr) => add(ptr.project_product(&col_list)?),
                Row::Ref(val) => add(val.project_product(&col_list)?),
            })?;
        }
        Ok(())
    })
}

/// Execute a subscription query
pub fn execute_plan<'p, F>(
    plan_fragments: impl IntoIterator<Item = &'p PipelinedProject>,
    tx: &(impl Datastore + DeltaStore),
    params: &impl ParamResolver,
    rlb_pool: &impl RowListBuilderSource<F>,
) -> Result<(F::List, u64, ExecutionMetrics)>
where
    F: BuildableWebsocketFormat,
{
    build_list_with_executor(rlb_pool, |metrics, add| {
        for fragment in plan_fragments {
            fragment.execute(tx, params, metrics, add)?;
        }
        Ok(())
    })
}

/// Returns a list built by passing a function `add` to `driver`,
/// which will call the former for every row it processes.
pub fn build_list_with_executor<F: BuildableWebsocketFormat, R: ToBsatn + Serialize>(
    rlb_pool: &impl RowListBuilderSource<F>,
    driver: impl FnOnce(&mut ExecutionMetrics, &mut dyn FnMut(R) -> Result<()>) -> Result<()>,
) -> Result<(F::List, u64, ExecutionMetrics)> {
    let mut count = 0;
    let mut list = rlb_pool.take_row_list_builder();
    let mut metrics = ExecutionMetrics::default();

    driver(&mut metrics, &mut |row| {
        count += 1;
        list.push(row);
        Ok(())
    })?;

    let list = list.finish();
    metrics.bytes_scanned += list.num_bytes();
    metrics.bytes_sent_to_clients += list.num_bytes();
    Ok((list, count, metrics))
}

/// When collecting a table update are we inserting or deleting rows?
/// For unsubscribe operations, we need to delete rows.
#[derive(Debug, Clone, Copy)]
pub enum TableUpdateType {
    Subscribe,
    Unsubscribe,
}

fn table_update_from_rows<F: BuildableWebsocketFormat>(
    rows: F::List,
    num_rows: u64,
    metrics: ExecutionMetrics,
    table_id: TableId,
    table_name: TableName,
    update_type: TableUpdateType,
) -> (ws_v1::TableUpdate<F>, ExecutionMetrics) {
    let empty = F::List::default();
    let qu = match update_type {
        TableUpdateType::Subscribe => ws_v1::QueryUpdate {
            deletes: empty,
            inserts: rows,
        },
        TableUpdateType::Unsubscribe => ws_v1::QueryUpdate {
            deletes: rows,
            inserts: empty,
        },
    };
    // We will compress the outer server message,
    // after we release the tx lock.
    // There's no need to compress the inner table update too.
    let update = F::into_query_update(qu, ws_v1::Compression::None);
    (
        ws_v1::TableUpdate::new(
            table_id,
            table_name.into(),
            ws_v1::SingleQueryUpdate { update, num_rows },
        ),
        metrics,
    )
}

/// Execute subscription query fragments over a view and collect the results in a [TableUpdate].
#[allow(clippy::too_many_arguments)]
pub fn collect_table_update_for_view<'p, Tx, F>(
    plan_fragments: impl IntoIterator<Item = &'p PipelinedProject>,
    num_cols: usize,
    num_private_cols: usize,
    table_id: TableId,
    table_name: TableName,
    tx: &Tx,
    params: &impl ParamResolver,
    update_type: TableUpdateType,
    rlb_pool: &impl RowListBuilderSource<F>,
) -> Result<(ws_v1::TableUpdate<F>, ExecutionMetrics)>
where
    Tx: Datastore + DeltaStore,
    F: BuildableWebsocketFormat,
{
    execute_plan_for_view::<F>(plan_fragments, num_cols, num_private_cols, tx, params, rlb_pool).map(
        |(rows, num_rows, metrics)| table_update_from_rows(rows, num_rows, metrics, table_id, table_name, update_type),
    )
}

/// Execute a subscription query and collect the results in a [TableUpdate]
pub fn collect_table_update<'p, F>(
    plan_fragments: impl IntoIterator<Item = &'p PipelinedProject>,
    table_id: TableId,
    table_name: TableName,
    tx: &(impl Datastore + DeltaStore),
    params: &impl ParamResolver,
    update_type: TableUpdateType,
    rlb_pool: &impl RowListBuilderSource<F>,
) -> Result<(ws_v1::TableUpdate<F>, ExecutionMetrics)>
where
    F: BuildableWebsocketFormat,
{
    execute_plan::<F>(plan_fragments, tx, params, rlb_pool).map(|(rows, num_rows, metrics)| {
        table_update_from_rows(rows, num_rows, metrics, table_id, table_name, update_type)
    })
}

/// Execute a collection of subscription queries in parallel
pub fn execute_plans<F: BuildableWebsocketFormat>(
    plans: &[Arc<Plan>],
    tx: &(impl Datastore + DeltaStore + Sync),
    update_type: TableUpdateType,
    rlb_pool: &(impl Sync + RowListBuilderSource<F>),
) -> Result<(ws_v1::DatabaseUpdate<F>, ExecutionMetrics, Vec<QueryMetrics>), DBError> {
    plans
        .par_iter()
        .flat_map_iter(|plan| plan.plans_fragments().map(|fragment| (plan.sql(), fragment)))
        .filter(|(_, plan)| {
            // Since subscriptions only support selects and inner joins,
            // we filter out any plans that read from an empty table.
            plan.table_ids().all(|table_id| tx.row_count(table_id) > 0)
        })
        .map(|(sql, plan)| (sql, plan, plan.subscribed_table_id(), plan.subscribed_table_name()))
        .map(|(sql, plan, table_id, table_name)| {
            {
                let start_time = std::time::Instant::now();

                let result = if plan.is_view() {
                    collect_table_update_for_view(
                        std::iter::once(plan.base_plan()),
                        plan.num_cols(),
                        plan.num_private_cols(),
                        table_id,
                        table_name.clone(),
                        tx,
                        plan.params(),
                        update_type,
                        rlb_pool,
                    )?
                } else {
                    collect_table_update(
                        std::iter::once(plan.base_plan()),
                        table_id,
                        table_name.clone(),
                        tx,
                        plan.params(),
                        update_type,
                        rlb_pool,
                    )?
                };

                let elapsed = start_time.elapsed();

                let (ref _table_update, ref metrics) = result;
                let query_metrics = metrics::get_query_metrics(
                    table_name.clone(),
                    plan.scan_metrics(),
                    metrics.rows_scanned as u64,
                    elapsed.as_micros() as u64,
                );

                Ok((result.0, result.1, Some(query_metrics)))
            }
            .map_err(|err| DBError::WithSql {
                sql: sql.into(),
                error: Box::new(DBError::Other(err)),
            })
        })
        .collect::<Result<Vec<_>, _>>()
        .map(|table_updates_with_metrics| {
            let n = table_updates_with_metrics.len();
            let mut tables = Vec::with_capacity(n);
            let mut aggregated_metrics = ExecutionMetrics::default();
            let mut query_metrics_vec = Vec::new();

            for (update, metrics, query_metrics) in table_updates_with_metrics {
                tables.push(update);
                aggregated_metrics.merge(metrics);
                if let Some(qm) = query_metrics {
                    query_metrics_vec.push(qm);
                }
            }
            (ws_v1::DatabaseUpdate { tables }, aggregated_metrics, query_metrics_vec)
        })
}

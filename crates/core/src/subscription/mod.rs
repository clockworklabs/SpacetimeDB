use crate::subscription::websocket_building::{BuildableWebsocketFormat, RowListBuilder as _};
use crate::{error::DBError, worker_metrics::WORKER_METRICS};
use anyhow::Result;
use metrics::QueryMetrics;
use module_subscription_manager::Plan;
use prometheus::IntCounter;
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use spacetimedb_client_api_messages::websocket::{
    ByteListLen, Compression, DatabaseUpdate, QueryUpdate, SingleQueryUpdate, TableUpdate,
};
use spacetimedb_datastore::{
    db_metrics::DB_METRICS, execution_context::WorkloadType, locking_tx_datastore::datastore::MetricsRecorder,
};
use spacetimedb_execution::pipelined::ViewProject;
use spacetimedb_execution::{pipelined::PipelinedProject, Datastore, DeltaStore};
use spacetimedb_lib::identity::AuthCtx;
use spacetimedb_lib::{metrics::ExecutionMetrics, Identity};
use spacetimedb_primitives::TableId;
use std::sync::Arc;

pub mod delta;
pub mod execution_unit;
pub mod metrics;
pub mod module_subscription_actor;
pub mod module_subscription_manager;
pub mod query;
#[allow(clippy::module_inception)] // it's right this isn't ideal :/
pub mod subscription;
pub mod tx;
pub mod websocket_building;

#[derive(Debug)]
pub struct ExecutionCounters {
    rdb_num_index_seeks: IntCounter,
    rdb_num_rows_scanned: IntCounter,
    rdb_num_bytes_scanned: IntCounter,
    rdb_num_bytes_written: IntCounter,
    bytes_sent_to_clients: IntCounter,
    delta_queries_matched: IntCounter,
    delta_queries_evaluated: IntCounter,
    duplicate_rows_evaluated: IntCounter,
    duplicate_rows_sent: IntCounter,
}

impl ExecutionCounters {
    pub fn new(workload: &WorkloadType, db: &Identity) -> Self {
        Self {
            rdb_num_index_seeks: DB_METRICS.rdb_num_index_seeks.with_label_values(workload, db),
            rdb_num_rows_scanned: DB_METRICS.rdb_num_rows_scanned.with_label_values(workload, db),
            rdb_num_bytes_scanned: DB_METRICS.rdb_num_bytes_scanned.with_label_values(workload, db),
            rdb_num_bytes_written: DB_METRICS.rdb_num_bytes_written.with_label_values(workload, db),
            bytes_sent_to_clients: WORKER_METRICS.bytes_sent_to_clients.with_label_values(workload, db),
            delta_queries_matched: DB_METRICS.delta_queries_matched.with_label_values(db),
            delta_queries_evaluated: DB_METRICS.delta_queries_evaluated.with_label_values(db),
            duplicate_rows_evaluated: DB_METRICS.duplicate_rows_evaluated.with_label_values(db),
            duplicate_rows_sent: DB_METRICS.duplicate_rows_sent.with_label_values(db),
        }
    }

    /// Update the global system metrics with transaction-level execution metrics.
    pub(crate) fn record(&self, metrics: &ExecutionMetrics) {
        if metrics.index_seeks > 0 {
            self.rdb_num_index_seeks.inc_by(metrics.index_seeks as u64);
        }
        if metrics.rows_scanned > 0 {
            self.rdb_num_rows_scanned.inc_by(metrics.rows_scanned as u64);
        }
        if metrics.bytes_scanned > 0 {
            self.rdb_num_bytes_scanned.inc_by(metrics.bytes_scanned as u64);
        }
        if metrics.bytes_written > 0 {
            self.rdb_num_bytes_written.inc_by(metrics.bytes_written as u64);
        }
        if metrics.bytes_sent_to_clients > 0 {
            self.bytes_sent_to_clients.inc_by(metrics.bytes_sent_to_clients as u64);
        }
        if metrics.delta_queries_matched > 0 {
            self.delta_queries_matched.inc_by(metrics.delta_queries_matched);
        }
        if metrics.delta_queries_evaluated > 0 {
            self.delta_queries_evaluated.inc_by(metrics.delta_queries_evaluated);
        }
        if metrics.duplicate_rows_evaluated > 0 {
            self.duplicate_rows_evaluated.inc_by(metrics.duplicate_rows_evaluated);
        }
        if metrics.duplicate_rows_sent > 0 {
            self.duplicate_rows_sent.inc_by(metrics.duplicate_rows_sent);
        }
    }
}

impl MetricsRecorder for ExecutionCounters {
    fn record(&self, metrics: &ExecutionMetrics) {
        self.record(metrics);
    }
}

/// Execute a subscription query over a view.
///
/// Specifically this utility is for queries that return rows from a view.
/// Unlike user tables, views have internal columns that should not be returned to clients.
/// The [`ViewProject`] operator implicitly drops these columns as part of its execution.
///
/// NOTE: This method was largely copied from [`execute_plan`].
/// TODO: Merge with [`execute_plan`].
pub fn execute_plan_for_view<Tx, F>(plan_fragments: &[ViewProject], tx: &Tx) -> Result<(F::List, u64, ExecutionMetrics)>
where
    Tx: Datastore + DeltaStore,
    F: BuildableWebsocketFormat,
{
    let mut count = 0;
    let mut list = F::ListBuilder::default();
    let mut metrics = ExecutionMetrics::default();

    for fragment in plan_fragments {
        fragment.execute(tx, &mut metrics, &mut |row| {
            count += 1;
            list.push(row);
            Ok(())
        })?;
    }

    let list = list.finish();
    metrics.bytes_scanned += list.num_bytes();
    metrics.bytes_sent_to_clients += list.num_bytes();
    Ok((list, count, metrics))
}

/// Execute a subscription query
pub fn execute_plan<Tx, F>(plan_fragments: &[PipelinedProject], tx: &Tx) -> Result<(F::List, u64, ExecutionMetrics)>
where
    Tx: Datastore + DeltaStore,
    F: BuildableWebsocketFormat,
{
    let mut count = 0;
    let mut list = F::ListBuilder::default();
    let mut metrics = ExecutionMetrics::default();

    for fragment in plan_fragments {
        fragment.execute(tx, &mut metrics, &mut |row| {
            count += 1;
            list.push(row);
            Ok(())
        })?;
    }

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

/// Execute a subscription query over a view and collect the results in a [TableUpdate].
///
/// Specifically this utility is for queries that return rows from a view.
/// Unlike user tables, views have internal columns that should not be returned to clients.
/// The [`ViewProject`] operator implicitly drops these columns as part of its execution.
///
/// NOTE: This method was largely copied from [`collect_table_update`].
/// TODO: Merge with [`collect_table_update`].
pub fn collect_table_update_for_view<Tx, F>(
    plan_fragments: &[ViewProject],
    table_id: TableId,
    table_name: Box<str>,
    tx: &Tx,
    update_type: TableUpdateType,
) -> Result<(TableUpdate<F>, ExecutionMetrics)>
where
    Tx: Datastore + DeltaStore,
    F: BuildableWebsocketFormat,
{
    execute_plan_for_view::<Tx, F>(plan_fragments, tx).map(|(rows, num_rows, metrics)| {
        let empty = F::List::default();
        let qu = match update_type {
            TableUpdateType::Subscribe => QueryUpdate {
                deletes: empty,
                inserts: rows,
            },
            TableUpdateType::Unsubscribe => QueryUpdate {
                deletes: rows,
                inserts: empty,
            },
        };
        // We will compress the outer server message,
        // after we release the tx lock.
        // There's no need to compress the inner table update too.
        let update = F::into_query_update(qu, Compression::None);
        (
            TableUpdate::new(table_id, table_name, SingleQueryUpdate { update, num_rows }),
            metrics,
        )
    })
}

/// Execute a subscription query and collect the results in a [TableUpdate]
pub fn collect_table_update<Tx, F>(
    plan_fragments: &[PipelinedProject],
    table_id: TableId,
    table_name: Box<str>,
    tx: &Tx,
    update_type: TableUpdateType,
) -> Result<(TableUpdate<F>, ExecutionMetrics)>
where
    Tx: Datastore + DeltaStore,
    F: BuildableWebsocketFormat,
{
    execute_plan::<Tx, F>(plan_fragments, tx).map(|(rows, num_rows, metrics)| {
        let empty = F::List::default();
        let qu = match update_type {
            TableUpdateType::Subscribe => QueryUpdate {
                deletes: empty,
                inserts: rows,
            },
            TableUpdateType::Unsubscribe => QueryUpdate {
                deletes: rows,
                inserts: empty,
            },
        };
        // We will compress the outer server message,
        // after we release the tx lock.
        // There's no need to compress the inner table update too.
        let update = F::into_query_update(qu, Compression::None);
        (
            TableUpdate::new(table_id, table_name, SingleQueryUpdate { update, num_rows }),
            metrics,
        )
    })
}

/// Execute a collection of subscription queries in parallel
pub fn execute_plans<Tx, F>(
    auth: &AuthCtx,
    plans: &[Arc<Plan>],
    tx: &Tx,
    update_type: TableUpdateType,
) -> Result<(DatabaseUpdate<F>, ExecutionMetrics, Vec<QueryMetrics>), DBError>
where
    Tx: Datastore + DeltaStore + Sync,
    F: BuildableWebsocketFormat,
{
    plans
        .par_iter()
        .flat_map_iter(|plan| plan.plans_fragments().map(|fragment| (plan.sql(), fragment)))
        .filter(|(_, plan)| {
            // Since subscriptions only support selects and inner joins,
            // we filter out any plans that read from an empty table.
            plan.table_ids().all(|table_id| tx.row_count(table_id) > 0)
        })
        .map(|(sql, plan)| (sql, plan, plan.subscribed_table_id(), plan.subscribed_table_name()))
        .map(|(sql, plan, table_id, table_name)| (sql, plan.optimized_physical_plan().clone(), table_id, table_name))
        .map(|(sql, plan, table_id, table_name)| (sql, plan.optimize(auth), table_id, table_name))
        .map(|(sql, plan, table_id, table_name)| {
            plan.and_then(|plan| {
                let start_time = std::time::Instant::now();

                let result = if plan.returns_view_table() {
                    match plan.return_table() {
                        Some(schema) => {
                            let pipelined_plan = PipelinedProject::from(plan.clone());
                            let view_plan =
                                ViewProject::new(pipelined_plan, schema.num_cols(), schema.num_private_cols());
                            collect_table_update_for_view(
                                &[view_plan],
                                table_id,
                                (&**table_name).into(),
                                tx,
                                update_type,
                            )?
                        }
                        _ => {
                            let pipelined_plan = PipelinedProject::from(plan.clone());
                            collect_table_update(&[pipelined_plan], table_id, (&**table_name).into(), tx, update_type)?
                        }
                    }
                } else {
                    let pipelined_plan = PipelinedProject::from(plan.clone());
                    collect_table_update(&[pipelined_plan], table_id, (&**table_name).into(), tx, update_type)?
                };

                let elapsed = start_time.elapsed();

                let (ref _table_update, ref metrics) = result;
                let query_metrics = metrics::get_query_metrics(
                    table_name,
                    &plan,
                    metrics.rows_scanned as u64,
                    elapsed.as_micros() as u64,
                );

                Ok((result.0, result.1, Some(query_metrics)))
            })
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
            (DatabaseUpdate { tables }, aggregated_metrics, query_metrics_vec)
        })
}

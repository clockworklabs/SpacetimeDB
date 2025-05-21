use std::sync::Arc;

use anyhow::Result;
use module_subscription_manager::Plan;
use prometheus::IntCounter;
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use spacetimedb_client_api_messages::websocket::{
    ByteListLen, Compression, DatabaseUpdate, QueryUpdate, TableUpdate, WebsocketFormat,
};
use spacetimedb_execution::{pipelined::PipelinedProject, Datastore, DeltaStore};
use spacetimedb_lib::{metrics::ExecutionMetrics, Identity};
use spacetimedb_primitives::TableId;

use crate::{
    db::db_metrics::DB_METRICS, error::DBError, execution_context::WorkloadType, worker_metrics::WORKER_METRICS,
};

pub mod delta;
pub mod execution_unit;
pub mod module_subscription_actor;
pub mod module_subscription_manager;
pub mod query;
#[allow(clippy::module_inception)] // it's right this isn't ideal :/
pub mod subscription;
pub mod tx;

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

/// Execute a subscription query
pub fn execute_plan<Tx, F>(plan_fragments: &[PipelinedProject], tx: &Tx) -> Result<(F::List, u64, ExecutionMetrics)>
where
    Tx: Datastore + DeltaStore,
    F: WebsocketFormat,
{
    let mut rows = vec![];
    let mut metrics = ExecutionMetrics::default();

    for fragment in plan_fragments {
        fragment.execute(tx, &mut metrics, &mut |row| {
            rows.push(row);
            Ok(())
        })?;
    }

    let (list, n) = F::encode_list(rows.into_iter());
    metrics.bytes_scanned += list.num_bytes();
    metrics.bytes_sent_to_clients += list.num_bytes();
    Ok((list, n, metrics))
}

/// When collecting a table update are we inserting or deleting rows?
/// For unsubscribe operations, we need to delete rows.
#[derive(Debug, Clone, Copy)]
pub enum TableUpdateType {
    Subscribe,
    Unsubscribe,
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
    F: WebsocketFormat,
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
        (TableUpdate::new(table_id, table_name, (update, num_rows)), metrics)
    })
}

/// Execute a collection of subscription queries in parallel
pub fn execute_plans<Tx, F>(
    plans: &[Arc<Plan>],
    tx: &Tx,
    update_type: TableUpdateType,
) -> Result<(DatabaseUpdate<F>, ExecutionMetrics), DBError>
where
    Tx: Datastore + DeltaStore + Sync,
    F: WebsocketFormat,
{
    plans
        .par_iter()
        .flat_map_iter(|plan| plan.plans_fragments().map(|fragment| (plan.sql(), fragment)))
        .filter(|(_, plan)| {
            // Since subscriptions only support selects and inner joins,
            // we filter out any plans that read from an empty table.
            plan.table_ids()
                .all(|table_id| tx.table(table_id).is_some_and(|t| t.row_count > 0))
        })
        .map(|(sql, plan)| (sql, plan, plan.subscribed_table_id(), plan.subscribed_table_name()))
        .map(|(sql, plan, table_id, table_name)| {
            plan.physical_plan()
                .clone()
                .optimize()
                .map(|plan| (sql, PipelinedProject::from(plan)))
                .and_then(|(_, plan)| collect_table_update(&[plan], table_id, table_name.into(), tx, update_type))
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
            for (update, metrics) in table_updates_with_metrics {
                tables.push(update);
                aggregated_metrics.merge(metrics);
            }
            (DatabaseUpdate { tables }, aggregated_metrics)
        })
}

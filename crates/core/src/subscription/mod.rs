use std::sync::Arc;

use anyhow::Result;
use module_subscription_manager::Plan;
use spacetimedb_client_api_messages::websocket::{
    ByteListLen, Compression, DatabaseUpdate, QueryUpdate, TableUpdate, WebsocketFormat,
};
use spacetimedb_execution::{pipelined::PipelinedProject, Datastore, DeltaStore};
use spacetimedb_lib::{metrics::ExecutionMetrics, Identity};
use spacetimedb_primitives::TableId;

use crate::{db::db_metrics::DB_METRICS, execution_context::WorkloadType, worker_metrics::WORKER_METRICS};

pub mod delta;
pub mod execution_unit;
pub mod module_subscription_actor;
pub mod module_subscription_manager;
pub mod query;
#[allow(clippy::module_inception)] // it's right this isn't ideal :/
pub mod subscription;
pub mod tx;

/// Update the global system metrics with transaction-level execution metrics
pub(crate) fn record_exec_metrics(workload: &WorkloadType, db: &Identity, metrics: ExecutionMetrics) {
    DB_METRICS
        .rdb_num_index_seeks
        .with_label_values(workload, db)
        .inc_by(metrics.index_seeks as u64);
    DB_METRICS
        .rdb_num_rows_scanned
        .with_label_values(workload, db)
        .inc_by(metrics.rows_scanned as u64);
    DB_METRICS
        .rdb_num_bytes_scanned
        .with_label_values(workload, db)
        .inc_by(metrics.bytes_scanned as u64);
    DB_METRICS
        .rdb_num_bytes_written
        .with_label_values(workload, db)
        .inc_by(metrics.bytes_written as u64);
    WORKER_METRICS
        .bytes_sent_to_clients
        .with_label_values(workload, db)
        .inc_by(metrics.bytes_sent_to_clients as u64);
}

/// Execute a subscription query
pub fn execute_plan<Tx, F>(plan: &PipelinedProject, tx: &Tx) -> Result<(F::List, u64, ExecutionMetrics)>
where
    Tx: Datastore + DeltaStore,
    F: WebsocketFormat,
{
    let mut rows = vec![];
    let mut metrics = ExecutionMetrics::default();
    plan.execute(tx, &mut metrics, &mut |row| {
        rows.push(row);
        Ok(())
    })?;
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
    plan: &PipelinedProject,
    table_id: TableId,
    table_name: Box<str>,
    comp: Compression,
    tx: &Tx,
    update_type: TableUpdateType,
) -> Result<(TableUpdate<F>, ExecutionMetrics)>
where
    Tx: Datastore + DeltaStore,
    F: WebsocketFormat,
{
    execute_plan::<Tx, F>(plan, tx).map(|(rows, num_rows, metrics)| {
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
        let update = F::into_query_update(qu, comp);
        (TableUpdate::new(table_id, table_name, (update, num_rows)), metrics)
    })
}

/// Execute a collection of subscription queries in parallel
pub fn execute_plans<Tx, F>(
    plans: &[Arc<Plan>],
    comp: Compression,
    tx: &Tx,
    update_type: TableUpdateType,
) -> Result<(DatabaseUpdate<F>, ExecutionMetrics)>
where
    Tx: Datastore + DeltaStore + Sync,
    F: WebsocketFormat,
{
    // FOR TESTING: Just evaluate sequentially.
    plans
        .iter()
        .map(|plan| (plan, plan.subscribed_table_id(), plan.subscribed_table_name()))
        .map(|(plan, table_id, table_name)| {
            plan.physical_plan()
                .clone()
                .optimize()
                .map(PipelinedProject::from)
                .and_then(|plan| collect_table_update(&plan, table_id, table_name.into(), comp, tx, update_type))
        })
        .collect::<Result<Vec<_>>>()
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

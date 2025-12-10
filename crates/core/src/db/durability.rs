use crate::db::persistence::Durability;
use futures::{channel::mpsc, StreamExt};
use spacetimedb_commitlog::payload::{
    txdata::{Mutations, Ops},
    Txdata,
};
use spacetimedb_data_structures::map::IntSet;
use spacetimedb_datastore::{execution_context::ReducerContext, traits::TxData};
use spacetimedb_durability::DurableOffset;
use spacetimedb_primitives::TableId;
use std::sync::Arc;

/// A request to persist a transaction.
pub struct DurabilityRequest {
    reducer_context: Option<ReducerContext>,
    tx_data: Arc<TxData>,
}

/// Represents a handle to a background task that persists transactions
/// according to the [`Durability`] policy provided.
///
/// This exists to avoid doing some preparatory work
/// before sending over to the `Durability` layer.
#[derive(Clone)]
pub struct DurabilityWorker {
    request_tx: mpsc::UnboundedSender<DurabilityRequest>,
    durability: Arc<Durability>,
}

impl DurabilityWorker {
    /// Create a new [`DurabilityWorker`] using the given `durability` policy.
    pub fn new(durability: Arc<Durability>) -> Self {
        let (request_tx, request_rx) = mpsc::unbounded();

        let actor = DurabilityWorkerActor {
            request_rx,
            durability: durability.clone(),
        };
        tokio::spawn(actor.run());

        Self { request_tx, durability }
    }

    /// Request that a transaction be made be made durable.
    /// That is, if `(tx_data, ctx)` should be appended to the commitlog, do so.
    ///
    /// Note that by this stage,
    /// [`spacetimedb_datastore::locking_tx_datastore::committed_state::tx_consumes_offset`]
    /// has already decided based on the reducer and operations whether the transaction should be appended;
    /// this method is responsible only for reading its decision out of the `tx_data`
    /// and calling `durability.append_tx`.
    ///
    /// This method does not block,
    /// and sends the work to an actor that collects data and calls `durability.append_tx`.
    ///
    /// Panics if the durability worker has closed the receive end of its queue(s),
    /// which is likely due to it having panicked.
    pub fn request_durability(&self, reducer_context: Option<ReducerContext>, tx_data: &Arc<TxData>) {
        self.request_tx
            .unbounded_send(DurabilityRequest {
                reducer_context,
                tx_data: tx_data.clone(),
            })
            .expect("durability worker panicked");
    }

    /// Get the [`DurableOffset`] of this database.
    pub fn durable_tx_offset(&self) -> DurableOffset {
        self.durability.durable_tx_offset()
    }
}

pub struct DurabilityWorkerActor {
    request_rx: mpsc::UnboundedReceiver<DurabilityRequest>,
    durability: Arc<Durability>,
}

impl DurabilityWorkerActor {
    /// Processes requests to do durability.
    async fn run(mut self) {
        while let Some(DurabilityRequest {
            reducer_context,
            tx_data,
        }) = self.request_rx.next().await
        {
            Self::do_durability(&*self.durability, reducer_context, &tx_data);
        }
    }

    pub fn do_durability(durability: &Durability, reducer_context: Option<ReducerContext>, tx_data: &TxData) {
        if tx_data.tx_offset().is_none() {
            let name = reducer_context.as_ref().map(|rcx| &*rcx.name);
            debug_assert!(
                !tx_data.has_rows_or_connect_disconnect(name),
                "tx_data has no rows but has connect/disconnect: `{name:?}`"
            );
            return;
        }

        let is_not_ephemeral_table = |table_id: &TableId| -> bool {
            tx_data
                .ephemeral_tables()
                .map(|etables| !etables.contains(table_id))
                .unwrap_or(true)
        };

        let inserts: Box<_> = tx_data
            .inserts()
            // Skip ephemeral tables
            .filter(|(table_id, _)| is_not_ephemeral_table(table_id))
            .map(|(table_id, rowdata)| Ops {
                table_id: *table_id,
                rowdata: rowdata.clone(),
            })
            .collect();

        let truncates: IntSet<TableId> = tx_data.truncates().collect();

        let deletes: Box<_> = tx_data
            .deletes()
            .filter(|(table_id, _)| is_not_ephemeral_table(table_id))
            .map(|(table_id, rowdata)| Ops {
                table_id: *table_id,
                rowdata: rowdata.clone(),
            })
            // filter out deletes for tables that are truncated in the same transaction.
            .filter(|ops| !truncates.contains(&ops.table_id))
            .collect();

        let truncates = truncates.into_iter().filter(is_not_ephemeral_table).collect();

        let inputs = reducer_context.map(|rcx| rcx.into());

        let txdata = Txdata {
            inputs,
            outputs: None,
            mutations: Some(Mutations {
                inserts,
                deletes,
                truncates,
            }),
        };

        // TODO: Should measure queuing time + actual write
        // This does not block, as per trait docs.
        durability.append_tx(txdata);
    }
}

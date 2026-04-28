use std::{sync::Arc, time::Duration};

use log::{error, info};
use spacetimedb_commitlog::payload::{
    txdata::{Mutations, Ops},
    Txdata,
};
use spacetimedb_datastore::{execution_context::ReducerContext, traits::TxData};
use spacetimedb_durability::Transaction;
use spacetimedb_lib::Identity;
use spacetimedb_sats::ProductValue;
use tokio::{runtime, time::timeout};

use crate::db::persistence::Durability;

pub(super) fn request_durability(
    durability: &Durability,
    reducer_context: Option<ReducerContext>,
    tx_data: &Arc<TxData>,
) {
    let Some(tx_offset) = tx_data.tx_offset() else {
        let name = reducer_context.as_ref().map(|rcx| &rcx.name);
        debug_assert!(
            !tx_data.has_rows_or_connect_disconnect(name),
            "tx_data has no rows but has connect/disconnect: `{name:?}`"
        );
        return;
    };
    let tx_data = tx_data.clone();
    durability.append_tx(Box::new(move || {
        prepare_tx_data_for_durability(tx_offset, reducer_context, &tx_data)
    }));
}

pub(super) fn spawn_close(durability: Arc<Durability>, runtime: &runtime::Handle, database_identity: Identity) {
    let rt = runtime.clone();
    rt.spawn(async move {
        log::info!("starting spawn close");
        let label = format!("[{database_identity}]");
        match timeout(Duration::from_secs(10), durability.close()).await {
            Err(_elapsed) => {
                error!("{label} timeout waiting for durability shutdown");
            }
            Ok(offset) => {
                info!("{label} durability shut down at tx offset: {offset:?}");
            }
        }

        log::info!("closing spawn close");
    });
}

fn prepare_tx_data_for_durability(
    tx_offset: u64,
    reducer_context: Option<ReducerContext>,
    tx_data: &TxData,
) -> Transaction<Txdata<ProductValue>> {
    let mut inserts: Box<_> = tx_data
        .persistent_inserts()
        .map(|(table_id, rowdata)| Ops { table_id, rowdata })
        .collect();
    inserts.sort_unstable_by_key(|ops| ops.table_id);

    let mut deletes: Box<_> = tx_data
        .persistent_deletes()
        .map(|(table_id, rowdata)| Ops { table_id, rowdata })
        .collect();
    deletes.sort_unstable_by_key(|ops| ops.table_id);

    let mut truncates: Box<[_]> = tx_data.persistent_truncates().collect();
    truncates.sort_unstable_by_key(|table_id| *table_id);

    let inputs = reducer_context.map(|rcx| rcx.into());

    debug_assert!(
        !(inserts.is_empty() && truncates.is_empty() && deletes.is_empty() && inputs.is_none()),
        "empty transaction"
    );

    Transaction {
        offset: tx_offset,
        txdata: Txdata {
            inputs,
            outputs: None,
            mutations: Some(Mutations {
                inserts,
                deletes,
                truncates,
            }),
        },
    }
}

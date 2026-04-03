use spacetimedb_datastore::{
    locking_tx_datastore::{
        datastore::Locking,
        lock_trace::{install_lock_event_hook, LockEvent},
    },
    traits::{IsolationLevel, MutTx, MutTxDatastore},
};
use spacetimedb_lib::{
    db::auth::{StAccess, StTableType},
    Identity,
};
use spacetimedb_primitives::TableId;
use spacetimedb_sats::{AlgebraicType, AlgebraicValue, ProductValue};
use spacetimedb_schema::{
    schema::{ColumnSchema, TableSchema},
    table_name::TableName,
};
use spacetimedb_table::page_pool::PagePool;

pub fn bootstrap_datastore() -> spacetimedb_datastore::Result<Locking> {
    Locking::bootstrap(Identity::ZERO, PagePool::new_for_test())
}

pub fn basic_table_schema(name: &str) -> TableSchema {
    TableSchema::new(
        TableId::SENTINEL,
        TableName::for_test(name),
        None,
        vec![
            ColumnSchema::for_test(0, "id", AlgebraicType::U64),
            ColumnSchema::for_test(1, "name", AlgebraicType::String),
        ],
        vec![],
        vec![],
        vec![],
        StTableType::User,
        StAccess::Public,
        None,
        None,
        false,
        None,
    )
}

pub fn create_table(datastore: &Locking, schema: TableSchema) -> spacetimedb_datastore::Result<TableId> {
    let mut tx = datastore.begin_mut_tx(
        IsolationLevel::Serializable,
        spacetimedb_datastore::execution_context::Workload::ForTests,
    );
    let table_id = datastore.create_table_mut_tx(&mut tx, schema)?;
    datastore.commit_mut_tx(tx)?;
    Ok(table_id)
}

pub fn insert_row(datastore: &Locking, table_id: TableId, id: u64, name: &str) -> spacetimedb_datastore::Result<()> {
    let row = ProductValue::from_iter([AlgebraicValue::U64(id), AlgebraicValue::String(name.into())]);
    let bytes = spacetimedb_sats::bsatn::to_vec(&row).map_err(anyhow::Error::from)?;
    let mut tx = datastore.begin_mut_tx(
        IsolationLevel::Serializable,
        spacetimedb_datastore::execution_context::Workload::ForTests,
    );
    datastore.insert_mut_tx(&mut tx, table_id, &bytes)?;
    datastore.commit_mut_tx(tx)?;
    Ok(())
}

pub fn observe_lock_events<F, R>(hook: F, body: impl FnOnce() -> R) -> R
where
    F: Fn(LockEvent) + Send + Sync + 'static,
{
    let _guard = install_lock_event_hook(hook);
    body()
}

#[cfg(test)]
mod tests {
    use std::{sync::mpsc, thread};

    use pretty_assertions::assert_eq;
    use spacetimedb_datastore::{
        execution_context::Workload,
        locking_tx_datastore::lock_trace::{LockEvent, LockEventKind},
        traits::{IsolationLevel, MutTx, Tx},
    };

    use super::{bootstrap_datastore, observe_lock_events};

    #[test]
    fn datastore_writer_waits_for_reader() {
        let datastore = bootstrap_datastore().expect("bootstrap datastore");
        let (tx, rx) = mpsc::channel::<LockEvent>();

        observe_lock_events(
            move |event| {
                tx.send(event).expect("send lock event");
            },
            || {
                let read_tx = datastore.begin_tx(Workload::ForTests);
                let datastore_for_writer = datastore.clone();

                let writer = thread::spawn(move || {
                    let write_tx = datastore_for_writer.begin_mut_tx(IsolationLevel::Serializable, Workload::ForTests);
                    let _ = datastore_for_writer.rollback_mut_tx(write_tx);
                });

                let mut events: Vec<LockEvent> = Vec::new();
                while !events
                    .iter()
                    .any(|event| event.kind == LockEventKind::BeginWriteRequested)
                {
                    events.push(rx.recv().expect("receive requested event"));
                }

                assert_eq!(
                    events.last().map(|event| event.kind),
                    Some(LockEventKind::BeginWriteRequested)
                );
                assert!(
                    !events
                        .iter()
                        .any(|event| event.kind == LockEventKind::BeginWriteAcquired),
                    "writer should not acquire while a reader is held"
                );

                drop(read_tx);
                events.push(rx.recv().expect("receive acquired event"));
                writer.join().expect("writer join");

                assert_eq!(
                    events.iter().map(|event| event.kind).collect::<Vec<_>>(),
                    vec![
                        LockEventKind::BeginReadRequested,
                        LockEventKind::BeginReadAcquired,
                        LockEventKind::BeginWriteRequested,
                        LockEventKind::BeginWriteAcquired,
                    ]
                );
            },
        );
    }
}

use super::*;
use spacetimedb_datastore::system_tables::{StEnvFields, StEnvRow, ST_ENV_ID};
use tests_utils::TestDB;

#[test]
fn operation_drain_shutdown_serializes_transactions_and_survives_cancelled_waiter() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let fixture = TestDB::durable().unwrap();
    let db = fixture.db.clone();
    runtime.block_on(async {
        let mut tx = db.begin_mut_tx(IsolationLevel::Serializable, Workload::Sql);
        insert(&mut tx, "BEFORE", "committed");
        let closing = tokio::spawn({
            let db = db.clone();
            async move { db.shutdown().await }
        });
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            while db.shutdown.get().is_none() {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        closing.abort();
        assert!(closing.await.unwrap_err().is_cancelled());
        assert!(!db.commits_closed.load(Ordering::Relaxed));
        // This SQL/view transaction acquired the exclusive lock before shutdown.
        let (_, _, read) = db.commit_tx_downgrade(tx, Workload::Sql).unwrap();
        let _ = db.release_tx(read);
        assert!(db.shutdown().await.is_some());
        assert!(db.commits_closed.load(Ordering::Relaxed));
        for downgrade in [false, true] {
            let mut late = db.begin_mut_tx(IsolationLevel::Serializable, Workload::Subscribe);
            insert(&mut late, "LATE", "must-roll-back");
            let error = if downgrade {
                db.commit_tx_downgrade(late, Workload::Subscribe).err().unwrap()
            } else {
                db.commit_tx(late).err().unwrap()
            };
            assert!(matches!(error, DBError::DatabaseClosed));
        }
        let read = db.begin_tx(Workload::Internal);
        assert_eq!(get(&read, "BEFORE").as_deref(), Some("committed"));
        assert_eq!(get(&read, "LATE"), None);
        let _ = db.release_tx(read);
    });
}

fn insert(tx: &mut MutTx, key: &str, value: &str) {
    tx.insert_via_serialize_bsatn(
        ST_ENV_ID,
        &StEnvRow {
            key: key.into(),
            value: value.into(),
        },
    )
    .unwrap();
}

fn get(tx: &Tx, key: &str) -> Option<String> {
    tx.iter_by_col_eq(ST_ENV_ID, StEnvFields::Key, &AlgebraicValue::String(key.into()))
        .unwrap()
        .next()
        .map(|row| StEnvRow::try_from(row).unwrap().value)
}

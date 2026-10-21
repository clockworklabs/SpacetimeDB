use super::*;
use spacetimedb_lib::db::raw_def::{v10::RawModuleDefV10Builder, v9::btree};
use spacetimedb_lib::{AlgebraicType, ProductType};

struct Database(*mut StdbTestDatastore);
struct Transaction(*mut StdbTestTransaction);

fn call(expected: u32, f: impl FnOnce(*mut StdbTestBuffer) -> u32) {
    let mut error = StdbTestBuffer::EMPTY;
    let status = f(&mut error);
    let message = String::from_utf8(take_buffer(error)).unwrap();
    assert_eq!(status, expected, "{message}");
    assert_eq!(message.is_empty(), status == OK);
}

fn take_buffer(mut buffer: StdbTestBuffer) -> Vec<u8> {
    unsafe {
        let bytes = input_bytes(buffer.data, buffer.len).unwrap().to_vec();
        assert_eq!(stdb_test_v1_buffer_free(&mut buffer), OK);
        bytes
    }
}

impl Database {
    fn new() -> Self {
        let mut builder = RawModuleDefV10Builder::new();
        builder
            .build_table_with_new_type(
                "person",
                ProductType::from([("id", AlgebraicType::U64), ("value", AlgebraicType::I64)]),
                true,
            )
            .with_auto_inc_primary_key(0)
            .with_index_no_accessor_name(btree(0), "person_id_idx")
            .finish();
        let schema = bsatn::to_vec(&RawModuleDef::V10(builder.finish())).unwrap();
        let mut handle = ptr::null_mut();
        call(OK, |error| unsafe {
            stdb_test_v1_datastore_create(schema.as_ptr(), schema.len() as u64, &mut handle, error)
        });
        Self(handle)
    }

    fn begin(&self) -> Transaction {
        let mut handle = ptr::null_mut();
        call(OK, |error| unsafe {
            stdb_test_v1_transaction_begin(self.0, &mut handle, error)
        });
        Transaction(handle)
    }
}

impl Drop for Database {
    fn drop(&mut self) {
        assert_eq!(unsafe { stdb_test_v1_datastore_destroy(self.0) }, OK);
    }
}

impl Drop for Transaction {
    fn drop(&mut self) {
        assert_eq!(unsafe { stdb_test_v1_transaction_destroy(self.0) }, OK);
    }
}

impl Transaction {
    fn table(&self) -> u32 {
        let mut id = 0;
        call(OK, |error| unsafe {
            stdb_test_v1_table_id(self.0, b"person".as_ptr(), 6, &mut id, error)
        });
        id
    }

    fn count(&self) -> u64 {
        let mut count = 0;
        let table = self.table();
        call(OK, |error| unsafe {
            stdb_test_v1_table_count(self.0, table, &mut count, error)
        });
        count
    }

    fn insert(&self, value: i64) -> u64 {
        let row = bsatn::to_vec(&(0_u64, value)).unwrap();
        let mut generated = StdbTestBuffer::EMPTY;
        let table = self.table();
        call(OK, |error| unsafe {
            stdb_test_v1_table_insert(self.0, table, row.as_ptr(), row.len() as u64, &mut generated, error)
        });
        let (id,): (u64,) = bsatn::from_slice(&take_buffer(generated)).unwrap();
        id
    }

    fn rows(&self) -> Vec<(u64, i64)> {
        let mut rows = StdbTestBuffer::EMPTY;
        let table = self.table();
        call(OK, |error| unsafe {
            stdb_test_v1_table_scan(self.0, table, &mut rows, error)
        });
        let rows: Vec<Vec<u8>> = bsatn::from_slice(&take_buffer(rows)).unwrap();
        rows.iter().map(|row| bsatn::from_slice(row).unwrap()).collect()
    }

    fn commit(&self) {
        call(OK, |error| unsafe { stdb_test_v1_transaction_commit(self.0, error) });
    }
}

#[test]
fn commit_writeback_and_finished_handles() {
    let db = Database::new();
    let tx = db.begin();
    let id = tx.insert(42);
    assert_ne!(id, 0);
    assert_eq!(tx.count(), 1);
    assert_eq!(tx.rows(), [(id, 42)]);
    tx.commit();
    call(TRANSACTION_FINISHED, |error| unsafe {
        stdb_test_v1_transaction_commit(tx.0, error)
    });
    let mut count = 123;
    call(TRANSACTION_FINISHED, |error| unsafe {
        stdb_test_v1_table_count(tx.0, 0, &mut count, error)
    });
    assert_eq!(count, 0);
    let next = db.begin();
    drop(tx); // A finished handle must not roll back the new transaction.
    assert_eq!(next.rows(), [(id, 42)]);
    assert_ne!(next.insert(43), id);
    next.commit();
}

#[test]
fn rollback_disposal_and_fixture_isolation() {
    let first = Database::new();
    let second = Database::new();
    let tx = first.begin();
    tx.insert(10);
    let other = second.begin();
    assert_eq!(other.count(), 0);
    other.insert(20);
    other.commit();
    call(OK, |error| unsafe { stdb_test_v1_transaction_rollback(tx.0, error) });
    call(TRANSACTION_FINISHED, |error| unsafe {
        stdb_test_v1_transaction_rollback(tx.0, error)
    });
    let pending = first.begin();
    assert_eq!(pending.count(), 0);
    pending.insert(30);
    // Transfer ownership without requiring an unsafe Send implementation.
    let pending = std::mem::ManuallyDrop::new(pending);
    let handle = unsafe { Box::from_raw(pending.0) };
    assert_eq!(
        std::thread::spawn(move || unsafe { stdb_test_v1_transaction_destroy(Box::into_raw(handle)) })
            .join()
            .unwrap(),
        OK
    );
    assert_eq!(first.begin().count(), 0);
    assert_eq!(second.begin().count(), 1);
}

#[test]
fn errors_do_not_open_nested_transactions_or_leave_outputs_uninitialized() {
    let db = Database::new();
    let tx = db.begin();
    let mut nested = ptr::null_mut();
    call(TRANSACTION_ACTIVE, |error| unsafe {
        stdb_test_v1_transaction_begin(db.0, &mut nested, error)
    });
    assert!(nested.is_null());
    assert_eq!(
        unsafe { stdb_test_v1_transaction_commit(tx.0, ptr::null_mut()) },
        INVALID_ARGUMENT
    );
    let table = tx.table();
    let mut output = StdbTestBuffer::EMPTY;
    call(DATABASE_ERROR, |error| unsafe {
        stdb_test_v1_table_insert(tx.0, table, ptr::null(), 0, &mut output, error)
    });
    assert!(output.data.is_null());
    assert_eq!(output.len, 0);
    call(INVALID_ARGUMENT, |error| unsafe {
        stdb_test_v1_table_insert(tx.0, table, ptr::null(), 1, &mut output, error)
    });
    let mut id = 123;
    call(DATABASE_ERROR, |error| unsafe {
        stdb_test_v1_table_id(tx.0, b"missing".as_ptr(), 7, &mut id, error)
    });
    assert_eq!(id, 0);
    assert_eq!(tx.count(), 0);
    for (value, expected) in [(10_i64, OK), (11, DATABASE_ERROR)] {
        let row = bsatn::to_vec(&(999_u64, value)).unwrap();
        call(expected, |error| unsafe {
            stdb_test_v1_table_insert(tx.0, table, row.as_ptr(), row.len() as u64, &mut output, error)
        });
        // Explicit IDs generate no columns; both this success and the duplicate
        // key error must return an empty, safely releasable output buffer.
        assert!(take_buffer(output).is_empty());
        output = StdbTestBuffer::EMPTY;
    }
    assert_eq!(tx.count(), 1);
    call(OK, |error| unsafe { stdb_test_v1_transaction_rollback(tx.0, error) });
    assert_eq!(db.begin().count(), 0);
}

#[test]
fn datastore_shutdown_and_worker_panic_invalidate_surviving_handles() {
    for panic in [false, true] {
        let db = Database::new();
        let tx = db.begin();
        tx.insert(10);
        if panic {
            let err = request::<()>(&unsafe { &*db.0 }.requests, |_| panic!("operation panicked")).unwrap_err();
            assert_eq!(err.0, INTERNAL_ERROR);
        }
        drop(db); // Joins the owner thread, including rollback of pending work.
        let mut count = 123;
        call(DATASTORE_CLOSED, |error| unsafe {
            stdb_test_v1_table_count(tx.0, 0, &mut count, error)
        });
        assert_eq!(count, 0);
        drop(tx);
    }
}

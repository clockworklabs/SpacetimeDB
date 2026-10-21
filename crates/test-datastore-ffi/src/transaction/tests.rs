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
    fn query(&self, sql: &str, identity: Identity, expected: u32) -> Vec<Vec<u8>> {
        let identity = bsatn::to_vec(&identity).unwrap();
        let mut output = StdbTestBuffer::EMPTY;
        call(expected, |error| unsafe {
            stdb_test_v1_query(
                self.0,
                sql.as_ptr(),
                sql.len() as u64,
                identity.as_ptr(),
                identity.len() as u64,
                &mut output,
                error,
            )
        });
        let bytes = take_buffer(output);
        if expected != OK {
            assert!(bytes.is_empty());
            vec![]
        } else {
            bsatn::from_slice(&bytes).unwrap()
        }
    }

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
            .with_index_no_accessor_name(btree(1), "person_value_idx")
            .with_index_no_accessor_name(btree([1, 0]), "person_value_id_idx")
            .finish();
        Self::from_schema(builder)
    }

    fn from_schema(builder: RawModuleDefV10Builder) -> Self {
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

#[test]
fn queries_use_committed_state_and_release_locks_on_failure() {
    let db = Database::new();
    let tx = db.begin();
    let id = tx.insert(42);
    db.query("SELECT * FROM person", Identity::ZERO, TRANSACTION_ACTIVE);
    tx.commit();
    for (sql, expected) in [
        (r#"UPDATE person SET value = 99"#, NOT_SELECT),
        (r#"INSERT INTO person (id, value) VALUES (999, 99)"#, NOT_SELECT),
        (r#"DELETE FROM person"#, NOT_SELECT),
        (r#"SELECT * FROM missing"#, DATABASE_ERROR),
        (r#"SELECT"#, DATABASE_ERROR),
        (r#"SELECT * FROM person; DELETE FROM person"#, DATABASE_ERROR),
    ] {
        db.query(sql, Identity::ZERO, expected);
        let rows = db.query("SELECT * FROM person", Identity::ZERO, OK);
        assert_eq!(rows.len(), 1);
        assert_eq!(bsatn::from_slice::<(u64, i64)>(&rows[0]).unwrap(), (id, 42));
        // A write transaction must also remain usable after query failure.
        assert_eq!(db.begin().count(), 1);
    }
    assert!(db
        .query("SELECT * FROM person WHERE value = 0", Identity::ZERO, OK)
        .is_empty());
}

#[test]
fn queries_resolve_mounted_tables_and_database_identity() {
    let mut library = RawModuleDefV10Builder::new();
    library
        .build_table_with_new_type(
            "entries",
            ProductType::from([("id", AlgebraicType::U64), ("owner", AlgebraicType::identity())]),
            true,
        )
        .finish();
    let mut root = RawModuleDefV10Builder::new();
    root.add_submodule("audit", library.finish());
    let db = Database::from_schema(root);
    let tx = db.begin();
    let mut table = 0;
    call(OK, |error| unsafe {
        stdb_test_v1_table_id(tx.0, b"audit.entries".as_ptr(), 13, &mut table, error)
    });
    let identity = Identity::from_byte_array(std::array::from_fn(|i| i as u8 + 1));
    for (id, owner) in [(7_u64, identity), (8, Identity::ZERO)] {
        let row = bsatn::to_vec(&(id, owner)).unwrap();
        let mut output = StdbTestBuffer::EMPTY;
        call(OK, |error| unsafe {
            stdb_test_v1_table_insert(tx.0, table, row.as_ptr(), row.len() as u64, &mut output, error)
        });
        assert!(take_buffer(output).is_empty());
    }
    tx.commit();
    let sql = r#"SELECT id FROM "audit"."entries" WHERE owner = :sender"#;
    for (owner, expected) in [(identity, 7_u64), (Identity::ZERO, 8)] {
        let rows = db.query(sql, owner, OK);
        assert_eq!(rows.len(), 1);
        assert_eq!(bsatn::from_slice::<(u64,)>(&rows[0]).unwrap(), (expected,));
    }
    let identity = bsatn::to_vec(&identity).unwrap();
    let mut output = StdbTestBuffer::EMPTY;
    call(INVALID_ARGUMENT, |error| unsafe {
        stdb_test_v1_query(
            db.0,
            sql.as_ptr(),
            sql.len() as u64,
            identity.as_ptr(),
            31,
            &mut output,
            error,
        )
    });
    assert!(take_buffer(output).is_empty());
    assert_eq!(db.query(sql, Identity::ZERO, OK).len(), 1);
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
    fn index(&self, name: &str) -> u32 {
        let mut id = 0;
        call(OK, |error| unsafe {
            stdb_test_v1_index_id(self.0, name.as_ptr(), name.len() as u64, &mut id, error)
        });
        id
    }

    fn point(&self, index: u32, point: &[u8]) -> Vec<(u64, i64)> {
        let mut output = StdbTestBuffer::EMPTY;
        call(OK, |error| unsafe {
            stdb_test_v1_index_scan_point(self.0, index, point.as_ptr(), point.len() as u64, &mut output, error)
        });
        decode_rows(output)
    }

    fn range(&self, index: u32, prefix: &[u8], prefix_elems: u16, start: &[u8], end: &[u8]) -> Vec<(u64, i64)> {
        let mut output = StdbTestBuffer::EMPTY;
        call(OK, |error| unsafe {
            stdb_test_v1_index_scan_range(
                self.0,
                index,
                prefix.as_ptr(),
                prefix.len() as u64,
                prefix_elems,
                start.as_ptr(),
                start.len() as u64,
                end.as_ptr(),
                end.len() as u64,
                &mut output,
                error,
            )
        });
        decode_rows(output)
    }

    fn update(&self, index: u32, row: (u64, i64), expected: u32) -> Vec<u8> {
        let table = self.table();
        let row = bsatn::to_vec(&row).unwrap();
        let mut output = StdbTestBuffer::EMPTY;
        call(expected, |error| unsafe {
            stdb_test_v1_table_update(self.0, table, index, row.as_ptr(), row.len() as u64, &mut output, error)
        });
        take_buffer(output)
    }
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

fn decode_rows(output: StdbTestBuffer) -> Vec<(u64, i64)> {
    let rows: Vec<Vec<u8>> = bsatn::from_slice(&take_buffer(output)).unwrap();
    rows.iter().map(|row| bsatn::from_slice(row).unwrap()).collect()
}

#[test]
fn point_and_composite_range_scans_and_deletes() {
    use std::ops::Bound::{Excluded, Included, Unbounded};
    let db = Database::new();
    let tx = db.begin();
    let first = tx.insert(10);
    let second = tx.insert(10);
    let third = tx.insert(20);
    tx.commit();
    let tx = db.begin();
    let primary = tx.index("person_id_idx");
    assert_eq!(tx.point(primary, &bsatn::to_vec(&first).unwrap()), [(first, 10)]);
    let composite = tx.index("person_value_id_idx");
    assert_eq!(
        tx.point(composite, &bsatn::to_vec(&(10_i64, second)).unwrap()),
        [(second, 10)]
    );
    let prefix = bsatn::to_vec(&(10_i64,)).unwrap();
    let start = bsatn::to_vec(&Included(first)).unwrap();
    let end = bsatn::to_vec(&Excluded(second)).unwrap();
    assert_eq!(tx.range(primary, &[], 0, &start, &end), [(first, 10)]);
    assert_eq!(tx.range(composite, &prefix, 1, &start, &end), [(first, 10)]);
    let unbounded = bsatn::to_vec(&Unbounded::<u64>).unwrap();
    let mut all = tx.range(composite, &prefix, 1, &unbounded, &unbounded);
    all.sort();
    assert_eq!(all, [(first, 10), (second, 10)]);
    let mut deleted = 0;
    call(OK, |error| unsafe {
        stdb_test_v1_index_delete_range(
            tx.0,
            composite,
            prefix.as_ptr(),
            prefix.len() as u64,
            1,
            start.as_ptr(),
            start.len() as u64,
            end.as_ptr(),
            end.len() as u64,
            &mut deleted,
            error,
        )
    });
    assert_eq!(deleted, 1);
    let point = bsatn::to_vec(&10_i64).unwrap();
    let value_index = tx.index("person_value_idx");
    call(OK, |error| unsafe {
        stdb_test_v1_index_delete_point(
            tx.0,
            value_index,
            point.as_ptr(),
            point.len() as u64,
            &mut deleted,
            error,
        )
    });
    assert_eq!(deleted, 1);
    assert_eq!(tx.rows(), [(third, 20)]);
    call(OK, |error| unsafe { stdb_test_v1_transaction_rollback(tx.0, error) });
    let tx = db.begin();
    assert_eq!(tx.count(), 3);
    call(OK, |error| unsafe {
        stdb_test_v1_index_delete_point(
            tx.0,
            value_index,
            point.as_ptr(),
            point.len() as u64,
            &mut deleted,
            error,
        )
    });
    assert_eq!(deleted, 2);
    tx.commit();
    assert_eq!(db.begin().rows(), [(third, 20)]);
}

#[test]
fn full_row_delete_and_clear_preserve_transaction_and_sequence_semantics() {
    let db = Database::new();
    let tx = db.begin();
    let first = tx.insert(10);
    tx.insert(20);
    tx.commit();
    let tx = db.begin();
    let table = tx.table();
    let mut deleted = 0;
    for (value, expected) in [(99_i64, 0), (10, 1), (10, 0)] {
        let row = bsatn::to_vec(&(first, value)).unwrap();
        call(OK, |error| unsafe {
            stdb_test_v1_table_delete_row(tx.0, table, row.as_ptr(), row.len() as u64, &mut deleted, error)
        });
        assert_eq!(deleted, expected);
    }
    // Decode the full row before mutating anything.
    call(DATABASE_ERROR, |error| unsafe {
        stdb_test_v1_table_delete_row(tx.0, table, ptr::null(), 0, &mut deleted, error)
    });
    assert_eq!(deleted, 0);
    let mut cleared = 0;
    call(OK, |error| unsafe {
        stdb_test_v1_table_clear(tx.0, table, &mut cleared, error)
    });
    assert_eq!(cleared, 1);
    assert_eq!(tx.count(), 0);
    call(OK, |error| unsafe { stdb_test_v1_transaction_rollback(tx.0, error) });
    let tx = db.begin();
    assert_eq!(tx.count(), 2);
    call(OK, |error| unsafe {
        stdb_test_v1_table_clear(tx.0, table, &mut cleared, error)
    });
    assert_eq!(cleared, 2);
    tx.commit();
    let tx = db.begin();
    assert_eq!(tx.count(), 0);
    assert!(tx.insert(30) > first);
    tx.commit();
    assert_eq!(db.begin().count(), 1);
}

#[test]
fn indexed_update_returns_generated_columns_and_structured_constraint_errors() {
    let mut builder = RawModuleDefV10Builder::new();
    builder
        .build_table_with_new_type(
            "person",
            ProductType::from([("id", AlgebraicType::U64), ("value", AlgebraicType::I64)]),
            true,
        )
        .with_auto_inc_primary_key(0)
        .with_unique_constraint(1)
        .with_index_no_accessor_name(btree(0), "person_id_idx")
        .with_index_no_accessor_name(btree(1), "person_value_idx")
        .finish();
    let db = Database::from_schema(builder);
    let tx = db.begin();
    let first = tx.insert(10);
    let second = tx.insert(20);
    let value_index = tx.index("person_value_idx");
    assert!(tx.update(value_index, (first, 20), UNIQUE_VIOLATION).is_empty());
    let bytes = tx.update(value_index, (0, 20), OK);
    let (generated,): (u64,) = bsatn::from_slice(&bytes).unwrap();
    assert_ne!(generated, second);
    let primary = tx.index("person_id_idx");
    assert_eq!(
        tx.point(primary, &bsatn::to_vec(&generated).unwrap()),
        [(generated, 20)]
    );
    assert!(tx.update(primary, (generated, 30), OK).is_empty());
    tx.commit();
    let tx = db.begin();
    let mut rows = tx.rows();
    rows.sort();
    assert_eq!(rows, [(first, 10), (generated, 30)]);
    let mut missing = 123;
    call(DATABASE_ERROR, |error| unsafe {
        stdb_test_v1_index_id(tx.0, b"missing".as_ptr(), 7, &mut missing, error)
    });
    assert_eq!(missing, 0);
    let mut output = StdbTestBuffer::EMPTY;
    call(DATABASE_ERROR, |error| unsafe {
        stdb_test_v1_index_scan_point(tx.0, primary, ptr::null(), 0, &mut output, error)
    });
    assert!(take_buffer(output).is_empty());
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
    for (value, expected) in [(10_i64, OK), (11, UNIQUE_VIOLATION)] {
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

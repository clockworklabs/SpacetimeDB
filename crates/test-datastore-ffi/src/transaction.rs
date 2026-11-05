use super::*;
use spacetimedb_lib::{bsatn::ToBsatn, Identity};
use spacetimedb_primitives::{ColId, IndexId, TableId};
use spacetimedb_test_datastore::TestTransaction;

#[cfg(test)]
mod tests;

pub(super) struct Worker {
    // Pending transactions roll back before the datastore is dropped.
    transaction: Option<(u64, TestTransaction)>,
    datastore: TestDatastore,
    next_id: u64,
}

impl Worker {
    pub(super) fn new(datastore: TestDatastore) -> Self {
        Self {
            transaction: None,
            datastore,
            next_id: 0,
        }
    }

    fn transaction(&self, id: u64) -> Result<&TestTransaction, Failure> {
        self.transaction
            .as_ref()
            .filter(|(active_id, _)| *active_id == id)
            .map(|(_, transaction)| transaction)
            .ok_or_else(|| (TRANSACTION_FINISHED, "Transaction is finished".into()))
    }

    fn finish(&mut self, id: u64, commit: bool) -> Result<(), Failure> {
        self.transaction(id)?;
        let (_, transaction) = self.transaction.take().unwrap();
        if commit {
            transaction.commit()
        } else {
            transaction.rollback()
        }
        .map_err(database_error)
    }
}

pub struct StdbTestTransaction {
    requests: Requests,
    id: u64,
}

/// Execute one SELECT against committed state. `identity` is exactly one BSATN `Identity`
/// (32 bytes), used as the database owner and caller, including `:sender`; this is not a
/// caller-auth/RLS test. Results are a row snapshot of the projected product rows, and the
/// caller supplies the result schema.
///
/// Queries open their own transaction. A pending explicit transaction returns
/// `TRANSACTION_ACTIVE` without waiting for its lock. Non-SELECT statements are rejected
/// without running (`NOT_SELECT` if they compile; parse and validation errors return
/// `DATABASE_ERROR`). Query failures release locks and leave the datastore usable.
///
/// # Safety
/// Pointers must satisfy the ABI contract in the crate documentation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn stdb_test_v1_query(
    datastore: *const StdbTestDatastore,
    sql: *const u8,
    sql_len: u64,
    identity: *const u8,
    identity_len: u64,
    out: *mut StdbTestBuffer,
    error: *mut StdbTestBuffer,
) -> u32 {
    unsafe {
        with_output(out, error, || {
            let datastore = handle(datastore, "datastore")?;
            let sql = input_str(sql, sql_len)?.to_owned();
            let identity = input_bytes(identity, identity_len)?;
            if identity.len() != 32 {
                return Err((
                    INVALID_ARGUMENT,
                    "Database identity must be a 32-byte BSATN Identity".into(),
                ));
            }
            let identity: Identity = bsatn::from_slice(identity).map_err(|err| (INVALID_ARGUMENT, err.to_string()))?;
            let bytes = request(&datastore.requests, move |state| {
                if state.transaction.is_some() {
                    return Err((
                        TRANSACTION_ACTIVE,
                        "Queries cannot run while a transaction is active".into(),
                    ));
                }
                let rows = state.datastore.run_select_query(&sql, identity).map_err(|err| {
                    if matches!(err, TestDatastoreError::NonSelectQuery) {
                        (NOT_SELECT, err.to_string())
                    } else {
                        database_error(err)
                    }
                })?;
                let rows = rows
                    .iter()
                    .map(ToBsatn::to_bsatn_vec)
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|err| database_error(err.into()))?;
                bsatn::to_vec(&rows).map_err(|err| database_error(err.into()))
            })?;
            Ok(StdbTestBuffer::from_bytes(bytes))
        })
    }
}

impl StdbTestTransaction {
    fn run<T: Send + 'static>(
        &self,
        f: impl FnOnce(&TestTransaction) -> Result<T, TestDatastoreError> + Send + 'static,
    ) -> Result<T, Failure> {
        let id = self.id;
        request(&self.requests, move |state| {
            f(state.transaction(id)?).map_err(database_error)
        })
    }
}

/// Begin the datastore's one pending transaction. A nested begin returns
/// `TRANSACTION_ACTIVE` rather than waiting for the existing transaction's lock. The table
/// and index functions use this explicit transaction and never open another. Table and
/// index IDs belong to this datastore.
///
/// # Safety
/// Pointers must satisfy the ABI contract in the crate documentation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn stdb_test_v1_transaction_begin(
    datastore: *const StdbTestDatastore,
    out: *mut *mut StdbTestTransaction,
    error: *mut StdbTestBuffer,
) -> u32 {
    unsafe {
        with_output(out, error, || {
            let datastore = handle(datastore, "datastore")?;
            let id = request(&datastore.requests, |state| {
                if state.transaction.is_some() {
                    return Err((TRANSACTION_ACTIVE, "A transaction is already active".into()));
                }
                let id = state
                    .next_id
                    .checked_add(1)
                    .ok_or_else(|| (INTERNAL_ERROR, "Transaction IDs exhausted".into()))?;
                state.next_id = id;
                state.transaction = Some((id, state.datastore.begin_mut_tx()));
                Ok(id)
            })?;
            Ok(Box::into_raw(Box::new(StdbTestTransaction {
                requests: datastore.requests.clone(),
                id,
            })))
        })
    }
}

unsafe fn finish(transaction: *const StdbTestTransaction, error: *mut StdbTestBuffer, commit: bool) -> u32 {
    unsafe {
        with_output(&mut (), error, || {
            let transaction = handle(transaction, "transaction")?;
            let id = transaction.id;
            request(&transaction.requests, move |state| state.finish(id, commit))
        })
    }
}

/// Commit the transaction without freeing its handle, even if the database reports an
/// error. Later operations return `TRANSACTION_FINISHED`. Argument errors do not finish
/// it. Table-operation errors leave it pending; roll back to discard partial changes.
///
/// An internal error, including a panic, stops the worker and rolls back pending work;
/// later calls report `INTERNAL_ERROR` with the original message.
///
/// # Safety
/// Pointers must satisfy the ABI contract in the crate documentation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn stdb_test_v1_transaction_commit(
    transaction: *const StdbTestTransaction,
    error: *mut StdbTestBuffer,
) -> u32 {
    unsafe { finish(transaction, error, true) }
}

/// Roll back the transaction without freeing its handle; see
/// [`stdb_test_v1_transaction_commit`].
///
/// # Safety
/// Pointers must satisfy the ABI contract in the crate documentation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn stdb_test_v1_transaction_rollback(
    transaction: *const StdbTestTransaction,
    error: *mut StdbTestBuffer,
) -> u32 {
    unsafe { finish(transaction, error, false) }
}

/// Consume the handle, rolling back if still pending. Null is a no-op. It may run on any
/// caller thread, but never concurrently with another call using this handle.
/// Datastore destruction rolls back pending work even if transaction handles survive it;
/// those handles report `DATASTORE_CLOSED` and remain safe to destroy. Destroying a
/// finished handle never affects a newer transaction.
///
/// # Safety
/// The handle must be null or a live, exclusively owned transaction handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn stdb_test_v1_transaction_destroy(transaction: *mut StdbTestTransaction) -> u32 {
    contain_panic(|| {
        if !transaction.is_null() {
            let transaction = unsafe { Box::from_raw(transaction) };
            // A stopped worker has already rolled back any pending transaction.
            if transaction.requests.has_stopped() {
                return Ok(());
            }
            let id = transaction.id;
            match request(&transaction.requests, move |state| {
                if state.transaction(id).is_ok() {
                    state.finish(id, false)?;
                }
                Ok(())
            }) {
                // Datastore shutdown has already released any pending transaction.
                Err((DATASTORE_CLOSED, _)) => (),
                result => result?,
            }
        }
        Ok(())
    })
    .map_or_else(|(status, _)| status, |()| OK)
}

/// Look up a table by UTF-8 name.
///
/// # Safety
/// Pointers must satisfy the ABI contract in the crate documentation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn stdb_test_v1_table_id(
    transaction: *const StdbTestTransaction,
    name: *const u8,
    name_len: u64,
    out: *mut u32,
    error: *mut StdbTestBuffer,
) -> u32 {
    unsafe {
        with_output(out, error, || {
            let transaction = handle(transaction, "transaction")?;
            let name = input_str(name, name_len)?.to_owned();
            transaction.run(move |tx| tx.table_id(&name).map(|id| id.0))
        })
    }
}

/// Count a table's rows, including this transaction's changes.
///
/// # Safety
/// Pointers must satisfy the ABI contract in the crate documentation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn stdb_test_v1_table_count(
    transaction: *const StdbTestTransaction,
    table_id: u32,
    out: *mut u64,
    error: *mut StdbTestBuffer,
) -> u32 {
    unsafe {
        with_output(out, error, || {
            let transaction = handle(transaction, "transaction")?;
            transaction.run(move |tx| tx.table_row_count(TableId(table_id)))
        })
    }
}

/// Return a row snapshot of the table.
///
/// # Safety
/// Pointers must satisfy the ABI contract in the crate documentation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn stdb_test_v1_table_scan(
    transaction: *const StdbTestTransaction,
    table_id: u32,
    out: *mut StdbTestBuffer,
    error: *mut StdbTestBuffer,
) -> u32 {
    unsafe {
        with_output(out, error, || {
            let transaction = handle(transaction, "transaction")?;
            let bytes = transaction.run(move |tx| Ok(bsatn::to_vec(&tx.table_rows_bsatn(TableId(table_id))?)?))?;
            Ok(StdbTestBuffer::from_bytes(bytes))
        })
    }
}

/// Look up an index by UTF-8 name.
///
/// # Safety
/// Pointers must satisfy the ABI contract in the crate documentation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn stdb_test_v1_index_id(
    transaction: *const StdbTestTransaction,
    name: *const u8,
    name_len: u64,
    out: *mut u32,
    error: *mut StdbTestBuffer,
) -> u32 {
    unsafe {
        with_output(out, error, || {
            let transaction = handle(transaction, "transaction")?;
            let name = input_str(name, name_len)?.to_owned();
            transaction.run(move |tx| tx.index_id(&name).map(|id| id.0))
        })
    }
}

/// Return a row snapshot of the rows matching a point key. The key is encoded as the
/// index key type: a scalar for one column, a product for several.
///
/// # Safety
/// Pointers must satisfy the ABI contract in the crate documentation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn stdb_test_v1_index_scan_point(
    transaction: *const StdbTestTransaction,
    index_id: u32,
    point: *const u8,
    point_len: u64,
    out: *mut StdbTestBuffer,
    error: *mut StdbTestBuffer,
) -> u32 {
    unsafe {
        with_output(out, error, || {
            let transaction = handle(transaction, "transaction")?;
            let point = input_bytes(point, point_len)?.to_vec();
            let bytes = transaction
                .run(move |tx| Ok(bsatn::to_vec(&tx.index_scan_point_bsatn(IndexId(index_id), &point)?)?))?;
            Ok(StdbTestBuffer::from_bytes(bytes))
        })
    }
}

/// Delete the rows matching a point key (encoded as for
/// [`stdb_test_v1_index_scan_point`]) and return how many were deleted.
///
/// # Safety
/// Pointers must satisfy the ABI contract in the crate documentation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn stdb_test_v1_index_delete_point(
    transaction: *const StdbTestTransaction,
    index_id: u32,
    point: *const u8,
    point_len: u64,
    out: *mut u32,
    error: *mut StdbTestBuffer,
) -> u32 {
    unsafe {
        with_output(out, error, || {
            let transaction = handle(transaction, "transaction")?;
            let point = input_bytes(point, point_len)?.to_vec();
            transaction.run(move |tx| tx.delete_by_index_scan_point_bsatn(IndexId(index_id), &point))
        })
    }
}

/// Return a row snapshot of a range. The first `prefix_elems` index columns are fixed to
/// a BSATN product `prefix` (zero bytes when `prefix_elems` is 0). `start` and `end` are
/// BSATN `Bound` values on the next column, using the engine's encoding: tag 0 is
/// `Included(value)`, 1 is `Excluded(value)`, 2 is `Unbounded`. Both bounds are required,
/// including the tag for `Unbounded`. Later columns are unrestricted.
///
/// # Safety
/// Pointers must satisfy the ABI contract in the crate documentation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn stdb_test_v1_index_scan_range(
    transaction: *const StdbTestTransaction,
    index_id: u32,
    prefix: *const u8,
    prefix_len: u64,
    prefix_elems: u16,
    start: *const u8,
    start_len: u64,
    end: *const u8,
    end_len: u64,
    out: *mut StdbTestBuffer,
    error: *mut StdbTestBuffer,
) -> u32 {
    unsafe {
        with_output(out, error, || {
            let transaction = handle(transaction, "transaction")?;
            let prefix = input_bytes(prefix, prefix_len)?.to_vec();
            let start = input_bytes(start, start_len)?.to_vec();
            let end = input_bytes(end, end_len)?.to_vec();
            let bytes = transaction.run(move |tx| {
                Ok(bsatn::to_vec(&tx.index_scan_range_bsatn(
                    IndexId(index_id),
                    &prefix,
                    ColId(prefix_elems),
                    &start,
                    &end,
                )?)?)
            })?;
            Ok(StdbTestBuffer::from_bytes(bytes))
        })
    }
}

/// Delete the rows in a range (encoded as for [`stdb_test_v1_index_scan_range`]) and
/// return how many were deleted.
///
/// # Safety
/// Pointers must satisfy the ABI contract in the crate documentation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn stdb_test_v1_index_delete_range(
    transaction: *const StdbTestTransaction,
    index_id: u32,
    prefix: *const u8,
    prefix_len: u64,
    prefix_elems: u16,
    start: *const u8,
    start_len: u64,
    end: *const u8,
    end_len: u64,
    out: *mut u32,
    error: *mut StdbTestBuffer,
) -> u32 {
    unsafe {
        with_output(out, error, || {
            let transaction = handle(transaction, "transaction")?;
            let prefix = input_bytes(prefix, prefix_len)?.to_vec();
            let start = input_bytes(start, start_len)?.to_vec();
            let end = input_bytes(end, end_len)?.to_vec();
            transaction.run(move |tx| {
                tx.delete_by_index_scan_range_bsatn(IndexId(index_id), &prefix, ColId(prefix_elems), &start, &end)
            })
        })
    }
}

/// Update the row found through the unique index using key fields from the supplied full
/// row. Returns generated columns exactly as [`stdb_test_v1_table_insert`] does, with the
/// same constraint statuses.
///
/// # Safety
/// Pointers must satisfy the ABI contract in the crate documentation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn stdb_test_v1_table_update(
    transaction: *const StdbTestTransaction,
    table_id: u32,
    index_id: u32,
    row: *const u8,
    row_len: u64,
    out: *mut StdbTestBuffer,
    error: *mut StdbTestBuffer,
) -> u32 {
    unsafe {
        with_output(out, error, || {
            let transaction = handle(transaction, "transaction")?;
            let row = input_bytes(row, row_len)?.to_vec();
            let bytes = transaction
                .run(move |tx| tx.update_bsatn_generated_cols(TableId(table_id), IndexId(index_id), &row))?;
            Ok(StdbTestBuffer::from_bytes(bytes))
        })
    }
}

/// Delete a row matching the entire row, not just its primary key. Returns 0 or 1. Event
/// rows stay visible in their transaction but are not retained at commit.
///
/// # Safety
/// Pointers must satisfy the ABI contract in the crate documentation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn stdb_test_v1_table_delete_row(
    transaction: *const StdbTestTransaction,
    table_id: u32,
    row: *const u8,
    row_len: u64,
    out: *mut u32,
    error: *mut StdbTestBuffer,
) -> u32 {
    unsafe {
        with_output(out, error, || {
            let transaction = handle(transaction, "transaction")?;
            let row = input_bytes(row, row_len)?.to_vec();
            transaction.run(move |tx| tx.delete_row_bsatn(TableId(table_id), &row))
        })
    }
}

/// Remove every row, preserving schema and sequence state, and return how many were
/// removed. Like other table operations it can be rolled back.
///
/// # Safety
/// Pointers must satisfy the ABI contract in the crate documentation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn stdb_test_v1_table_clear(
    transaction: *const StdbTestTransaction,
    table_id: u32,
    out: *mut u64,
    error: *mut StdbTestBuffer,
) -> u32 {
    unsafe {
        with_output(out, error, || {
            let transaction = handle(transaction, "transaction")?;

            transaction.run(move |tx| tx.clear_table(TableId(table_id)))
        })
    }
}

/// Insert a BSATN row and return the BSATN product of its generated column values, in
/// the engine's generated-column order. Apply them to the caller's row, and free the
/// buffer even if writeback fails. A row with no generated values returns an empty product
/// (zero bytes). Constraint failures report `UNIQUE_VIOLATION` and `AUTO_INC_OVERFLOW`.
///
/// # Safety
/// Pointers must satisfy the ABI contract in the crate documentation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn stdb_test_v1_table_insert(
    transaction: *const StdbTestTransaction,
    table_id: u32,
    row: *const u8,
    row_len: u64,
    out: *mut StdbTestBuffer,
    error: *mut StdbTestBuffer,
) -> u32 {
    unsafe {
        with_output(out, error, || {
            let transaction = handle(transaction, "transaction")?;
            let row = input_bytes(row, row_len)?.to_vec();
            let bytes = transaction.run(move |tx| tx.insert_bsatn_generated_cols(TableId(table_id), &row))?;
            Ok(StdbTestBuffer::from_bytes(bytes))
        })
    }
}

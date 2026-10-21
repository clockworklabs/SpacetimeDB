use super::*;
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
    requests: Sender<Command>,
    id: u64,
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

/// # Safety
/// Pointers must satisfy the ownership and memory contract in the C header.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn stdb_test_v1_transaction_begin(
    datastore: *const StdbTestDatastore,
    out: *mut *mut StdbTestTransaction,
    error: *mut StdbTestBuffer,
) -> u32 {
    unsafe {
        with_output(out, error, || {
            let datastore = datastore
                .as_ref()
                .ok_or_else(|| (INVALID_ARGUMENT, "Null datastore".into()))?;
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
            let transaction = transaction
                .as_ref()
                .ok_or_else(|| (INVALID_ARGUMENT, "Null transaction".into()))?;
            let id = transaction.id;
            request(&transaction.requests, move |state| state.finish(id, commit))
        })
    }
}

/// # Safety
/// Pointers must satisfy the ownership and memory contract in the C header.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn stdb_test_v1_transaction_commit(
    transaction: *const StdbTestTransaction,
    error: *mut StdbTestBuffer,
) -> u32 {
    unsafe { finish(transaction, error, true) }
}

/// # Safety
/// Pointers must satisfy the ownership and memory contract in the C header.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn stdb_test_v1_transaction_rollback(
    transaction: *const StdbTestTransaction,
    error: *mut StdbTestBuffer,
) -> u32 {
    unsafe { finish(transaction, error, false) }
}

/// # Safety
/// The handle must be null or a live, exclusively owned transaction handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn stdb_test_v1_transaction_destroy(transaction: *mut StdbTestTransaction) -> u32 {
    contain_panic(|| {
        if !transaction.is_null() {
            let transaction = unsafe { Box::from_raw(transaction) };
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

/// # Safety
/// Pointers must satisfy the ownership and memory contract in the C header.
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
            let transaction = transaction
                .as_ref()
                .ok_or_else(|| (INVALID_ARGUMENT, "Null transaction".into()))?;
            let name = std::str::from_utf8(input_bytes(name, name_len)?)
                .map_err(|err| (INVALID_ARGUMENT, err.to_string()))?
                .to_owned();
            transaction.run(move |tx| tx.table_id(&name).map(|id| id.0))
        })
    }
}

/// # Safety
/// Pointers must satisfy the ownership and memory contract in the C header.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn stdb_test_v1_table_count(
    transaction: *const StdbTestTransaction,
    table_id: u32,
    out: *mut u64,
    error: *mut StdbTestBuffer,
) -> u32 {
    unsafe {
        with_output(out, error, || {
            let transaction = transaction
                .as_ref()
                .ok_or_else(|| (INVALID_ARGUMENT, "Null transaction".into()))?;
            transaction.run(move |tx| tx.table_row_count(TableId(table_id)))
        })
    }
}

/// # Safety
/// Pointers must satisfy the ownership and memory contract in the C header.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn stdb_test_v1_table_scan(
    transaction: *const StdbTestTransaction,
    table_id: u32,
    out: *mut StdbTestBuffer,
    error: *mut StdbTestBuffer,
) -> u32 {
    unsafe {
        with_output(out, error, || {
            let transaction = transaction
                .as_ref()
                .ok_or_else(|| (INVALID_ARGUMENT, "Null transaction".into()))?;
            let bytes = transaction.run(move |tx| Ok(bsatn::to_vec(&tx.table_rows_bsatn(TableId(table_id))?)?))?;
            Ok(StdbTestBuffer::from_bytes(bytes))
        })
    }
}

/// # Safety
/// Pointers must satisfy the ownership and memory contract in the C header.
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
            let transaction = transaction
                .as_ref()
                .ok_or_else(|| (INVALID_ARGUMENT, "Null transaction".into()))?;
            let name = std::str::from_utf8(input_bytes(name, name_len)?)
                .map_err(|err| (INVALID_ARGUMENT, err.to_string()))?
                .to_owned();
            transaction.run(move |tx| tx.index_id(&name).map(|id| id.0))
        })
    }
}

/// # Safety
/// Pointers must satisfy the ownership and memory contract in the C header.
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
            let transaction = transaction
                .as_ref()
                .ok_or_else(|| (INVALID_ARGUMENT, "Null transaction".into()))?;
            let point = input_bytes(point, point_len)?.to_vec();
            let bytes = transaction
                .run(move |tx| Ok(bsatn::to_vec(&tx.index_scan_point_bsatn(IndexId(index_id), &point)?)?))?;
            Ok(StdbTestBuffer::from_bytes(bytes))
        })
    }
}

/// # Safety
/// Pointers must satisfy the ownership and memory contract in the C header.
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
            let transaction = transaction
                .as_ref()
                .ok_or_else(|| (INVALID_ARGUMENT, "Null transaction".into()))?;
            let point = input_bytes(point, point_len)?.to_vec();
            transaction.run(move |tx| tx.delete_by_index_scan_point_bsatn(IndexId(index_id), &point))
        })
    }
}

/// # Safety
/// Pointers must satisfy the ownership and memory contract in the C header.
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
            let transaction = transaction
                .as_ref()
                .ok_or_else(|| (INVALID_ARGUMENT, "Null transaction".into()))?;
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

/// # Safety
/// Pointers must satisfy the ownership and memory contract in the C header.
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
            let transaction = transaction
                .as_ref()
                .ok_or_else(|| (INVALID_ARGUMENT, "Null transaction".into()))?;
            let prefix = input_bytes(prefix, prefix_len)?.to_vec();
            let start = input_bytes(start, start_len)?.to_vec();
            let end = input_bytes(end, end_len)?.to_vec();
            transaction.run(move |tx| {
                tx.delete_by_index_scan_range_bsatn(IndexId(index_id), &prefix, ColId(prefix_elems), &start, &end)
            })
        })
    }
}

/// # Safety
/// Pointers must satisfy the ownership and memory contract in the C header.
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
            let transaction = transaction
                .as_ref()
                .ok_or_else(|| (INVALID_ARGUMENT, "Null transaction".into()))?;
            let row = input_bytes(row, row_len)?.to_vec();
            let bytes = transaction
                .run(move |tx| tx.update_bsatn_generated_cols(TableId(table_id), IndexId(index_id), &row))?;
            Ok(StdbTestBuffer::from_bytes(bytes))
        })
    }
}

/// # Safety
/// Pointers must satisfy the ownership and memory contract in the C header.
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
            let transaction = transaction
                .as_ref()
                .ok_or_else(|| (INVALID_ARGUMENT, "Null transaction".into()))?;
            let row = input_bytes(row, row_len)?.to_vec();
            transaction.run(move |tx| tx.delete_row_bsatn(TableId(table_id), &row))
        })
    }
}

/// # Safety
/// Pointers must satisfy the ownership and memory contract in the C header.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn stdb_test_v1_table_clear(
    transaction: *const StdbTestTransaction,
    table_id: u32,
    out: *mut u64,
    error: *mut StdbTestBuffer,
) -> u32 {
    unsafe {
        with_output(out, error, || {
            let transaction = transaction
                .as_ref()
                .ok_or_else(|| (INVALID_ARGUMENT, "Null transaction".into()))?;

            transaction.run(move |tx| tx.clear_table(TableId(table_id)))
        })
    }
}

/// # Safety
/// Pointers must satisfy the ownership and memory contract in the C header.
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
            let transaction = transaction
                .as_ref()
                .ok_or_else(|| (INVALID_ARGUMENT, "Null transaction".into()))?;
            let row = input_bytes(row, row_len)?.to_vec();
            let bytes = transaction.run(move |tx| tx.insert_bsatn_generated_cols(TableId(table_id), &row))?;
            Ok(StdbTestBuffer::from_bytes(bytes))
        })
    }
}

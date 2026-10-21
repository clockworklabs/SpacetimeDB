//! Native test ABI. See include/spacetimedb_test_datastore.h for the ownership contract.

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::ptr;
use std::sync::mpsc::{self, Sender};
use std::thread::{self, JoinHandle};

use spacetimedb_lib::{bsatn, RawModuleDef};
use spacetimedb_test_datastore::{TestDatastore, TestDatastoreError};

mod transaction;
use transaction::Worker;

const OK: u32 = 0;
const INVALID_ARGUMENT: u32 = 1;
const INVALID_SCHEMA: u32 = 2;
const DATABASE_ERROR: u32 = 3;
const INTERNAL_ERROR: u32 = 4;
const TRANSACTION_FINISHED: u32 = 5;
const TRANSACTION_ACTIVE: u32 = 6;
const DATASTORE_CLOSED: u32 = 7;
const UNIQUE_VIOLATION: u32 = 8;
const AUTO_INC_OVERFLOW: u32 = 9;
const NOT_SELECT: u32 = 10;

#[repr(C)]
pub struct StdbTestBuffer {
    data: *mut u8,
    len: u64,
}

impl StdbTestBuffer {
    const EMPTY: Self = Self {
        data: ptr::null_mut(),
        len: 0,
    };

    fn from_message(message: String) -> Self {
        Self::from_bytes(message.into_bytes())
    }

    fn from_bytes(bytes: Vec<u8>) -> Self {
        if bytes.is_empty() {
            return Self::EMPTY;
        }
        let bytes = bytes.into_boxed_slice();
        let len = bytes.len() as u64;
        Self {
            data: Box::into_raw(bytes).cast(),
            len,
        }
    }
}

impl Default for StdbTestBuffer {
    fn default() -> Self {
        Self::EMPTY
    }
}

// Only the channel and join handle cross threads. In particular, a
// TestTransaction must stay on the owner thread: MutTxId is explicitly !Send.
pub struct StdbTestDatastore {
    requests: Sender<Command>,
    worker: JoinHandle<()>,
}

type Failure = (u32, String);

enum Command {
    Run(Box<dyn FnOnce(&mut Worker) -> bool + Send>),
    Shutdown,
}

fn request<T: Send + 'static>(
    requests: &Sender<Command>,
    f: impl FnOnce(&mut Worker) -> Result<T, Failure> + Send + 'static,
) -> Result<T, Failure> {
    let (reply, response) = mpsc::sync_channel(1);
    requests
        .send(Command::Run(Box::new(move |state| {
            let result = contain_panic(|| f(state));
            // A panicking operation may have left the transaction inconsistent.
            // Stop this worker, rolling back before accepting any further work.
            let healthy = !matches!(&result, Err((INTERNAL_ERROR, _)));
            let _ = reply.send(result);
            healthy
        })))
        .map_err(|_| (DATASTORE_CLOSED, "Datastore is closed".into()))?;
    response
        .recv()
        .map_err(|_| (DATASTORE_CLOSED, "Datastore worker stopped".into()))?
}

fn database_error(err: TestDatastoreError) -> Failure {
    use spacetimedb_primitives::errno;
    let status = match err.insert_errno_code() {
        Some(code) if code == errno::UNIQUE_ALREADY_EXISTS.get() => UNIQUE_VIOLATION,
        Some(code) if code == errno::AUTO_INC_OVERFLOW.get() => AUTO_INC_OVERFLOW,
        _ => DATABASE_ERROR,
    };
    (status, err.to_string())
}

/// Initialize output slots before performing an operation, including on failure.
/// Callers must supply valid, nonoverlapping storage, as described in the header.
unsafe fn with_output<T: Default>(
    out: *mut T,
    error: *mut StdbTestBuffer,
    f: impl FnOnce() -> Result<T, Failure>,
) -> u32 {
    let result = contain_panic(|| {
        if !out.is_null() {
            unsafe { out.write(T::default()) };
        }
        if !error.is_null() {
            unsafe { error.write(StdbTestBuffer::EMPTY) };
        }
        if out.is_null() || error.is_null() {
            return Err((INVALID_ARGUMENT, "Null output slot".into()));
        }
        unsafe { out.write(f()?) };
        Ok(())
    });
    match result {
        Ok(()) => OK,
        Err((status, message)) => {
            if !error.is_null() {
                unsafe { error.write(StdbTestBuffer::from_message(message)) };
            }
            status
        }
    }
}

unsafe fn input_bytes<'a>(data: *const u8, len: u64) -> Result<&'a [u8], Failure> {
    if (data.is_null() && len != 0) || len > isize::MAX as u64 {
        return Err((INVALID_ARGUMENT, "Invalid input pointer or length".into()));
    }
    Ok(if len == 0 {
        &[]
    } else {
        unsafe { std::slice::from_raw_parts(data, len as usize) }
    })
}

fn contain_panic<T>(f: impl FnOnce() -> Result<T, Failure>) -> Result<T, Failure> {
    catch_unwind(AssertUnwindSafe(f)).unwrap_or_else(|payload| {
        // Release ordinary payloads, but don't let a payload's destructor unwind
        // across the ABI. Only a second, panicking destructor forces a leak.
        if let Err(nested) = catch_unwind(AssertUnwindSafe(|| drop(payload))) {
            std::mem::forget(nested);
        }
        Err((INTERNAL_ERROR, "Native test datastore panicked".into()))
    })
}

fn create(schema: &[u8]) -> Result<StdbTestDatastore, Failure> {
    let mut remaining = schema;
    let raw: RawModuleDef = bsatn::from_reader(&mut remaining).map_err(|err| (INVALID_SCHEMA, err.to_string()))?;
    if !remaining.is_empty() {
        return Err((INVALID_SCHEMA, "Trailing bytes after module definition".into()));
    }

    let (ready_tx, ready_rx) = mpsc::sync_channel(1);
    let (requests, commands) = mpsc::channel();
    let worker = thread::Builder::new()
        .name("stdb-test-datastore".into())
        .spawn(move || {
            let result = contain_panic(|| {
                TestDatastore::from_module_def(raw).map_err(|err| {
                    let status = if matches!(err, TestDatastoreError::ModuleDef(_)) {
                        INVALID_SCHEMA
                    } else {
                        DATABASE_ERROR
                    };
                    (status, err.to_string())
                })
            });
            match result {
                Ok(datastore) => {
                    let mut state = Worker::new(datastore);
                    if ready_tx.send(Ok(())).is_ok() {
                        while let Ok(Command::Run(run)) = commands.recv() {
                            if !run(&mut state) {
                                break;
                            }
                        }
                    }
                    drop(state);
                }
                Err(err) => {
                    let _ = ready_tx.send(Err(err));
                }
            }
        })
        .map_err(|err| (INTERNAL_ERROR, err.to_string()))?;

    let result = ready_rx
        .recv()
        .unwrap_or_else(|err| Err((INTERNAL_ERROR, err.to_string())));
    if let Err(err) = result {
        drop(requests);
        if let Err(payload) = worker.join() {
            std::panic::resume_unwind(payload);
        }
        return Err(err);
    }
    Ok(StdbTestDatastore { requests, worker })
}

#[unsafe(no_mangle)]
pub extern "C" fn stdb_test_abi_version() -> u32 {
    1
}

/// # Safety
/// Pointers must satisfy the ownership and memory contract in the C header.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn stdb_test_v1_datastore_create(
    schema: *const u8,
    schema_len: u64,
    out: *mut *mut StdbTestDatastore,
    error: *mut StdbTestBuffer,
) -> u32 {
    unsafe {
        with_output(out, error, || {
            let datastore = create(input_bytes(schema, schema_len)?)?;
            Ok(Box::into_raw(Box::new(datastore)))
        })
    }
}

/// # Safety
/// The handle must be null or a live, exclusively owned handle returned by create.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn stdb_test_v1_datastore_destroy(datastore: *mut StdbTestDatastore) -> u32 {
    contain_panic(|| {
        if !datastore.is_null() {
            let StdbTestDatastore { requests, worker } = *unsafe { Box::from_raw(datastore) };
            let _ = requests.send(Command::Shutdown);
            drop(requests);
            if let Err(payload) = worker.join() {
                std::panic::resume_unwind(payload);
            }
        }
        Ok(())
    })
    .map_or_else(|(status, _)| status, |()| OK)
}

/// # Safety
/// The slot must be null or contain an unmodified, exclusively owned returned buffer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn stdb_test_v1_buffer_free(buffer: *mut StdbTestBuffer) -> u32 {
    contain_panic(|| {
        if !buffer.is_null() {
            let buffer = unsafe { &mut *buffer };
            if !buffer.data.is_null() {
                let bytes = ptr::slice_from_raw_parts_mut(buffer.data, buffer.len as usize);
                unsafe { drop(Box::from_raw(bytes)) };
            }
            *buffer = StdbTestBuffer::EMPTY;
        }
        Ok(())
    })
    .map_or_else(|(status, _)| status, |()| OK)
}

#[cfg(test)]
mod tests {
    use super::*;
    use spacetimedb_lib::db::raw_def::v10::RawModuleDefV10Builder;
    use spacetimedb_lib::{sats::AlgebraicTypeRef, AlgebraicType, ProductType};

    fn schema() -> Vec<u8> {
        let mut builder = RawModuleDefV10Builder::new();
        builder
            .build_table_with_new_type("person", ProductType::from([("id", AlgebraicType::I64)]), true)
            .finish();
        bsatn::to_vec(&RawModuleDef::V10(builder.finish())).unwrap()
    }

    #[test]
    fn lifecycle_across_caller_threads() {
        assert_eq!(stdb_test_abi_version(), 1);
        let schema = schema();
        let mut handle = ptr::null_mut();
        let mut error = StdbTestBuffer::EMPTY;
        unsafe {
            assert_eq!(
                stdb_test_v1_datastore_create(schema.as_ptr(), schema.len() as u64, &mut handle, &mut error),
                OK
            );
            assert!(!handle.is_null());
            assert!(error.data.is_null());
            assert_eq!(error.len, 0);
            // Moving the owned box also checks that the handle is Send without an unsafe impl.
            let handle = Box::from_raw(handle);
            assert_eq!(
                thread::spawn(move || stdb_test_v1_datastore_destroy(Box::into_raw(handle)))
                    .join()
                    .unwrap(),
                OK
            );
            assert_eq!(stdb_test_v1_datastore_destroy(ptr::null_mut()), OK);
            assert_eq!(stdb_test_v1_buffer_free(&mut error), OK);
        }
    }

    #[test]
    fn schema_errors_own_their_diagnostics() {
        let mut invalid = RawModuleDefV10Builder::new();
        invalid.build_table("broken", AlgebraicTypeRef(999)).finish();
        let invalid = bsatn::to_vec(&RawModuleDef::V10(invalid.finish())).unwrap();
        let mut trailing = schema();
        trailing.push(0);
        for bytes in [vec![], vec![255], invalid, trailing] {
            let mut handle = ptr::null_mut();
            let mut error = StdbTestBuffer::EMPTY;
            unsafe {
                assert_eq!(
                    stdb_test_v1_datastore_create(bytes.as_ptr(), bytes.len() as u64, &mut handle, &mut error),
                    INVALID_SCHEMA
                );
                assert!(handle.is_null());
                assert!(error.len > 0);
                assert!(std::str::from_utf8(std::slice::from_raw_parts(error.data, error.len as usize)).is_ok());
                assert_eq!(stdb_test_v1_buffer_free(&mut error), OK);
                assert!(error.data.is_null());
                assert_eq!(error.len, 0);
                assert_eq!(stdb_test_v1_buffer_free(&mut error), OK);
            }
        }
        let mut error = StdbTestBuffer::EMPTY;
        unsafe {
            assert_eq!(
                stdb_test_v1_datastore_create(ptr::null(), 1, ptr::null_mut(), &mut error),
                INVALID_ARGUMENT
            );
            assert_eq!(stdb_test_v1_buffer_free(&mut error), OK);
        }
    }

    #[test]
    fn panics_become_internal_errors() {
        assert_eq!(
            contain_panic::<()>(|| panic!("test panic")).unwrap_err().0,
            INTERNAL_ERROR
        );
    }
}

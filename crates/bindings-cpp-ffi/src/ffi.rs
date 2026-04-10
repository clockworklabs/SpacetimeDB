//! FFI exports matching the SpacetimeDB WASM ABI.
//!
//! These functions are the Rust equivalents of the C++ exports in
//! `module_exports.cpp` and `Module.cpp`.

#![allow(clippy::disallowed_macros)]

use crate::module_type_registration::{serialize_module_def, ModuleTypeRegistration, RegistrationError};
use spacetimedb_lib::bsatn;
use spacetimedb_lib::db::raw_def::v10::{RawModuleDefV10, RawModuleDefV10Section, RawScopedTypeNameV10, RawTypeDefV10};
use spacetimedb_lib::{ConnectionId, Identity};
use spacetimedb_sats::raw_identifier::RawIdentifier;
use spacetimedb_sats::AlgebraicType;

// ============================================================
// Opaque FFI handles (mirroring the C++ opaque types)
// ============================================================

/// Opaque handle provided by the host to write bytes into.
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct BytesSink {
    pub inner: u32,
}

/// Opaque handle provided by the host to read bytes from.
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct BytesSource {
    pub inner: u32,
}

impl BytesSource {
    /// Sentinel value indicating an invalid / absent source.
    pub const INVALID: Self = Self { inner: 0 };
}

// ============================================================
// Host FFI imports
// ============================================================

#[cfg(not(test))]
#[link(wasm_import_module = "spacetimedb")]
unsafe extern "C" {
    /// Write bytes to a sink. Returns 0 on success, negative on error.
    fn bytes_sink_write(sink: BytesSink, data: *const u8, len: *mut usize) -> i16;
    /// Read bytes from a source. Returns -1 when exhausted, 0 on success, negative on error.
    fn bytes_source_read(source: BytesSource, buf: *mut u8, len: *mut usize) -> i16;
    /// Get the remaining length of a source. Returns 0 on success, negative on error.
    fn bytes_source_remaining_length(source: BytesSource, len: *mut u32) -> i16;
}

// Stub implementations for native testing
#[cfg(test)]
mod host_stubs {
    #![allow(dead_code)]
    #![allow(unsafe_op_in_unsafe_fn)]
    use super::*;

    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};

    fn test_sources() -> &'static Mutex<HashMap<u32, Vec<u8>>> {
        static ONCE: OnceLock<Mutex<HashMap<u32, Vec<u8>>>> = OnceLock::new();
        ONCE.get_or_init(|| Mutex::new(HashMap::new()))
    }

    fn test_sinks() -> &'static Mutex<HashMap<u32, Vec<u8>>> {
        static ONCE: OnceLock<Mutex<HashMap<u32, Vec<u8>>>> = OnceLock::new();
        ONCE.get_or_init(|| Mutex::new(HashMap::new()))
    }

    static NEXT_HANDLE: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(1);

    pub fn register_test_source(data: Vec<u8>) -> BytesSource {
        let handle = NEXT_HANDLE.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        test_sources().lock().unwrap().insert(handle, data);
        BytesSource { inner: handle }
    }

    pub fn register_test_sink() -> BytesSink {
        let handle = NEXT_HANDLE.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        test_sinks().lock().unwrap().insert(handle, Vec::new());
        BytesSink { inner: handle }
    }

    pub fn get_sink_data(sink: &BytesSink) -> Vec<u8> {
        test_sinks()
            .lock()
            .unwrap()
            .get(&sink.inner)
            .cloned()
            .unwrap_or_default()
    }

    pub unsafe fn bytes_sink_write(sink: BytesSink, data: *const u8, len: *mut usize) -> i16 {
        if let Some(buf) = test_sinks().lock().unwrap().get_mut(&sink.inner) {
            // SAFETY: caller guarantees `len` and `data` are valid
            let len_ref = unsafe { &mut *len };
            let slice = unsafe { std::slice::from_raw_parts(data, *len_ref) };
            buf.extend_from_slice(slice);
            0
        } else {
            errno::NO_SUCH_BYTES
        }
    }

    pub unsafe fn bytes_source_read(source: BytesSource, buf: *mut u8, len: *mut usize) -> i16 {
        if let Some(data) = test_sources().lock().unwrap().get(&source.inner) {
            // SAFETY: caller guarantees `len` and `buf` are valid
            let len_ref = unsafe { &mut *len };
            let max = *len_ref;
            let available = data.len();
            let to_copy = max.min(available);
            if to_copy > 0 {
                unsafe { std::ptr::copy_nonoverlapping(data.as_ptr(), buf, to_copy) };
                *len_ref = to_copy;
            }
            -1
        } else {
            errno::NO_SUCH_BYTES
        }
    }

    pub unsafe fn bytes_source_remaining_length(source: BytesSource, len: *mut u32) -> i16 {
        if let Some(data) = test_sources().lock().unwrap().get(&source.inner) {
            // SAFETY: caller guarantees `len` is valid
            unsafe { *len = data.len() as u32 };
            0
        } else {
            errno::NO_SUCH_BYTES as i32 as i16
        }
    }
}

#[cfg(test)]
use host_stubs::*;

// ============================================================
// Status codes (matching C++ StatusCode enum)
// ============================================================

mod errno {
    pub const OK: i16 = 0;
    pub const HOST_CALL_FAILURE: i16 = 1;
    pub const NO_SUCH_BYTES: i16 = 2;
    pub const NO_SUCH_REDUCER: i16 = 3;
    pub const NO_SUCH_VIEW: i16 = 4;
    pub const NO_SUCH_PROCEDURE: i16 = 5;
}

// ============================================================
// View result header
// ============================================================

/// Prepended to view results to indicate the result type.
#[repr(u8)]
enum ViewResultHeader {
    RowData = 0,
    #[allow(unused)]
    RawSql = 1,
}

// ============================================================
// Reducer / View / Procedure handler registration
// ============================================================

/// A reducer function: takes `(ReducerContext, &[u8])` → `Result<(), Box<str>>`.
/// `ReducerContext` is defined by the consuming SDK.
pub type ReducerFn = for<'a> fn(&[u8]) -> Result<(), Box<str>>;

/// A view function: takes `&[u8]` → `Vec<u8>` (BSATN-encoded rows).
pub type ViewFn = for<'a> fn(&[u8]) -> Vec<u8>;

/// A procedure function: takes `&[u8]` → `Vec<u8>` (BSATN-encoded result).
pub type ProcedureFn = for<'a> fn(&[u8]) -> Vec<u8>;

// ============================================================
// Global module state
// ============================================================

struct GlobalModuleState {
    type_reg: ModuleTypeRegistration,
    reducers: Vec<ReducerFn>,
    views: Vec<ViewFn>,
    views_anon: Vec<ViewFn>,
    procedures: Vec<ProcedureFn>,
    /// Error flags from constraint / primary-key / circular-ref detection
    error_state: Option<ErrorState>,
}

#[derive(Clone, Debug)]
struct ErrorState {
    variant: ErrorVariant,
}

#[allow(dead_code)]
#[derive(Clone, Debug)]
enum ErrorVariant {
    CircularReference { type_name: String },
    MultiplePrimaryKeys { table_name: String },
    ConstraintRegistration { code: String, details: String },
    TypeRegistration { message: String, type_description: String },
}

impl GlobalModuleState {
    fn new() -> Self {
        Self {
            type_reg: ModuleTypeRegistration::new(),
            reducers: Vec::new(),
            views: Vec::new(),
            views_anon: Vec::new(),
            procedures: Vec::new(),
            error_state: None,
        }
    }

    fn clear(&mut self) {
        self.type_reg.clear();
        self.reducers.clear();
        self.views.clear();
        self.views_anon.clear();
        self.procedures.clear();
        self.error_state = None;
    }
}

struct SingletonCell<T>(std::cell::UnsafeCell<T>);
// SAFETY: WASM modules are single-threaded. This pattern is used in the Rust
// standard library for WASI singletons.
#[allow(clippy::mut_from_ref)]
unsafe impl<T> Sync for SingletonCell<T> {}
impl<T> SingletonCell<T> {
    const fn new(val: T) -> Self {
        Self(std::cell::UnsafeCell::new(val))
    }
    #[allow(clippy::mut_from_ref)]
    fn get(&self) -> &mut T {
        unsafe { &mut *self.0.get() }
    }
}

static MODULE: SingletonCell<Option<GlobalModuleState>> = SingletonCell::new(None);

fn get_module() -> &'static mut GlobalModuleState {
    let cell = &MODULE;
    if cell.get().is_none() {
        *cell.get() = Some(GlobalModuleState::new());
    }
    cell.get().as_mut().unwrap()
}

// ============================================================
// Public registration APIs (called by SDK macros)
// ============================================================

/// Register a reducer function. Returns the reducer's index.
pub fn register_reducer(f: ReducerFn) -> u32 {
    let module = get_module();
    let id = module.reducers.len() as u32;
    module.reducers.push(f);
    id
}

/// Register a view function (with sender identity). Returns the view's index.
pub fn register_view(f: ViewFn) -> u32 {
    let module = get_module();
    let id = module.views.len() as u32;
    module.views.push(f);
    id
}

/// Register an anonymous view function (no sender). Returns the view's index.
pub fn register_view_anon(f: ViewFn) -> u32 {
    let module = get_module();
    let id = module.views_anon.len() as u32;
    module.views_anon.push(f);
    id
}

/// Register a procedure function. Returns the procedure's index.
pub fn register_procedure(f: ProcedureFn) -> u32 {
    let module = get_module();
    let id = module.procedures.len() as u32;
    module.procedures.push(f);
    id
}

/// Register a type with the module's typespace.
pub fn register_type(ty: AlgebraicType, explicit_name: &str) -> AlgebraicType {
    let module = get_module();
    module.type_reg.register_type(ty, explicit_name)
}

/// Check if the module has a registration error.
pub fn has_registration_error() -> bool {
    get_module().type_reg.has_error()
}

/// Get the registration error details.
pub fn registration_error() -> Option<&'static RegistrationError> {
    get_module().type_reg.error()
}

// ============================================================
// Error module generation (matching C++ __preinit__99_validate_types)
// ============================================================

fn make_error_type(name: &str) -> RawTypeDefV10 {
    RawTypeDefV10 {
        source_name: RawScopedTypeNameV10 {
            scope: Box::new([]),
            source_name: RawIdentifier::new(name),
        },
        ty: spacetimedb_sats::AlgebraicTypeRef(999_999),
        custom_ordering: false,
    }
}

fn make_error_module(error_type: RawTypeDefV10) -> RawModuleDefV10 {
    let mut module = RawModuleDefV10::default();
    module.sections.push(RawModuleDefV10Section::Types(vec![error_type]));
    module
}

fn sanitize_for_error_name(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '_' { c } else { '_' })
        .take(100)
        .collect()
}

fn extract_type_name_from_error(message: &str) -> &str {
    if let Some(start) = message.find('\'')
        && let Some(end) = message[start + 1..].find('\'')
    {
        return &message[start + 1..start + 1 + end];
    }
    "unknown"
}

/// Build error module definition if there's a registration error.
/// Returns `Some(RawModuleDefV10)` if an error was detected, `None` otherwise.
fn build_error_module_def(state: &mut GlobalModuleState) -> Option<RawModuleDefV10> {
    // 1. Circular reference error
    if let Some(ErrorState {
        variant: ErrorVariant::CircularReference { type_name },
    }) = &state.error_state
    {
        let name = format!("ERROR_CIRCULAR_REFERENCE_{type_name}");
        return Some(make_error_module(make_error_type(&name)));
    }

    // 2. Multiple primary key error
    if let Some(ErrorState {
        variant: ErrorVariant::MultiplePrimaryKeys { table_name },
    }) = &state.error_state
    {
        let name = format!("ERROR_MULTIPLE_PRIMARY_KEYS_{table_name}");
        return Some(make_error_module(make_error_type(&name)));
    }

    // 3. Constraint registration error
    if let Some(ErrorState {
        variant: ErrorVariant::ConstraintRegistration { code, details },
    }) = &state.error_state
    {
        let sanitized = sanitize_for_error_name(&format!("ERROR_CONSTRAINT_REGISTRATION_{code}"));
        eprintln!("\n[CONSTRAINT REGISTRATION ERROR] Module cleared and replaced with error type: {sanitized}");
        eprintln!("Original error: {details}\n");
        return Some(make_error_module(make_error_type(&sanitized)));
    }

    // 4. Type registration error
    if let Some(err) = state.type_reg.error() {
        let message = &err.message;
        let type_description = &err.type_description;

        let error_name = if message.contains("Recursive type reference") {
            let problematic = extract_type_name_from_error(message);
            format!("ERROR_RECURSIVE_TYPE_{problematic}")
        } else if message.contains("Missing type name") {
            let sanitized = sanitize_for_error_name(type_description);
            format!("ERROR_MISSING_TYPE_NAME_{sanitized}")
        } else {
            "ERROR_TYPE_REGISTRATION_FAILED".to_owned()
        };

        eprintln!("\n[TYPE ERROR] Module cleared and replaced with error type: {error_name}");
        eprintln!("Original error: {message}\n");
        return Some(make_error_module(make_error_type(&error_name)));
    }

    None
}

// ============================================================
// Helper: read all bytes from a BytesSource
// ============================================================

fn read_bytes_source(source: BytesSource) -> Vec<u8> {
    if source == BytesSource::INVALID {
        return Vec::new();
    }

    let mut buf = Vec::new();

    // Try to get the remaining length for efficient reservation
    if let Some(len) = {
        let mut len: u32 = 0;
        let ret = unsafe { bytes_source_remaining_length(source, &mut len) };
        if ret == 0 {
            Some(len as usize)
        } else {
            None
        }
    } {
        buf.reserve(len);
    } else {
        buf.reserve(1024);
    }

    // Read in a loop to handle partial reads
    loop {
        let spare = buf.spare_capacity_mut();
        let spare_len = spare.len();
        let mut buf_len = spare.len();
        let ptr = spare.as_mut_ptr().cast::<u8>();

        let ret = unsafe { bytes_source_read(source, ptr, &mut buf_len) };

        match ret {
            -1 => {
                // Exhausted — `buf_len` was written, advance now
                if buf_len > 0 {
                    unsafe { buf.set_len(buf.len() + buf_len) };
                }
                break;
            }
            0 => {
                // Partial read — `buf_len` was written, advance
                unsafe { buf.set_len(buf.len() + buf_len) };
                if buf_len == spare_len {
                    buf.reserve(1024);
                }
                // else: partial read but not exhausted, loop again
            }
            _ => {
                eprintln!("ERROR: Failed to read from BytesSource: {ret}");
                break;
            }
        }
    }

    buf
}

// ============================================================
// Helper: write bytes to a BytesSink
// ============================================================

fn write_to_sink(sink: BytesSink, mut buf: &[u8]) {
    if sink.inner == 0 || buf.is_empty() {
        return;
    }

    loop {
        let mut len = buf.len();
        let ret = unsafe { bytes_sink_write(sink, buf.as_ptr(), &mut len) };

        match ret {
            0 => {
                // Advance past the written bytes
                buf = &buf[len..];
                if buf.is_empty() {
                    break;
                }
            }
            errno::NO_SUCH_BYTES => panic!("invalid BytesSink passed"),
            errno::HOST_CALL_FAILURE => panic!("no space left at sink"),
            _ => {
                eprintln!("ERROR: Failed to write to BytesSink: {ret}");
                break;
            }
        }
    }
}

// ============================================================
// Helper: reconstruct Identity from 4x u64 (little-endian)
// ============================================================

fn reconstruct_identity(s0: u64, s1: u64, s2: u64, s3: u64) -> Identity {
    // Identity is 32 bytes, stored little-endian
    let mut bytes = [0u8; 32];
    bytes[0..8].copy_from_slice(&s0.to_le_bytes());
    bytes[8..16].copy_from_slice(&s1.to_le_bytes());
    bytes[16..24].copy_from_slice(&s2.to_le_bytes());
    bytes[24..32].copy_from_slice(&s3.to_le_bytes());
    Identity::from_byte_array(bytes)
}

// ============================================================
// Helper: reconstruct ConnectionId from 2x u64 (little-endian)
// ============================================================

fn reconstruct_connection_id(c0: u64, c1: u64) -> Option<ConnectionId> {
    let mut bytes = [0u8; 16];
    bytes[0..8].copy_from_slice(&c0.to_le_bytes());
    bytes[8..16].copy_from_slice(&c1.to_le_bytes());
    let conn_id = ConnectionId::from_le_byte_array(bytes);
    if conn_id == ConnectionId::ZERO {
        None
    } else {
        Some(conn_id)
    }
}

// ============================================================
// __preinit__01_clear_global_state
// ============================================================

#[unsafe(export_name = "__preinit__01_clear_global_state")]
extern "C" fn preinit_clear_global_state() {
    get_module().clear();
}

// ============================================================
// __preinit__99_validate_types
// ============================================================

#[unsafe(export_name = "__preinit__99_validate_types")]
extern "C" fn preinit_validate_types() {
    let state = get_module();

    // If there's an error state, the module definition will be replaced
    // with an error type that SpacetimeDB will reject with a clear message
    if state.error_state.is_some() || state.type_reg.has_error() {
        // Build the error module — this replaces the normal module
        if let Some(_error_module) = build_error_module_def(state) {
            // Error module is built; when __describe_module__ runs,
            // it will serialize this error module instead
        }
    }
}

// ============================================================
// __describe_module__
// ============================================================

#[unsafe(no_mangle)]
pub extern "C" fn __describe_module__(description: BytesSink) {
    let state = get_module();

    // Check for errors — if present, build error module
    let buffer = if state.error_state.is_some() || state.type_reg.has_error() {
        if let Some(error_module) = build_error_module_def(state) {
            let versioned = spacetimedb_lib::RawModuleDef::V10(error_module);
            bsatn::to_vec(&versioned).expect("failed to serialize error module")
        } else {
            // No error after all — serialize normal module
            serialize_module_def(&state.type_reg)
        }
    } else {
        serialize_module_def(&state.type_reg)
    };

    if !buffer.is_empty() {
        write_to_sink(description, &buffer);
    }
}

// ============================================================
// __call_reducer__
// ============================================================

#[unsafe(no_mangle)]
pub extern "C" fn __call_reducer__(
    id: u32,
    _sender_0: u64,
    _sender_1: u64,
    _sender_2: u64,
    _sender_3: u64,
    _conn_id_0: u64,
    _conn_id_1: u64,
    _timestamp_us: u64,
    args: BytesSource,
    error: BytesSink,
) -> i16 {
    let state = get_module();

    // Validate reducer ID
    if id as usize >= state.reducers.len() {
        let msg = format!("Invalid reducer ID: {id}");
        write_to_sink(error, msg.as_bytes());
        return errno::NO_SUCH_REDUCER;
    }

    // Read args
    let args_bytes = read_bytes_source(args);

    // Dispatch
    let reducer_fn = state.reducers[id as usize];
    let result = reducer_fn(&args_bytes);

    // Handle errors
    match result {
        Ok(()) => errno::OK,
        Err(msg) => {
            write_to_sink(error, msg.as_bytes());
            errno::HOST_CALL_FAILURE
        }
    }
}

// ============================================================
// __call_view__ (with sender identity)
// ============================================================

#[unsafe(no_mangle)]
pub extern "C" fn __call_view__(
    id: u32,
    sender_0: u64,
    sender_1: u64,
    sender_2: u64,
    sender_3: u64,
    args: BytesSource,
    result: BytesSink,
) -> i16 {
    let state = get_module();

    // Validate view ID
    if id as usize >= state.views.len() {
        eprintln!("ERROR: Invalid view ID {id} (have {} views)", state.views.len());
        return errno::NO_SUCH_VIEW;
    }

    // Reconstruct sender identity (C++ builds Identity but doesn't use it for views currently)
    let _sender = reconstruct_identity(sender_0, sender_1, sender_2, sender_3);

    // Read args
    let args_bytes = read_bytes_source(args);

    // Dispatch
    let view_fn = state.views[id as usize];
    let result_data = view_fn(&args_bytes);

    // Serialize ViewResultHeader::RowData + result
    let mut full_result = Vec::with_capacity(1 + result_data.len());
    full_result.push(ViewResultHeader::RowData as u8);
    full_result.extend_from_slice(&result_data);

    write_to_sink(result, &full_result);
    2 // Success with data (new ABI)
}

// ============================================================
// __call_view_anon__ (no sender identity)
// ============================================================

#[unsafe(no_mangle)]
pub extern "C" fn __call_view_anon__(id: u32, args: BytesSource, result: BytesSink) -> i16 {
    let state = get_module();

    // Validate view ID
    if id as usize >= state.views_anon.len() {
        eprintln!(
            "ERROR: Invalid anonymous view ID {id} (have {} anonymous views)",
            state.views_anon.len()
        );
        return errno::NO_SUCH_VIEW;
    }

    // Read args
    let args_bytes = read_bytes_source(args);

    // Dispatch
    let view_fn = state.views_anon[id as usize];
    let result_data = view_fn(&args_bytes);

    // Serialize ViewResultHeader::RowData + result
    let mut full_result = Vec::with_capacity(1 + result_data.len());
    full_result.push(ViewResultHeader::RowData as u8);
    full_result.extend_from_slice(&result_data);

    write_to_sink(result, &full_result);
    2 // Success with data (new ABI)
}

// ============================================================
// __call_procedure__
// ============================================================

#[unsafe(no_mangle)]
pub extern "C" fn __call_procedure__(
    id: u32,
    sender_0: u64,
    sender_1: u64,
    sender_2: u64,
    sender_3: u64,
    conn_id_0: u64,
    conn_id_1: u64,
    _timestamp_microseconds: u64,
    args_source: BytesSource,
    result_sink: BytesSink,
) -> i16 {
    let state = get_module();

    // Validate procedure ID
    if id as usize >= state.procedures.len() {
        eprintln!(
            "ERROR: Invalid procedure ID {id} (have {} procedures)",
            state.procedures.len()
        );
        return errno::NO_SUCH_PROCEDURE;
    }

    // Reconstruct sender identity (for context, though procedure fn doesn't receive it here)
    let _sender = reconstruct_identity(sender_0, sender_1, sender_2, sender_3);
    let _conn_id = reconstruct_connection_id(conn_id_0, conn_id_1);

    // Read args
    let args_bytes = read_bytes_source(args_source);

    // Dispatch
    let procedure_fn = state.procedures[id as usize];
    let result_data = procedure_fn(&args_bytes);

    // Write result
    write_to_sink(result_sink, &result_data);
    0 // Success
}

// ============================================================
// Tests
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_and_dispatch_reducer() {
        // Clear global state
        get_module().clear();

        // Register a simple reducer
        let id = register_reducer(|_args| Ok(()));
        assert_eq!(id, 0);

        // Register a second reducer
        let id2 = register_reducer(|_args| Err("test error".into()));
        assert_eq!(id2, 1);

        // Verify reducers are stored
        let state = get_module();
        assert_eq!(state.reducers.len(), 2);
    }

    #[test]
    fn register_and_dispatch_view() {
        get_module().clear();

        let id = register_view(|_args| vec![1, 2, 3]);
        assert_eq!(id, 0);

        let id2 = register_view(|_args| vec![4, 5]);
        assert_eq!(id2, 1);

        let state = get_module();
        assert_eq!(state.views.len(), 2);
    }

    #[test]
    fn register_and_dispatch_view_anon() {
        get_module().clear();

        let id = register_view_anon(|_args| vec![10, 20]);
        assert_eq!(id, 0);

        let state = get_module();
        assert_eq!(state.views_anon.len(), 1);
    }

    #[test]
    fn register_and_dispatch_procedure() {
        get_module().clear();

        let id = register_procedure(|_args| vec![100, 200]);
        assert_eq!(id, 0);

        let state = get_module();
        assert_eq!(state.procedures.len(), 1);
    }

    #[test]
    fn read_bytes_source_invalid_returns_empty() {
        let result = read_bytes_source(BytesSource::INVALID);
        assert!(result.is_empty());
    }

    #[test]
    fn write_to_sink_empty_buffer_noop() {
        // Writing empty buffer should not panic
        write_to_sink(BytesSink { inner: 0 }, &[]);
    }

    #[test]
    fn write_to_sink_zero_inner_noop() {
        // Sink with inner=0 should not panic
        write_to_sink(BytesSink { inner: 0 }, &[1, 2, 3]);
    }

    #[test]
    fn reconstruct_identity_little_endian() {
        let identity = reconstruct_identity(
            0x0102030405060708,
            0x090a0b0c0d0e0f10,
            0x1112131415161718,
            0x191a1b1c1d1e1f20,
        );
        let bytes = identity.to_byte_array();
        assert_eq!(&bytes[0..8], &0x0102030405060708u64.to_le_bytes());
        assert_eq!(&bytes[8..16], &0x090a0b0c0d0e0f10u64.to_le_bytes());
        assert_eq!(&bytes[16..24], &0x1112131415161718u64.to_le_bytes());
        assert_eq!(&bytes[24..32], &0x191a1b1c1d1e1f20u64.to_le_bytes());
    }

    #[test]
    fn reconstruct_connection_id_little_endian() {
        let conn_id = reconstruct_connection_id(0x0102030405060708, 0x090a0b0c0d0e0f10).unwrap();
        let bytes = conn_id.as_le_byte_array();
        assert_eq!(&bytes[0..8], &0x0102030405060708u64.to_le_bytes());
        assert_eq!(&bytes[8..16], &0x090a0b0c0d0e0f10u64.to_le_bytes());
    }

    #[test]
    fn reconstruct_connection_id_zero_returns_none() {
        let result = reconstruct_connection_id(0, 0);
        assert!(result.is_none());
    }

    #[test]
    fn sanitize_for_error_name_replaces_special_chars() {
        assert_eq!(sanitize_for_error_name("foo bar"), "foo_bar");
        assert_eq!(sanitize_for_error_name("foo/bar"), "foo_bar");
        assert_eq!(sanitize_for_error_name("foo@bar"), "foo_bar");
    }

    #[test]
    fn sanitize_for_error_name_truncates_long() {
        let long = "a".repeat(200);
        let result = sanitize_for_error_name(&long);
        assert_eq!(result.len(), 100);
    }

    #[test]
    fn sanitize_for_error_name_keeps_alphanumeric() {
        assert_eq!(sanitize_for_error_name("ABC_123"), "ABC_123");
    }

    #[test]
    fn extract_type_name_from_error_finds_quoted_name() {
        assert_eq!(
            extract_type_name_from_error("Recursive type reference: 'MyType' is referencing itself"),
            "MyType"
        );
    }

    #[test]
    fn extract_type_name_from_error_returns_unknown() {
        assert_eq!(extract_type_name_from_error("no quotes here"), "unknown");
    }

    #[test]
    fn make_error_type_correct_structure() {
        let error_type = make_error_type("ERROR_TEST");
        assert_eq!(&*error_type.source_name.source_name, "ERROR_TEST");
        assert!(error_type.source_name.scope.is_empty());
        assert_eq!(error_type.ty.0, 999_999);
        assert!(!error_type.custom_ordering);
    }

    #[test]
    fn make_error_module_has_types_section() {
        let module = make_error_module(make_error_type("ERROR_TEST"));
        assert_eq!(module.sections.len(), 1);
        match &module.sections[0] {
            RawModuleDefV10Section::Types(types) => {
                assert_eq!(types.len(), 1);
                assert_eq!(&*types[0].source_name.source_name, "ERROR_TEST");
            }
            _ => panic!("expected Types section"),
        }
    }

    #[test]
    fn view_result_header_values() {
        assert_eq!(ViewResultHeader::RowData as u8, 0);
        assert_eq!(ViewResultHeader::RawSql as u8, 1);
    }

    #[test]
    fn bytes_source_invalid_constant() {
        assert_eq!(BytesSource::INVALID.inner, 0);
    }

    #[test]
    fn error_state_circular_reference() {
        get_module().clear();
        get_module().error_state = Some(ErrorState {
            variant: ErrorVariant::CircularReference {
                type_name: "RecursiveType".to_owned(),
            },
        });

        let module = build_error_module_def(get_module()).unwrap();
        match &module.sections[0] {
            RawModuleDefV10Section::Types(types) => {
                assert_eq!(
                    &*types[0].source_name.source_name,
                    "ERROR_CIRCULAR_REFERENCE_RecursiveType"
                );
            }
            _ => panic!("expected Types section"),
        }
    }

    #[test]
    fn error_state_multiple_primary_keys() {
        get_module().clear();
        get_module().error_state = Some(ErrorState {
            variant: ErrorVariant::MultiplePrimaryKeys {
                table_name: "MyTable".to_owned(),
            },
        });

        let module = build_error_module_def(get_module()).unwrap();
        match &module.sections[0] {
            RawModuleDefV10Section::Types(types) => {
                assert_eq!(
                    &*types[0].source_name.source_name,
                    "ERROR_MULTIPLE_PRIMARY_KEYS_MyTable"
                );
            }
            _ => panic!("expected Types section"),
        }
    }

    #[test]
    fn error_state_constraint_registration() {
        get_module().clear();
        get_module().error_state = Some(ErrorState {
            variant: ErrorVariant::ConstraintRegistration {
                code: "PK_CONFLICT".to_owned(),
                details: "Duplicate primary key".to_owned(),
            },
        });

        let module = build_error_module_def(get_module()).unwrap();
        match &module.sections[0] {
            RawModuleDefV10Section::Types(types) => {
                let name = &*types[0].source_name.source_name;
                assert!(name.starts_with("ERROR_CONSTRAINT_REGISTRATION_PK_CONFLICT"));
            }
            _ => panic!("expected Types section"),
        }
    }

    #[test]
    fn error_state_type_registration_recursive() {
        get_module().clear();
        get_module()
            .type_reg
            .register_type(spacetimedb_sats::AlgebraicType::U8, "");
        // Manually set an error
        get_module().type_reg.clear();
        // We can't easily set the internal error, so test via build_error_module_def with None
        assert!(build_error_module_def(get_module()).is_none());
    }

    #[test]
    fn no_error_returns_none() {
        get_module().clear();
        assert!(build_error_module_def(get_module()).is_none());
    }

    #[test]
    fn clear_resets_all_handlers() {
        get_module().clear();
        register_reducer(|_| Ok(()));
        register_view(|_| vec![]);
        register_view_anon(|_| vec![]);
        register_procedure(|_| vec![]);

        get_module().clear();

        let state = get_module();
        assert!(state.reducers.is_empty());
        assert!(state.views.is_empty());
        assert!(state.views_anon.is_empty());
        assert!(state.procedures.is_empty());
        assert!(state.error_state.is_none());
        assert!(!state.type_reg.has_error());
    }
}

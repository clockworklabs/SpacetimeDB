//! Versioned built-in module for a database published with only a container.
//!
//! This is a real Wasm program, not an empty byte string. It declares an empty
//! V10 user schema and hosted_auth_v1, so normal database initialization, system
//! tables, subscriptions, and later migration use the existing host machinery.
//! Its required reducer ABI entry point traps because no reducer is declared.
//!
//! Version 1 bytes are immutable. Reproduce them with
//! `python3 crates/core/src/host/empty_module/generate.py --check` from public/.
//! New program bytes require a new version, preserving old deployment replay.

use spacetimedb_datastore::{system_tables::ModuleKind, traits::Program};
use spacetimedb_lib::{hash_bytes, Hash};
use std::sync::OnceLock;

pub const VERSION_1: u32 = 1;
pub use spacetimedb_lib::deployment::SYSTEM_EMPTY_MODULE_V1_PROGRAM_HASH as VERSION_1_PROGRAM_HASH;

pub use spacetimedb_lib::deployment::SYSTEM_EMPTY_MODULE_V1_BYTES as VERSION_1_BYTES;

/// Return the exact bundled program for a recognized system module version.
/// Unknown versions fail closed instead of silently selecting the latest one.
pub fn program(version: u32) -> Option<Program> {
    (version == VERSION_1 && v1_hash() == VERSION_1_PROGRAM_HASH).then(|| Program {
        hash: v1_hash(),
        bytes: VERSION_1_BYTES.into(),
        kind: ModuleKind::WASM,
    })
}

/// Validate a system-empty deployment against immutable platform bytes, not
/// against an empty-looking schema supplied by a publisher or a claimed hash.
pub fn matches_program(version: u32, candidate: &Program) -> bool {
    version == VERSION_1
        && candidate.kind == ModuleKind::WASM
        && candidate.hash == VERSION_1_PROGRAM_HASH
        && v1_hash() == VERSION_1_PROGRAM_HASH
        && candidate.bytes.as_ref() == VERSION_1_BYTES
}

fn v1_hash() -> Hash {
    static HASH: OnceLock<Hash> = OnceLock::new();
    *HASH.get_or_init(|| hash_bytes(VERSION_1_BYTES))
}

#[cfg(test)]
mod tests;

#![allow(clippy::disallowed_macros)]

// Put `spacetimedb` in the extern prelude for the whole crate, naming the client SDK,
// so that the module source's `spacetimedb::...` paths resolve on the client.
// `spacetimedb_sdk::...` keeps working, so the existing handlers compile unchanged.
// (Cargo's `spacetimedb = { package = "spacetimedb-sdk" }` would also work, but would break `spacetimedb_sdk::` paths:
// Cargo rejects depending on one package under two names.)
extern crate spacetimedb_sdk as spacetimedb;

// The module's source file, unmodified, including reducer bodies.
#[path = "../../../../../../modules/sdk-test-event-table/src/lib.rs"]
mod module;

mod module_bindings {
    pub use super::module::*;

    spacetimedb::client_module! {
        tables: [super::module::test_event],
        reducers: [
            super::module::emit_test_event(name, value),
            super::module::emit_multiple_test_events,
            super::module::noop,
        ],
    }
}

// The existing client's handlers, unchanged.
#[path = "../../../event-table-client/src/test_handlers.rs"]
pub mod test_handlers;

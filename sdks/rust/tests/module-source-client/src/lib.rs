#![allow(clippy::disallowed_macros)]

// Name the client SDK `spacetimedb` in every module of the crate, so that the module source's
// `spacetimedb::...` paths resolve to the client expansions. `spacetimedb_sdk::...` paths keep working.
extern crate spacetimedb_sdk as spacetimedb;

// The module's source, unmodified, including reducer bodies, which the client expansions discard.
// Its imports of `ReducerContext` and `Table` are then unused.
#[allow(unused_imports)]
#[path = "../../../../../modules/sdk-test-event-table/src/lib.rs"]
mod module;

// `client_module!` generates items with fixed names, such as `DbConnection`, so it gets a module of its own.
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

// `event-table-client`'s handlers, unchanged.
#[path = "../../event-table-client/src/test_handlers.rs"]
pub mod test_handlers;

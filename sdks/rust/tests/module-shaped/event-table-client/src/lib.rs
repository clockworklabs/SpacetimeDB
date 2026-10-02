#![allow(clippy::disallowed_macros)]

mod module_bindings;

// The existing client's handlers, unchanged.
#[path = "../../../event-table-client/src/test_handlers.rs"]
pub mod test_handlers;

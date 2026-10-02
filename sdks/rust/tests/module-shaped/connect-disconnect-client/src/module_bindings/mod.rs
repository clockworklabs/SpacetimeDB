// Module-shaped client bindings for `modules/sdk-test-connect-disconnect` (proposal 0040 spike).
// Hand-written in the layout codegen would emit: one file per declaration, plus this file.

use spacetimedb_sdk as spacetimedb;

pub mod connected_table;
pub mod disconnected_table;
pub mod identity_connected_reducer;
pub mod identity_disconnected_reducer;

pub use connected_table::*;
pub use disconnected_table::*;
pub use identity_connected_reducer::*;
pub use identity_disconnected_reducer::*;

spacetimedb::client_module! {
    tables: [connected_table::connected, disconnected_table::disconnected],
    reducers: [
        identity_connected_reducer::identity_connected,
        identity_disconnected_reducer::identity_disconnected,
    ],
}

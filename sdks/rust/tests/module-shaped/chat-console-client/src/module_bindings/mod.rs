// Module-shaped client bindings for `templates/chat-console-rs/spacetimedb` (proposal 0040 spike).
// Hand-written in the layout codegen would emit: one file per declaration, plus this file.
// Like today's generated bindings, they omit the private lifecycle reducers.

use spacetimedb_sdk as spacetimedb;

pub mod message_table;
pub mod send_message_reducer;
pub mod set_name_reducer;
pub mod user_table;

pub use message_table::*;
pub use send_message_reducer::*;
pub use set_name_reducer::*;
pub use user_table::*;

spacetimedb::client_module! {
    tables: [message_table::message, user_table::user],
    reducers: [send_message_reducer::send_message(text), set_name_reducer::set_name(name)],
}

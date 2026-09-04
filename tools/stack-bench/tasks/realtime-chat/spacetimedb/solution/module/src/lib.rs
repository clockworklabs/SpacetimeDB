// Reference (oracle) SpacetimeDB chat module for the stack-bench realtime-chat task.
//
// NOTE: written against the SpacetimeDB 1.x bindings. Verify it builds against
// the `spacetime` version pinned in ../../environment/Dockerfile before relying
// on `harbor run -a oracle` scoring 1.0.

use spacetimedb::{reducer, table, ReducerContext, Table, Timestamp};

#[table(name = message, public)]
pub struct Message {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    pub sender: String,
    pub text: String,
    pub sent_at: Timestamp,
}

#[reducer]
pub fn send_message(ctx: &ReducerContext, sender: String, text: String) -> Result<(), String> {
    if text.is_empty() {
        return Err("text must not be empty".to_string());
    }
    ctx.db.message().insert(Message {
        id: 0, // auto_inc: 0 is a placeholder; the real id is assigned on insert
        sender,
        text,
        sent_at: ctx.timestamp,
    });
    Ok(())
}

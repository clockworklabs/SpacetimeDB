use spacetimedb::{Identity, ProcedureContext, ReducerContext, ScheduleAt, Table, Timestamp};
use spacetimedb::sats::{i256, u256};

#[spacetimedb::table(accessor = user, public)]
pub struct User {
    #[primary_key]
    identity: Identity,
    name: Option<String>,
    online: bool,
}

#[spacetimedb::table(accessor = message, public)]
pub struct Message {
    #[auto_inc]
    #[primary_key]
    id: u64,
    sender: Identity,
    sent: Timestamp,
    text: String,
}

/// A simple note table — used to test onDelete and filtered subscriptions.
#[spacetimedb::table(accessor = note, public)]
pub struct Note {
    #[auto_inc]
    #[primary_key]
    id: u64,
    owner: Identity,
    content: String,
    tag: String,
}

/// Scheduled table — tests ScheduleAt and TimeDuration types.
/// When a row's scheduled_at time arrives, the server calls send_reminder.
#[spacetimedb::table(accessor = reminder, public, scheduled(send_reminder))]
pub struct Reminder {
    #[primary_key]
    #[auto_inc]
    scheduled_id: u64,
    scheduled_at: ScheduleAt,
    text: String,
    owner: Identity,
}

/// Table with large integer fields — tests Int128/UInt128/Int256/UInt256 codegen.
#[spacetimedb::table(accessor = big_int_row, public)]
pub struct BigIntRow {
    #[primary_key]
    #[auto_inc]
    id: u64,
    val_i128: i128,
    val_u128: u128,
    val_i256: i256,
    val_u256: u256,
}

#[spacetimedb::reducer]
pub fn insert_big_ints(
    ctx: &ReducerContext,
    val_i128: i128,
    val_u128: u128,
    val_i256: i256,
    val_u256: u256,
) -> Result<(), String> {
    ctx.db.big_int_row().insert(BigIntRow {
        id: 0,
        val_i128,
        val_u128,
        val_i256,
        val_u256,
    });
    Ok(())
}

fn validate_name(name: String) -> Result<String, String> {
    if name.is_empty() {
        Err("Names must not be empty".to_string())
    } else {
        Ok(name)
    }
}

#[spacetimedb::reducer]
pub fn set_name(ctx: &ReducerContext, name: String) -> Result<(), String> {
    let name = validate_name(name)?;
    if let Some(user) = ctx.db.user().identity().find(ctx.sender()) {
        log::info!("User {} sets name to {name}", ctx.sender());
        ctx.db.user().identity().update(User {
            name: Some(name),
            ..user
        });
        Ok(())
    } else {
        Err("Cannot set name for unknown user".to_string())
    }
}

fn validate_message(text: String) -> Result<String, String> {
    if text.is_empty() {
        Err("Messages must not be empty".to_string())
    } else {
        Ok(text)
    }
}

#[spacetimedb::reducer]
pub fn send_message(ctx: &ReducerContext, text: String) -> Result<(), String> {
    let text = validate_message(text)?;
    log::info!("User {}: {text}", ctx.sender());
    ctx.db.message().insert(Message {
        id: 0,
        sender: ctx.sender(),
        text,
        sent: ctx.timestamp,
    });
    Ok(())
}

#[spacetimedb::reducer]
pub fn delete_message(ctx: &ReducerContext, message_id: u64) -> Result<(), String> {
    if let Some(msg) = ctx.db.message().id().find(message_id) {
        if msg.sender != ctx.sender() {
            return Err("Cannot delete another user's message".to_string());
        }
        ctx.db.message().id().delete(message_id);
        log::info!("User {} deleted message {message_id}", ctx.sender());
        Ok(())
    } else {
        Err("Message not found".to_string())
    }
}

#[spacetimedb::reducer]
pub fn add_note(ctx: &ReducerContext, content: String, tag: String) -> Result<(), String> {
    if content.is_empty() {
        return Err("Note content must not be empty".to_string());
    }
    ctx.db.note().insert(Note {
        id: 0,
        owner: ctx.sender(),
        content,
        tag,
    });
    Ok(())
}

#[spacetimedb::reducer]
pub fn delete_note(ctx: &ReducerContext, note_id: u64) -> Result<(), String> {
    if let Some(note) = ctx.db.note().id().find(note_id) {
        if note.owner != ctx.sender() {
            return Err("Cannot delete another user's note".to_string());
        }
        ctx.db.note().id().delete(note_id);
        Ok(())
    } else {
        Err("Note not found".to_string())
    }
}

/// Schedule a one-shot reminder that fires after delay_ms milliseconds.
#[spacetimedb::reducer]
pub fn schedule_reminder(ctx: &ReducerContext, text: String, delay_ms: u64) -> Result<(), String> {
    if text.is_empty() {
        return Err("Reminder text must not be empty".to_string());
    }
    let at = ctx.timestamp + std::time::Duration::from_millis(delay_ms);
    ctx.db.reminder().insert(Reminder {
        scheduled_id: 0,
        scheduled_at: ScheduleAt::Time(at),
        text: text.clone(),
        owner: ctx.sender(),
    });
    log::info!("User {} scheduled reminder in {delay_ms}ms: {text}", ctx.sender());
    Ok(())
}

/// Schedule a repeating reminder that fires every interval_ms milliseconds.
#[spacetimedb::reducer]
pub fn schedule_reminder_repeat(ctx: &ReducerContext, text: String, interval_ms: u64) -> Result<(), String> {
    if text.is_empty() {
        return Err("Reminder text must not be empty".to_string());
    }
    let interval = std::time::Duration::from_millis(interval_ms);
    ctx.db.reminder().insert(Reminder {
        scheduled_id: 0,
        scheduled_at: interval.into(),
        text: text.clone(),
        owner: ctx.sender(),
    });
    log::info!("User {} scheduled repeating reminder every {interval_ms}ms: {text}", ctx.sender());
    Ok(())
}

/// Cancel a scheduled reminder by id.
#[spacetimedb::reducer]
pub fn cancel_reminder(ctx: &ReducerContext, reminder_id: u64) -> Result<(), String> {
    if let Some(reminder) = ctx.db.reminder().scheduled_id().find(reminder_id) {
        if reminder.owner != ctx.sender() {
            return Err("Cannot cancel another user's reminder".to_string());
        }
        ctx.db.reminder().scheduled_id().delete(reminder_id);
        log::info!("User {} cancelled reminder {reminder_id}", ctx.sender());
        Ok(())
    } else {
        Err("Reminder not found".to_string())
    }
}

/// Called by the scheduler when a reminder fires.
#[spacetimedb::reducer]
pub fn send_reminder(ctx: &ReducerContext, reminder: Reminder) {
    log::info!("Reminder fired for {}: {}", reminder.owner, reminder.text);
    // Insert a system message so the client sees it
    ctx.db.message().insert(Message {
        id: 0,
        sender: reminder.owner,
        text: format!("[REMINDER] {}", reminder.text),
        sent: ctx.timestamp,
    });
}

/// Simple procedure that echoes a greeting.
#[spacetimedb::procedure]
pub fn greet(_ctx: &mut ProcedureContext, name: String) -> String {
    format!("Hello, {name}!")
}

/// No-arg procedure that returns a constant.
#[spacetimedb::procedure]
pub fn server_ping(_ctx: &mut ProcedureContext) -> String {
    "pong".to_string()
}

#[spacetimedb::reducer(init)]
pub fn init(_ctx: &ReducerContext) {}

#[spacetimedb::reducer(client_connected)]
pub fn identity_connected(ctx: &ReducerContext) {
    if let Some(user) = ctx.db.user().identity().find(ctx.sender()) {
        ctx.db.user().identity().update(User { online: true, ..user });
    } else {
        ctx.db.user().insert(User {
            name: None,
            identity: ctx.sender(),
            online: true,
        });
    }
}

#[spacetimedb::reducer(client_disconnected)]
pub fn identity_disconnected(ctx: &ReducerContext) {
    if let Some(user) = ctx.db.user().identity().find(ctx.sender()) {
        ctx.db.user().identity().update(User { online: false, ..user });
    } else {
        log::warn!("Disconnect event for unknown user with identity {:?}", ctx.sender());
    }
}

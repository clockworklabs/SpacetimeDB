use spacetimedb::{table, reducer, ReducerContext, Identity, Timestamp, ScheduleAt, Table};

// ============================================================================
// TABLES
// ============================================================================

/// User table - stores display names and online status
#[table(name = user, public)]
pub struct User {
    #[primary_key]
    identity: Identity,
    name: Option<String>,
    online: bool,
}

/// Chat room
#[table(name = room, public)]
pub struct Room {
    #[primary_key]
    #[auto_inc]
    id: u64,
    name: String,
    owner: Identity,
    created_at: Timestamp,
}

/// Room membership (which users are in which rooms)
#[table(name = room_member, public)]
pub struct RoomMember {
    #[primary_key]
    #[auto_inc]
    id: u64,
    #[index(btree)]
    room_id: u64,
    #[index(btree)]
    user_identity: Identity,
    joined_at: Timestamp,
}

/// Chat messages
#[table(name = message, public)]
pub struct Message {
    #[primary_key]
    #[auto_inc]
    id: u64,
    #[index(btree)]
    room_id: u64,
    sender: Identity,
    text: String,
    sent_at: Timestamp,
    edited_at: Option<Timestamp>,
    /// If set, message will be deleted at this time
    disappear_at: Option<Timestamp>,
}

/// Message edit history
#[table(name = message_edit, public)]
pub struct MessageEdit {
    #[primary_key]
    #[auto_inc]
    id: u64,
    #[index(btree)]
    message_id: u64,
    previous_text: String,
    edited_at: Timestamp,
}

/// Typing indicators - ephemeral, cleaned up by scheduled job
#[table(name = typing_indicator, public)]
pub struct TypingIndicator {
    #[primary_key]
    #[auto_inc]
    id: u64,
    #[index(btree)]
    room_id: u64,
    user_identity: Identity,
    started_at: Timestamp,
}

/// Read receipts - tracks which users have seen which messages
#[table(name = read_receipt, public)]
pub struct ReadReceipt {
    #[primary_key]
    #[auto_inc]
    id: u64,
    #[index(btree)]
    message_id: u64,
    user_identity: Identity,
    read_at: Timestamp,
}

/// User room status - tracks last read message per user per room for unread counts
#[table(name = user_room_status, public)]
pub struct UserRoomStatus {
    #[primary_key]
    #[auto_inc]
    id: u64,
    #[index(btree)]
    user_identity: Identity,
    #[index(btree)]
    room_id: u64,
    last_read_message_id: u64,
}

/// Message reactions
#[table(name = message_reaction, public)]
pub struct MessageReaction {
    #[primary_key]
    #[auto_inc]
    id: u64,
    #[index(btree)]
    message_id: u64,
    user_identity: Identity,
    emoji: String,
    created_at: Timestamp,
}

/// Scheduled messages - queued for future delivery
#[table(name = scheduled_message, public)]
pub struct ScheduledMessage {
    #[primary_key]
    #[auto_inc]
    id: u64,
    room_id: u64,
    sender: Identity,
    text: String,
    scheduled_for: Timestamp,
    created_at: Timestamp,
}

// ============================================================================
// SCHEDULED TABLES (for automatic cleanup/delivery)
// ============================================================================

/// Scheduled job to send a message
#[table(name = send_scheduled_message_job, scheduled(deliver_scheduled_message))]
pub struct SendScheduledMessageJob {
    #[primary_key]
    #[auto_inc]
    scheduled_id: u64,
    scheduled_at: ScheduleAt,
    scheduled_message_id: u64,
}

/// Scheduled job to delete an ephemeral message
#[table(name = delete_message_job, scheduled(delete_ephemeral_message))]
pub struct DeleteMessageJob {
    #[primary_key]
    #[auto_inc]
    scheduled_id: u64,
    scheduled_at: ScheduleAt,
    message_id: u64,
}

/// Scheduled job to clean up expired typing indicators
#[table(name = typing_cleanup_job, scheduled(cleanup_typing_indicator))]
pub struct TypingCleanupJob {
    #[primary_key]
    #[auto_inc]
    scheduled_id: u64,
    scheduled_at: ScheduleAt,
    typing_indicator_id: u64,
}

// ============================================================================
// LIFECYCLE HOOKS
// ============================================================================

#[reducer(client_connected)]
pub fn client_connected(ctx: &ReducerContext) {
    if let Some(user) = ctx.db.user().identity().find(ctx.sender) {
        ctx.db.user().identity().update(User { online: true, ..user });
    } else {
        ctx.db.user().insert(User {
            identity: ctx.sender,
            name: None,
            online: true,
        });
    }
}

#[reducer(client_disconnected)]
pub fn client_disconnected(ctx: &ReducerContext) {
    if let Some(user) = ctx.db.user().identity().find(ctx.sender) {
        ctx.db.user().identity().update(User { online: false, ..user });
    }
    
    // Clean up typing indicators for this user
    let typing_ids: Vec<u64> = ctx.db.typing_indicator().iter()
        .filter(|t| t.user_identity == ctx.sender)
        .map(|t| t.id)
        .collect();
    
    for id in typing_ids {
        ctx.db.typing_indicator().id().delete(id);
    }
}

// ============================================================================
// USER REDUCERS
// ============================================================================

#[reducer]
pub fn set_name(ctx: &ReducerContext, name: String) -> Result<(), String> {
    if name.is_empty() {
        return Err("Name cannot be empty".to_string());
    }
    if name.len() > 50 {
        return Err("Name too long (max 50 chars)".to_string());
    }
    
    let user = ctx.db.user().identity().find(ctx.sender)
        .ok_or("User not found")?;
    
    ctx.db.user().identity().update(User {
        name: Some(name),
        ..user
    });
    
    Ok(())
}

// ============================================================================
// ROOM REDUCERS
// ============================================================================

#[reducer]
pub fn create_room(ctx: &ReducerContext, room_name: String) -> Result<(), String> {
    if room_name.is_empty() {
        return Err("Room name cannot be empty".to_string());
    }
    if room_name.len() > 100 {
        return Err("Room name too long (max 100 chars)".to_string());
    }
    
    let room = ctx.db.room().insert(Room {
        id: 0,
        name: room_name,
        owner: ctx.sender,
        created_at: ctx.timestamp,
    });
    
    // Auto-join the creator
    ctx.db.room_member().insert(RoomMember {
        id: 0,
        room_id: room.id,
        user_identity: ctx.sender,
        joined_at: ctx.timestamp,
    });
    
    // Initialize room status for the user
    ctx.db.user_room_status().insert(UserRoomStatus {
        id: 0,
        user_identity: ctx.sender,
        room_id: room.id,
        last_read_message_id: 0,
    });
    
    log::info!("Room {} created by {:?}", room.id, ctx.sender);
    Ok(())
}

#[reducer]
pub fn join_room(ctx: &ReducerContext, room_id: u64) -> Result<(), String> {
    // Verify room exists
    ctx.db.room().id().find(room_id)
        .ok_or("Room not found")?;
    
    // Check if already a member
    let already_member = ctx.db.room_member().room_id().filter(&room_id)
        .any(|m| m.user_identity == ctx.sender);
    
    if already_member {
        return Err("Already a member of this room".to_string());
    }
    
    ctx.db.room_member().insert(RoomMember {
        id: 0,
        room_id,
        user_identity: ctx.sender,
        joined_at: ctx.timestamp,
    });
    
    // Initialize room status for the user
    ctx.db.user_room_status().insert(UserRoomStatus {
        id: 0,
        user_identity: ctx.sender,
        room_id,
        last_read_message_id: 0,
    });
    
    log::info!("User {:?} joined room {}", ctx.sender, room_id);
    Ok(())
}

#[reducer]
pub fn leave_room(ctx: &ReducerContext, room_id: u64) -> Result<(), String> {
    let membership = ctx.db.room_member().room_id().filter(&room_id)
        .find(|m| m.user_identity == ctx.sender)
        .ok_or("Not a member of this room")?;
    
    ctx.db.room_member().id().delete(membership.id);
    
    // Clean up typing indicator
    let typing = ctx.db.typing_indicator().room_id().filter(&room_id)
        .find(|t| t.user_identity == ctx.sender);
    if let Some(t) = typing {
        ctx.db.typing_indicator().id().delete(t.id);
    }
    
    // Clean up user room status
    let status = ctx.db.user_room_status().room_id().filter(&room_id)
        .find(|s| s.user_identity == ctx.sender);
    if let Some(s) = status {
        ctx.db.user_room_status().id().delete(s.id);
    }
    
    log::info!("User {:?} left room {}", ctx.sender, room_id);
    Ok(())
}

// ============================================================================
// MESSAGE REDUCERS
// ============================================================================

#[reducer]
pub fn send_message(ctx: &ReducerContext, room_id: u64, text: String) -> Result<(), String> {
    if text.is_empty() {
        return Err("Message cannot be empty".to_string());
    }
    if text.len() > 2000 {
        return Err("Message too long (max 2000 chars)".to_string());
    }
    
    // Verify room exists
    ctx.db.room().id().find(room_id)
        .ok_or("Room not found")?;
    
    // Verify user is a member
    let is_member = ctx.db.room_member().room_id().filter(&room_id)
        .any(|m| m.user_identity == ctx.sender);
    
    if !is_member {
        return Err("Must be a member of the room to send messages".to_string());
    }
    
    ctx.db.message().insert(Message {
        id: 0,
        room_id,
        sender: ctx.sender,
        text,
        sent_at: ctx.timestamp,
        edited_at: None,
        disappear_at: None,
    });
    
    // Clear typing indicator
    let typing = ctx.db.typing_indicator().room_id().filter(&room_id)
        .find(|t| t.user_identity == ctx.sender);
    if let Some(t) = typing {
        ctx.db.typing_indicator().id().delete(t.id);
    }
    
    Ok(())
}

#[reducer]
pub fn send_ephemeral_message(ctx: &ReducerContext, room_id: u64, text: String, duration_secs: u64) -> Result<(), String> {
    if text.is_empty() {
        return Err("Message cannot be empty".to_string());
    }
    if text.len() > 2000 {
        return Err("Message too long (max 2000 chars)".to_string());
    }
    if duration_secs == 0 || duration_secs > 3600 {
        return Err("Duration must be between 1 and 3600 seconds".to_string());
    }
    
    // Verify room exists
    ctx.db.room().id().find(room_id)
        .ok_or("Room not found")?;
    
    // Verify user is a member
    let is_member = ctx.db.room_member().room_id().filter(&room_id)
        .any(|m| m.user_identity == ctx.sender);
    
    if !is_member {
        return Err("Must be a member of the room to send messages".to_string());
    }
    
    let disappear_time = ctx.timestamp + std::time::Duration::from_secs(duration_secs);
    
    let msg = ctx.db.message().insert(Message {
        id: 0,
        room_id,
        sender: ctx.sender,
        text,
        sent_at: ctx.timestamp,
        edited_at: None,
        disappear_at: Some(disappear_time),
    });
    
    // Schedule deletion
    ctx.db.delete_message_job().insert(DeleteMessageJob {
        scheduled_id: 0,
        scheduled_at: ScheduleAt::Time(disappear_time),
        message_id: msg.id,
    });
    
    Ok(())
}

#[reducer]
pub fn edit_message(ctx: &ReducerContext, message_id: u64, new_text: String) -> Result<(), String> {
    if new_text.is_empty() {
        return Err("Message cannot be empty".to_string());
    }
    if new_text.len() > 2000 {
        return Err("Message too long (max 2000 chars)".to_string());
    }
    
    let message = ctx.db.message().id().find(message_id)
        .ok_or("Message not found")?;
    
    if message.sender != ctx.sender {
        return Err("Can only edit your own messages".to_string());
    }
    
    // Save edit history
    ctx.db.message_edit().insert(MessageEdit {
        id: 0,
        message_id,
        previous_text: message.text.clone(),
        edited_at: ctx.timestamp,
    });
    
    // Update message
    ctx.db.message().id().update(Message {
        text: new_text,
        edited_at: Some(ctx.timestamp),
        ..message
    });
    
    Ok(())
}

// ============================================================================
// TYPING INDICATOR REDUCERS
// ============================================================================

const TYPING_EXPIRE_SECS: u64 = 5;

#[reducer]
pub fn start_typing(ctx: &ReducerContext, room_id: u64) -> Result<(), String> {
    // Verify room exists
    ctx.db.room().id().find(room_id)
        .ok_or("Room not found")?;
    
    // Verify user is a member
    let is_member = ctx.db.room_member().room_id().filter(&room_id)
        .any(|m| m.user_identity == ctx.sender);
    
    if !is_member {
        return Err("Must be a member of the room".to_string());
    }
    
    // Check if already typing
    let existing = ctx.db.typing_indicator().room_id().filter(&room_id)
        .find(|t| t.user_identity == ctx.sender);
    
    if let Some(t) = existing {
        // Update the timestamp
        ctx.db.typing_indicator().id().update(TypingIndicator {
            started_at: ctx.timestamp,
            ..t
        });
    } else {
        // Create new typing indicator
        let indicator = ctx.db.typing_indicator().insert(TypingIndicator {
            id: 0,
            room_id,
            user_identity: ctx.sender,
            started_at: ctx.timestamp,
        });
        
        // Schedule cleanup
        let expire_time = ctx.timestamp + std::time::Duration::from_secs(TYPING_EXPIRE_SECS);
        ctx.db.typing_cleanup_job().insert(TypingCleanupJob {
            scheduled_id: 0,
            scheduled_at: ScheduleAt::Time(expire_time),
            typing_indicator_id: indicator.id,
        });
    }
    
    Ok(())
}

#[reducer]
pub fn stop_typing(ctx: &ReducerContext, room_id: u64) -> Result<(), String> {
    let typing = ctx.db.typing_indicator().room_id().filter(&room_id)
        .find(|t| t.user_identity == ctx.sender);
    
    if let Some(t) = typing {
        ctx.db.typing_indicator().id().delete(t.id);
    }
    
    Ok(())
}

// ============================================================================
// READ RECEIPT REDUCERS
// ============================================================================

#[reducer]
pub fn mark_message_read(ctx: &ReducerContext, message_id: u64) -> Result<(), String> {
    let message = ctx.db.message().id().find(message_id)
        .ok_or("Message not found")?;
    
    // Verify user is a member of the room
    let is_member = ctx.db.room_member().room_id().filter(&message.room_id)
        .any(|m| m.user_identity == ctx.sender);
    
    if !is_member {
        return Err("Must be a member of the room".to_string());
    }
    
    // Check if already read
    let already_read = ctx.db.read_receipt().message_id().filter(&message_id)
        .any(|r| r.user_identity == ctx.sender);
    
    if !already_read {
        ctx.db.read_receipt().insert(ReadReceipt {
            id: 0,
            message_id,
            user_identity: ctx.sender,
            read_at: ctx.timestamp,
        });
    }
    
    // Update user room status
    let status = ctx.db.user_room_status().room_id().filter(&message.room_id)
        .find(|s| s.user_identity == ctx.sender);
    
    if let Some(s) = status {
        if message_id > s.last_read_message_id {
            ctx.db.user_room_status().id().update(UserRoomStatus {
                last_read_message_id: message_id,
                ..s
            });
        }
    }
    
    Ok(())
}

// ============================================================================
// SCHEDULED MESSAGE REDUCERS
// ============================================================================

#[reducer]
pub fn schedule_message(ctx: &ReducerContext, room_id: u64, text: String, delay_secs: u64) -> Result<(), String> {
    if text.is_empty() {
        return Err("Message cannot be empty".to_string());
    }
    if text.len() > 2000 {
        return Err("Message too long (max 2000 chars)".to_string());
    }
    if delay_secs == 0 || delay_secs > 86400 {
        return Err("Delay must be between 1 and 86400 seconds (24 hours)".to_string());
    }
    
    // Verify room exists
    ctx.db.room().id().find(room_id)
        .ok_or("Room not found")?;
    
    // Verify user is a member
    let is_member = ctx.db.room_member().room_id().filter(&room_id)
        .any(|m| m.user_identity == ctx.sender);
    
    if !is_member {
        return Err("Must be a member of the room to schedule messages".to_string());
    }
    
    let send_time = ctx.timestamp + std::time::Duration::from_secs(delay_secs);
    
    let scheduled = ctx.db.scheduled_message().insert(ScheduledMessage {
        id: 0,
        room_id,
        sender: ctx.sender,
        text,
        scheduled_for: send_time,
        created_at: ctx.timestamp,
    });
    
    // Schedule the delivery
    ctx.db.send_scheduled_message_job().insert(SendScheduledMessageJob {
        scheduled_id: 0,
        scheduled_at: ScheduleAt::Time(send_time),
        scheduled_message_id: scheduled.id,
    });
    
    log::info!("Message scheduled for delivery in {} seconds", delay_secs);
    Ok(())
}

#[reducer]
pub fn cancel_scheduled_message(ctx: &ReducerContext, scheduled_message_id: u64) -> Result<(), String> {
    let scheduled = ctx.db.scheduled_message().id().find(scheduled_message_id)
        .ok_or("Scheduled message not found")?;
    
    if scheduled.sender != ctx.sender {
        return Err("Can only cancel your own scheduled messages".to_string());
    }
    
    // Delete the scheduled message
    ctx.db.scheduled_message().id().delete(scheduled_message_id);
    
    // The scheduled job will handle the missing message gracefully
    
    log::info!("Scheduled message {} cancelled", scheduled_message_id);
    Ok(())
}

// ============================================================================
// REACTION REDUCERS
// ============================================================================

const ALLOWED_EMOJIS: [&str; 6] = ["👍", "❤️", "😂", "😮", "😢", "🎉"];

#[reducer]
pub fn toggle_reaction(ctx: &ReducerContext, message_id: u64, emoji: String) -> Result<(), String> {
    if !ALLOWED_EMOJIS.contains(&emoji.as_str()) {
        return Err(format!("Invalid emoji. Allowed: {:?}", ALLOWED_EMOJIS));
    }
    
    let message = ctx.db.message().id().find(message_id)
        .ok_or("Message not found")?;
    
    // Verify user is a member of the room
    let is_member = ctx.db.room_member().room_id().filter(&message.room_id)
        .any(|m| m.user_identity == ctx.sender);
    
    if !is_member {
        return Err("Must be a member of the room".to_string());
    }
    
    // Check if user already reacted with this emoji
    let existing = ctx.db.message_reaction().message_id().filter(&message_id)
        .find(|r| r.user_identity == ctx.sender && r.emoji == emoji);
    
    if let Some(r) = existing {
        // Remove reaction (toggle off)
        ctx.db.message_reaction().id().delete(r.id);
    } else {
        // Add reaction
        ctx.db.message_reaction().insert(MessageReaction {
            id: 0,
            message_id,
            user_identity: ctx.sender,
            emoji,
            created_at: ctx.timestamp,
        });
    }
    
    Ok(())
}

// ============================================================================
// SCHEDULED REDUCER HANDLERS
// ============================================================================

#[reducer]
pub fn deliver_scheduled_message(_ctx: &ReducerContext, job: SendScheduledMessageJob) {
    // Find the scheduled message
    if let Some(scheduled) = _ctx.db.scheduled_message().id().find(job.scheduled_message_id) {
        // Insert as a regular message
        _ctx.db.message().insert(Message {
            id: 0,
            room_id: scheduled.room_id,
            sender: scheduled.sender,
            text: scheduled.text,
            sent_at: _ctx.timestamp,
            edited_at: None,
            disappear_at: None,
        });
        
        // Delete the scheduled message record
        _ctx.db.scheduled_message().id().delete(job.scheduled_message_id);
        
        log::info!("Delivered scheduled message {}", job.scheduled_message_id);
    }
    // If not found, it was cancelled - do nothing
}

#[reducer]
pub fn delete_ephemeral_message(_ctx: &ReducerContext, job: DeleteMessageJob) {
    // Delete the message if it still exists
    if _ctx.db.message().id().find(job.message_id).is_some() {
        // Delete associated read receipts
        let receipt_ids: Vec<u64> = _ctx.db.read_receipt().message_id().filter(&job.message_id)
            .map(|r| r.id)
            .collect();
        for id in receipt_ids {
            _ctx.db.read_receipt().id().delete(id);
        }
        
        // Delete associated reactions
        let reaction_ids: Vec<u64> = _ctx.db.message_reaction().message_id().filter(&job.message_id)
            .map(|r| r.id)
            .collect();
        for id in reaction_ids {
            _ctx.db.message_reaction().id().delete(id);
        }
        
        // Delete edit history
        let edit_ids: Vec<u64> = _ctx.db.message_edit().message_id().filter(&job.message_id)
            .map(|e| e.id)
            .collect();
        for id in edit_ids {
            _ctx.db.message_edit().id().delete(id);
        }
        
        // Delete the message
        _ctx.db.message().id().delete(job.message_id);
        
        log::info!("Deleted ephemeral message {}", job.message_id);
    }
}

#[reducer]
pub fn cleanup_typing_indicator(_ctx: &ReducerContext, job: TypingCleanupJob) {
    // Only delete if the typing indicator still exists and hasn't been refreshed
    if let Some(indicator) = _ctx.db.typing_indicator().id().find(job.typing_indicator_id) {
        let expire_threshold = _ctx.timestamp - std::time::Duration::from_secs(TYPING_EXPIRE_SECS);
        if indicator.started_at <= expire_threshold {
            _ctx.db.typing_indicator().id().delete(job.typing_indicator_id);
        }
    }
}

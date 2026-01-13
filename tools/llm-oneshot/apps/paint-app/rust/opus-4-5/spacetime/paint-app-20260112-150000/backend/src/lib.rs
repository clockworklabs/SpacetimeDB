use spacetimedb::{table, reducer, ReducerContext, Identity, Timestamp, ScheduleAt, Table};
use serde::{Deserialize, Serialize};

// ============================================================================
// TABLES
// ============================================================================

/// User profile with presence information
#[table(name = user, public)]
pub struct User {
    #[primary_key]
    pub identity: Identity,
    pub name: String,
    pub avatar_color: String,
    pub status: String,           // "active", "idle", "away"
    pub current_canvas_id: Option<u64>,
    pub current_tool: String,     // "brush", "eraser", "select", "rectangle", "ellipse", "line", "arrow", "text", "sticky"
    pub selected_color: String,
    pub cursor_x: f64,
    pub cursor_y: f64,
    pub viewport_x: f64,
    pub viewport_y: f64,
    pub viewport_zoom: f64,
    pub following_user: Option<Identity>,
    pub last_active: Timestamp,
    pub created_at: Timestamp,
}

/// Canvas - the drawing board
#[table(name = canvas, public)]
pub struct Canvas {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    pub name: String,
    pub owner: Identity,
    pub is_public: bool,
    pub share_link: Option<String>,
    pub share_permission: String,  // "view", "edit"
    pub keep_forever: bool,
    pub last_active: Timestamp,
    pub created_at: Timestamp,
}

/// Canvas membership - who can access a canvas
#[table(name = canvas_member, public)]
pub struct CanvasMember {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    #[index(btree)]
    pub canvas_id: u64,
    pub user_identity: Identity,
    pub role: String,  // "owner", "editor", "viewer"
    pub joined_at: Timestamp,
}

/// Layer within a canvas
#[table(name = layer, public)]
pub struct Layer {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    #[index(btree)]
    pub canvas_id: u64,
    pub name: String,
    pub order_index: i32,
    pub visible: bool,
    pub opacity: f64,
    pub locked_by: Option<Identity>,
    pub locked_at: Option<Timestamp>,
}

/// Drawing element (shape, text, sticky note, etc.)
#[table(name = element, public)]
pub struct Element {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    #[index(btree)]
    pub canvas_id: u64,
    pub layer_id: u64,
    pub element_type: String,  // "rectangle", "ellipse", "line", "arrow", "text", "sticky", "image"
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub rotation: f64,
    pub stroke_color: String,
    pub fill_color: String,
    pub stroke_width: f64,
    pub text_content: Option<String>,
    pub font_size: String,  // "small", "medium", "large"
    pub points_json: Option<String>,  // For line/arrow: JSON array of points
    pub created_by: Identity,
    pub created_at: Timestamp,
}

/// Freehand stroke
#[table(name = stroke, public)]
pub struct Stroke {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    #[index(btree)]
    pub canvas_id: u64,
    pub layer_id: u64,
    pub points_json: String,  // JSON array of {x, y} points
    pub color: String,
    pub size: f64,
    pub tool: String,  // "brush", "eraser"
    pub created_by: Identity,
    pub created_at: Timestamp,
}

/// User's current selection on a canvas
#[table(name = user_selection, public)]
pub struct UserSelection {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    pub user_identity: Identity,
    #[index(btree)]
    pub canvas_id: u64,
    pub element_ids_json: String,  // JSON array of element IDs
    pub updated_at: Timestamp,
}

/// Comment pin on canvas
#[table(name = comment, public)]
pub struct Comment {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    #[index(btree)]
    pub canvas_id: u64,
    pub parent_id: Option<u64>,  // For threaded replies
    pub x: f64,
    pub y: f64,
    pub text: String,
    pub author: Identity,
    pub resolved: bool,
    pub created_at: Timestamp,
}

/// Canvas version/snapshot
#[table(name = version, public)]
pub struct Version {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    #[index(btree)]
    pub canvas_id: u64,
    pub name: Option<String>,
    pub description: Option<String>,
    pub snapshot_json: String,  // Full canvas state as JSON
    pub created_by: Option<Identity>,
    pub is_auto_save: bool,
    pub created_at: Timestamp,
}

/// Canvas chat message
#[table(name = chat_message, public)]
pub struct ChatMessage {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    #[index(btree)]
    pub canvas_id: u64,
    pub sender: Identity,
    pub text: String,
    pub created_at: Timestamp,
}

/// Typing indicator for chat
#[table(name = typing_indicator, public)]
pub struct TypingIndicator {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    #[index(btree)]
    pub canvas_id: u64,
    pub user_identity: Identity,
    pub is_typing: bool,
    pub updated_at: Timestamp,
}

/// Activity feed entry
#[table(name = activity_entry, public)]
pub struct ActivityEntry {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    #[index(btree)]
    pub canvas_id: u64,
    pub user_identity: Identity,
    pub action: String,  // "joined", "left", "added_shape", "erased", "added_comment", etc.
    pub description: String,
    pub location_x: Option<f64>,
    pub location_y: Option<f64>,
    pub created_at: Timestamp,
}

/// User notification
#[table(name = notification, public)]
pub struct Notification {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    pub user_identity: Identity,
    pub message: String,
    pub canvas_id: Option<u64>,
    pub read: bool,
    pub created_at: Timestamp,
}

/// Scheduled canvas cleanup
#[table(name = scheduled_cleanup, scheduled(run_cleanup))]
pub struct ScheduledCleanup {
    #[primary_key]
    #[auto_inc]
    pub scheduled_id: u64,
    pub scheduled_at: ScheduleAt,
    pub canvas_id: u64,
}

/// Scheduled auto-save
#[table(name = scheduled_auto_save, scheduled(run_auto_save))]
pub struct ScheduledAutoSave {
    #[primary_key]
    #[auto_inc]
    pub scheduled_id: u64,
    pub scheduled_at: ScheduleAt,
    pub canvas_id: u64,
}

/// Scheduled inactivity check
#[table(name = scheduled_inactivity, scheduled(run_inactivity_check))]
pub struct ScheduledInactivity {
    #[primary_key]
    #[auto_inc]
    pub scheduled_id: u64,
    pub scheduled_at: ScheduleAt,
    pub user_identity: Identity,
}

/// Scheduled layer unlock
#[table(name = scheduled_layer_unlock, scheduled(run_layer_unlock))]
pub struct ScheduledLayerUnlock {
    #[primary_key]
    #[auto_inc]
    pub scheduled_id: u64,
    pub scheduled_at: ScheduleAt,
    pub layer_id: u64,
}

// ============================================================================
// LIFECYCLE HOOKS
// ============================================================================

#[reducer(client_connected)]
pub fn client_connected(ctx: &ReducerContext) {
    log::info!("Client connected: {:?}", ctx.sender);
    
    // Check if user exists
    if ctx.db.user().identity().find(ctx.sender).is_none() {
        // Create new user with defaults
        ctx.db.user().insert(User {
            identity: ctx.sender,
            name: format!("User-{}", &ctx.sender.to_hex()[..6]),
            avatar_color: "#4cf490".to_string(),
            status: "active".to_string(),
            current_canvas_id: None,
            current_tool: "brush".to_string(),
            selected_color: "#4cf490".to_string(),
            cursor_x: 0.0,
            cursor_y: 0.0,
            viewport_x: 0.0,
            viewport_y: 0.0,
            viewport_zoom: 1.0,
            following_user: None,
            last_active: ctx.timestamp,
            created_at: ctx.timestamp,
        });
    } else {
        // Update existing user's status
        if let Some(user) = ctx.db.user().identity().find(ctx.sender) {
            ctx.db.user().identity().update(User {
                status: "active".to_string(),
                last_active: ctx.timestamp,
                ..user
            });
        }
    }
    
    // Schedule inactivity check in 2 minutes
    let future_time = ctx.timestamp + std::time::Duration::from_secs(120);
    ctx.db.scheduled_inactivity().insert(ScheduledInactivity {
        scheduled_id: 0,
        scheduled_at: ScheduleAt::Time(future_time),
        user_identity: ctx.sender,
    });
}

#[reducer(client_disconnected)]
pub fn client_disconnected(ctx: &ReducerContext) {
    log::info!("Client disconnected: {:?}", ctx.sender);
    
    if let Some(user) = ctx.db.user().identity().find(ctx.sender) {
        // Leave current canvas if any
        if let Some(canvas_id) = user.current_canvas_id {
            leave_canvas_internal(ctx, canvas_id);
        }
        
        // Update status to away
        ctx.db.user().identity().update(User {
            status: "away".to_string(),
            current_canvas_id: None,
            ..user
        });
    }
    
    // Unlock any layers locked by this user
    for layer in ctx.db.layer().iter() {
        if layer.locked_by == Some(ctx.sender) {
            ctx.db.layer().id().update(Layer {
                locked_by: None,
                locked_at: None,
                ..layer
            });
        }
    }
}

// ============================================================================
// USER REDUCERS
// ============================================================================

#[reducer]
pub fn set_name(ctx: &ReducerContext, name: String) -> Result<(), String> {
    if name.is_empty() || name.len() > 50 {
        return Err("Name must be 1-50 characters".to_string());
    }
    
    let user = ctx.db.user().identity().find(ctx.sender)
        .ok_or("User not found")?;
    
    ctx.db.user().identity().update(User {
        name,
        last_active: ctx.timestamp,
        ..user
    });
    
    Ok(())
}

#[reducer]
pub fn set_avatar_color(ctx: &ReducerContext, color: String) -> Result<(), String> {
    let user = ctx.db.user().identity().find(ctx.sender)
        .ok_or("User not found")?;
    
    ctx.db.user().identity().update(User {
        avatar_color: color,
        last_active: ctx.timestamp,
        ..user
    });
    
    Ok(())
}

#[reducer]
pub fn set_tool(ctx: &ReducerContext, tool: String) -> Result<(), String> {
    let valid_tools = ["brush", "eraser", "select", "rectangle", "ellipse", "line", "arrow", "text", "sticky"];
    if !valid_tools.contains(&tool.as_str()) {
        return Err("Invalid tool".to_string());
    }
    
    let user = ctx.db.user().identity().find(ctx.sender)
        .ok_or("User not found")?;
    
    ctx.db.user().identity().update(User {
        current_tool: tool,
        status: "active".to_string(),
        last_active: ctx.timestamp,
        ..user
    });
    
    Ok(())
}

#[reducer]
pub fn set_selected_color(ctx: &ReducerContext, color: String) -> Result<(), String> {
    let user = ctx.db.user().identity().find(ctx.sender)
        .ok_or("User not found")?;
    
    ctx.db.user().identity().update(User {
        selected_color: color,
        last_active: ctx.timestamp,
        ..user
    });
    
    Ok(())
}

#[reducer]
pub fn update_cursor(ctx: &ReducerContext, x: f64, y: f64) -> Result<(), String> {
    let user = ctx.db.user().identity().find(ctx.sender)
        .ok_or("User not found")?;
    
    ctx.db.user().identity().update(User {
        cursor_x: x,
        cursor_y: y,
        status: "active".to_string(),
        last_active: ctx.timestamp,
        ..user
    });
    
    Ok(())
}

#[reducer]
pub fn update_viewport(ctx: &ReducerContext, x: f64, y: f64, zoom: f64) -> Result<(), String> {
    let user = ctx.db.user().identity().find(ctx.sender)
        .ok_or("User not found")?;
    
    // If user was following someone and manually changed viewport, stop following
    let following = if user.following_user.is_some() { None } else { user.following_user };
    
    ctx.db.user().identity().update(User {
        viewport_x: x,
        viewport_y: y,
        viewport_zoom: zoom,
        following_user: following,
        status: "active".to_string(),
        last_active: ctx.timestamp,
        ..user
    });
    
    Ok(())
}

#[reducer]
pub fn set_status(ctx: &ReducerContext, status: String) -> Result<(), String> {
    let valid_statuses = ["active", "idle", "away"];
    if !valid_statuses.contains(&status.as_str()) {
        return Err("Invalid status".to_string());
    }
    
    let user = ctx.db.user().identity().find(ctx.sender)
        .ok_or("User not found")?;
    
    ctx.db.user().identity().update(User {
        status,
        last_active: ctx.timestamp,
        ..user
    });
    
    Ok(())
}

#[reducer]
pub fn follow_user(ctx: &ReducerContext, target_identity_hex: String) -> Result<(), String> {
    let target_bytes = hex::decode(&target_identity_hex)
        .map_err(|_| "Invalid identity hex")?;
    let target_identity = Identity::from_byte_array(target_bytes.try_into().map_err(|_| "Invalid identity length")?);
    
    // Verify target user exists
    ctx.db.user().identity().find(target_identity)
        .ok_or("Target user not found")?;
    
    let user = ctx.db.user().identity().find(ctx.sender)
        .ok_or("User not found")?;
    
    ctx.db.user().identity().update(User {
        following_user: Some(target_identity),
        last_active: ctx.timestamp,
        ..user
    });
    
    Ok(())
}

#[reducer]
pub fn unfollow_user(ctx: &ReducerContext) -> Result<(), String> {
    let user = ctx.db.user().identity().find(ctx.sender)
        .ok_or("User not found")?;
    
    ctx.db.user().identity().update(User {
        following_user: None,
        last_active: ctx.timestamp,
        ..user
    });
    
    Ok(())
}

// ============================================================================
// CANVAS REDUCERS
// ============================================================================

#[reducer]
pub fn create_canvas(ctx: &ReducerContext, name: String) -> Result<(), String> {
    if name.is_empty() || name.len() > 100 {
        return Err("Canvas name must be 1-100 characters".to_string());
    }
    
    // Create the canvas
    let canvas = ctx.db.canvas().insert(Canvas {
        id: 0,
        name: name.clone(),
        owner: ctx.sender,
        is_public: false,
        share_link: None,
        share_permission: "view".to_string(),
        keep_forever: false,
        last_active: ctx.timestamp,
        created_at: ctx.timestamp,
    });
    
    // Add owner as member
    ctx.db.canvas_member().insert(CanvasMember {
        id: 0,
        canvas_id: canvas.id,
        user_identity: ctx.sender,
        role: "owner".to_string(),
        joined_at: ctx.timestamp,
    });
    
    // Create default layer
    ctx.db.layer().insert(Layer {
        id: 0,
        canvas_id: canvas.id,
        name: "Layer 1".to_string(),
        order_index: 0,
        visible: true,
        opacity: 1.0,
        locked_by: None,
        locked_at: None,
    });
    
    // Schedule auto-cleanup in 30 days
    let cleanup_time = ctx.timestamp + std::time::Duration::from_secs(30 * 24 * 60 * 60);
    ctx.db.scheduled_cleanup().insert(ScheduledCleanup {
        scheduled_id: 0,
        scheduled_at: ScheduleAt::Time(cleanup_time),
        canvas_id: canvas.id,
    });
    
    // Log activity
    log_activity_internal(ctx, canvas.id, "created", &format!("Created canvas '{}'", name), None, None);
    
    // Update user's current canvas
    if let Some(user) = ctx.db.user().identity().find(ctx.sender) {
        ctx.db.user().identity().update(User {
            current_canvas_id: Some(canvas.id),
            last_active: ctx.timestamp,
            ..user
        });
    }
    
    Ok(())
}

#[reducer]
pub fn delete_canvas(ctx: &ReducerContext, canvas_id: u64) -> Result<(), String> {
    let canvas = ctx.db.canvas().id().find(canvas_id)
        .ok_or("Canvas not found")?;
    
    if canvas.owner != ctx.sender {
        return Err("Only the owner can delete the canvas".to_string());
    }
    
    // Delete all related data
    for member in ctx.db.canvas_member().canvas_id().filter(canvas_id) {
        ctx.db.canvas_member().id().delete(member.id);
    }
    for layer in ctx.db.layer().canvas_id().filter(canvas_id) {
        ctx.db.layer().id().delete(layer.id);
    }
    for element in ctx.db.element().canvas_id().filter(canvas_id) {
        ctx.db.element().id().delete(element.id);
    }
    for stroke in ctx.db.stroke().canvas_id().filter(canvas_id) {
        ctx.db.stroke().id().delete(stroke.id);
    }
    for comment in ctx.db.comment().canvas_id().filter(canvas_id) {
        ctx.db.comment().id().delete(comment.id);
    }
    for version in ctx.db.version().canvas_id().filter(canvas_id) {
        ctx.db.version().id().delete(version.id);
    }
    for msg in ctx.db.chat_message().canvas_id().filter(canvas_id) {
        ctx.db.chat_message().id().delete(msg.id);
    }
    for activity in ctx.db.activity_entry().canvas_id().filter(canvas_id) {
        ctx.db.activity_entry().id().delete(activity.id);
    }
    
    // Delete the canvas
    ctx.db.canvas().id().delete(canvas_id);
    
    Ok(())
}

#[reducer]
pub fn join_canvas(ctx: &ReducerContext, canvas_id: u64) -> Result<(), String> {
    let canvas = ctx.db.canvas().id().find(canvas_id)
        .ok_or("Canvas not found")?;
    
    // Check if user is already a member
    let is_member = ctx.db.canvas_member().canvas_id().filter(canvas_id)
        .any(|m| m.user_identity == ctx.sender);
    
    if !is_member && !canvas.is_public && canvas.share_link.is_none() {
        return Err("Canvas is private".to_string());
    }
    
    // Add as member if not already
    if !is_member {
        let role = if canvas.share_permission == "edit" { "editor" } else { "viewer" };
        ctx.db.canvas_member().insert(CanvasMember {
            id: 0,
            canvas_id,
            user_identity: ctx.sender,
            role: role.to_string(),
            joined_at: ctx.timestamp,
        });
    }
    
    // Leave previous canvas
    if let Some(user) = ctx.db.user().identity().find(ctx.sender) {
        if let Some(prev_canvas_id) = user.current_canvas_id {
            if prev_canvas_id != canvas_id {
                leave_canvas_internal(ctx, prev_canvas_id);
            }
        }
        
        ctx.db.user().identity().update(User {
            current_canvas_id: Some(canvas_id),
            status: "active".to_string(),
            last_active: ctx.timestamp,
            ..user
        });
    }
    
    // Log activity
    if let Some(user) = ctx.db.user().identity().find(ctx.sender) {
        log_activity_internal(ctx, canvas_id, "joined", &format!("{} joined", user.name), None, None);
    }
    
    // Update canvas last active
    ctx.db.canvas().id().update(Canvas {
        last_active: ctx.timestamp,
        ..canvas
    });
    
    // Schedule auto-save in 5 minutes
    let save_time = ctx.timestamp + std::time::Duration::from_secs(300);
    ctx.db.scheduled_auto_save().insert(ScheduledAutoSave {
        scheduled_id: 0,
        scheduled_at: ScheduleAt::Time(save_time),
        canvas_id,
    });
    
    Ok(())
}

fn leave_canvas_internal(ctx: &ReducerContext, canvas_id: u64) {
    // Log activity
    if let Some(user) = ctx.db.user().identity().find(ctx.sender) {
        log_activity_internal(ctx, canvas_id, "left", &format!("{} left", user.name), None, None);
    }
    
    // Clear selection
    for sel in ctx.db.user_selection().canvas_id().filter(canvas_id) {
        if sel.user_identity == ctx.sender {
            ctx.db.user_selection().id().delete(sel.id);
        }
    }
    
    // Unlock any layers
    for layer in ctx.db.layer().canvas_id().filter(canvas_id) {
        if layer.locked_by == Some(ctx.sender) {
            ctx.db.layer().id().update(Layer {
                locked_by: None,
                locked_at: None,
                ..layer
            });
        }
    }
}

#[reducer]
pub fn leave_canvas(ctx: &ReducerContext) -> Result<(), String> {
    let user = ctx.db.user().identity().find(ctx.sender)
        .ok_or("User not found")?;
    
    if let Some(canvas_id) = user.current_canvas_id {
        leave_canvas_internal(ctx, canvas_id);
    }
    
    ctx.db.user().identity().update(User {
        current_canvas_id: None,
        last_active: ctx.timestamp,
        ..user
    });
    
    Ok(())
}

#[reducer]
pub fn join_canvas_by_link(ctx: &ReducerContext, share_link: String) -> Result<(), String> {
    // Find canvas with this share link
    let canvas = ctx.db.canvas().iter()
        .find(|c| c.share_link.as_ref() == Some(&share_link))
        .ok_or("Invalid share link")?;
    
    join_canvas(ctx, canvas.id)
}

#[reducer]
pub fn generate_share_link(ctx: &ReducerContext, canvas_id: u64) -> Result<(), String> {
    let canvas = ctx.db.canvas().id().find(canvas_id)
        .ok_or("Canvas not found")?;
    
    if canvas.owner != ctx.sender {
        return Err("Only the owner can generate share links".to_string());
    }
    
    // Generate a random-ish share link
    let link = format!("share-{}-{}", canvas_id, ctx.timestamp.to_duration_since_unix_epoch().unwrap_or_default().as_micros() % 1000000);
    
    ctx.db.canvas().id().update(Canvas {
        share_link: Some(link),
        ..canvas
    });
    
    Ok(())
}

#[reducer]
pub fn revoke_share_link(ctx: &ReducerContext, canvas_id: u64) -> Result<(), String> {
    let canvas = ctx.db.canvas().id().find(canvas_id)
        .ok_or("Canvas not found")?;
    
    if canvas.owner != ctx.sender {
        return Err("Only the owner can revoke share links".to_string());
    }
    
    ctx.db.canvas().id().update(Canvas {
        share_link: None,
        ..canvas
    });
    
    Ok(())
}

#[reducer]
pub fn set_share_permission(ctx: &ReducerContext, canvas_id: u64, permission: String) -> Result<(), String> {
    if permission != "view" && permission != "edit" {
        return Err("Permission must be 'view' or 'edit'".to_string());
    }
    
    let canvas = ctx.db.canvas().id().find(canvas_id)
        .ok_or("Canvas not found")?;
    
    if canvas.owner != ctx.sender {
        return Err("Only the owner can change permissions".to_string());
    }
    
    ctx.db.canvas().id().update(Canvas {
        share_permission: permission,
        ..canvas
    });
    
    Ok(())
}

#[reducer]
pub fn set_keep_forever(ctx: &ReducerContext, canvas_id: u64, keep: bool) -> Result<(), String> {
    let canvas = ctx.db.canvas().id().find(canvas_id)
        .ok_or("Canvas not found")?;
    
    if canvas.owner != ctx.sender {
        return Err("Only the owner can change this setting".to_string());
    }
    
    ctx.db.canvas().id().update(Canvas {
        keep_forever: keep,
        ..canvas
    });
    
    Ok(())
}

#[reducer]
pub fn set_member_role(ctx: &ReducerContext, canvas_id: u64, target_identity_hex: String, role: String) -> Result<(), String> {
    if role != "viewer" && role != "editor" {
        return Err("Role must be 'viewer' or 'editor'".to_string());
    }
    
    let canvas = ctx.db.canvas().id().find(canvas_id)
        .ok_or("Canvas not found")?;
    
    if canvas.owner != ctx.sender {
        return Err("Only the owner can change roles".to_string());
    }
    
    let target_bytes = hex::decode(&target_identity_hex)
        .map_err(|_| "Invalid identity hex")?;
    let target_identity = Identity::from_byte_array(target_bytes.try_into().map_err(|_| "Invalid identity length")?);
    
    // Find and update member
    for member in ctx.db.canvas_member().canvas_id().filter(canvas_id) {
        if member.user_identity == target_identity && member.role != "owner" {
            ctx.db.canvas_member().id().update(CanvasMember {
                role,
                ..member
            });
            return Ok(());
        }
    }
    
    Err("Member not found".to_string())
}

#[reducer]
pub fn remove_member(ctx: &ReducerContext, canvas_id: u64, target_identity_hex: String) -> Result<(), String> {
    let canvas = ctx.db.canvas().id().find(canvas_id)
        .ok_or("Canvas not found")?;
    
    if canvas.owner != ctx.sender {
        return Err("Only the owner can remove members".to_string());
    }
    
    let target_bytes = hex::decode(&target_identity_hex)
        .map_err(|_| "Invalid identity hex")?;
    let target_identity = Identity::from_byte_array(target_bytes.try_into().map_err(|_| "Invalid identity length")?);
    
    if target_identity == ctx.sender {
        return Err("Cannot remove yourself".to_string());
    }
    
    // Find and remove member
    for member in ctx.db.canvas_member().canvas_id().filter(canvas_id) {
        if member.user_identity == target_identity {
            ctx.db.canvas_member().id().delete(member.id);
            
            // Force them to leave canvas
            if let Some(target_user) = ctx.db.user().identity().find(target_identity) {
                if target_user.current_canvas_id == Some(canvas_id) {
                    ctx.db.user().identity().update(User {
                        current_canvas_id: None,
                        ..target_user
                    });
                }
            }
            
            return Ok(());
        }
    }
    
    Err("Member not found".to_string())
}

#[reducer]
pub fn invite_user(ctx: &ReducerContext, canvas_id: u64, username: String, role: String) -> Result<(), String> {
    if role != "viewer" && role != "editor" {
        return Err("Role must be 'viewer' or 'editor'".to_string());
    }
    
    let canvas = ctx.db.canvas().id().find(canvas_id)
        .ok_or("Canvas not found")?;
    
    if canvas.owner != ctx.sender {
        return Err("Only the owner can invite users".to_string());
    }
    
    // Find user by name
    let target_user = ctx.db.user().iter()
        .find(|u| u.name == username)
        .ok_or("User not found")?;
    
    // Check if already a member
    let is_member = ctx.db.canvas_member().canvas_id().filter(canvas_id)
        .any(|m| m.user_identity == target_user.identity);
    
    if is_member {
        return Err("User is already a member".to_string());
    }
    
    // Add as member
    ctx.db.canvas_member().insert(CanvasMember {
        id: 0,
        canvas_id,
        user_identity: target_user.identity,
        role,
        joined_at: ctx.timestamp,
    });
    
    // Send notification
    ctx.db.notification().insert(Notification {
        id: 0,
        user_identity: target_user.identity,
        message: format!("You were invited to canvas '{}'", canvas.name),
        canvas_id: Some(canvas_id),
        read: false,
        created_at: ctx.timestamp,
    });
    
    Ok(())
}

// ============================================================================
// LAYER REDUCERS
// ============================================================================

#[reducer]
pub fn create_layer(ctx: &ReducerContext, canvas_id: u64, name: String) -> Result<(), String> {
    check_editor_permission(ctx, canvas_id)?;
    
    // Get max order index
    let max_order = ctx.db.layer().canvas_id().filter(canvas_id)
        .map(|l| l.order_index)
        .max()
        .unwrap_or(-1);
    
    ctx.db.layer().insert(Layer {
        id: 0,
        canvas_id,
        name,
        order_index: max_order + 1,
        visible: true,
        opacity: 1.0,
        locked_by: None,
        locked_at: None,
    });
    
    update_canvas_activity(ctx, canvas_id);
    Ok(())
}

#[reducer]
pub fn rename_layer(ctx: &ReducerContext, layer_id: u64, name: String) -> Result<(), String> {
    let layer = ctx.db.layer().id().find(layer_id)
        .ok_or("Layer not found")?;
    
    check_editor_permission(ctx, layer.canvas_id)?;
    
    ctx.db.layer().id().update(Layer {
        name,
        ..layer
    });
    
    update_canvas_activity(ctx, layer.canvas_id);
    Ok(())
}

#[reducer]
pub fn reorder_layer(ctx: &ReducerContext, layer_id: u64, new_order: i32) -> Result<(), String> {
    let layer = ctx.db.layer().id().find(layer_id)
        .ok_or("Layer not found")?;
    
    check_editor_permission(ctx, layer.canvas_id)?;
    
    ctx.db.layer().id().update(Layer {
        order_index: new_order,
        ..layer
    });
    
    update_canvas_activity(ctx, layer.canvas_id);
    Ok(())
}

#[reducer]
pub fn toggle_layer_visibility(ctx: &ReducerContext, layer_id: u64) -> Result<(), String> {
    let layer = ctx.db.layer().id().find(layer_id)
        .ok_or("Layer not found")?;
    
    check_editor_permission(ctx, layer.canvas_id)?;
    
    ctx.db.layer().id().update(Layer {
        visible: !layer.visible,
        ..layer
    });
    
    update_canvas_activity(ctx, layer.canvas_id);
    Ok(())
}

#[reducer]
pub fn set_layer_opacity(ctx: &ReducerContext, layer_id: u64, opacity: f64) -> Result<(), String> {
    let layer = ctx.db.layer().id().find(layer_id)
        .ok_or("Layer not found")?;
    
    check_editor_permission(ctx, layer.canvas_id)?;
    
    let opacity = opacity.clamp(0.0, 1.0);
    
    ctx.db.layer().id().update(Layer {
        opacity,
        ..layer
    });
    
    update_canvas_activity(ctx, layer.canvas_id);
    Ok(())
}

#[reducer]
pub fn lock_layer(ctx: &ReducerContext, layer_id: u64) -> Result<(), String> {
    let layer = ctx.db.layer().id().find(layer_id)
        .ok_or("Layer not found")?;
    
    check_editor_permission(ctx, layer.canvas_id)?;
    
    if layer.locked_by.is_some() && layer.locked_by != Some(ctx.sender) {
        return Err("Layer is already locked by another user".to_string());
    }
    
    ctx.db.layer().id().update(Layer {
        locked_by: Some(ctx.sender),
        locked_at: Some(ctx.timestamp),
        ..layer
    });
    
    // Schedule auto-unlock in 5 minutes
    let unlock_time = ctx.timestamp + std::time::Duration::from_secs(300);
    ctx.db.scheduled_layer_unlock().insert(ScheduledLayerUnlock {
        scheduled_id: 0,
        scheduled_at: ScheduleAt::Time(unlock_time),
        layer_id,
    });
    
    update_canvas_activity(ctx, layer.canvas_id);
    Ok(())
}

#[reducer]
pub fn unlock_layer(ctx: &ReducerContext, layer_id: u64) -> Result<(), String> {
    let layer = ctx.db.layer().id().find(layer_id)
        .ok_or("Layer not found")?;
    
    if layer.locked_by != Some(ctx.sender) {
        return Err("You don't have this layer locked".to_string());
    }
    
    ctx.db.layer().id().update(Layer {
        locked_by: None,
        locked_at: None,
        ..layer
    });
    
    update_canvas_activity(ctx, layer.canvas_id);
    Ok(())
}

// ============================================================================
// DRAWING REDUCERS
// ============================================================================

#[reducer]
pub fn add_stroke(ctx: &ReducerContext, canvas_id: u64, layer_id: u64, points_json: String, color: String, size: f64, tool: String) -> Result<(), String> {
    check_editor_permission(ctx, canvas_id)?;
    check_layer_editable(ctx, layer_id)?;

    let is_eraser = tool == "eraser";
    
    ctx.db.stroke().insert(Stroke {
        id: 0,
        canvas_id,
        layer_id,
        points_json,
        color,
        size,
        tool,
        created_by: ctx.sender,
        created_at: ctx.timestamp,
    });

    if let Some(user) = ctx.db.user().identity().find(ctx.sender) {
        let action = if is_eraser { "erased" } else { "drew" };
        log_activity_internal(ctx, canvas_id, action, &format!("{} {}", user.name, action), None, None);
    }
    
    update_canvas_activity(ctx, canvas_id);
    Ok(())
}

#[reducer]
pub fn add_element(ctx: &ReducerContext, canvas_id: u64, layer_id: u64, element_type: String, x: f64, y: f64, width: f64, height: f64, stroke_color: String, fill_color: String, stroke_width: f64, text_content: Option<String>, font_size: String, points_json: Option<String>) -> Result<(), String> {
    check_editor_permission(ctx, canvas_id)?;
    check_layer_editable(ctx, layer_id)?;
    
    let element = ctx.db.element().insert(Element {
        id: 0,
        canvas_id,
        layer_id,
        element_type: element_type.clone(),
        x,
        y,
        width,
        height,
        rotation: 0.0,
        stroke_color,
        fill_color,
        stroke_width,
        text_content,
        font_size,
        points_json,
        created_by: ctx.sender,
        created_at: ctx.timestamp,
    });
    
    if let Some(user) = ctx.db.user().identity().find(ctx.sender) {
        log_activity_internal(ctx, canvas_id, "added", &format!("{} added a {}", user.name, element_type), Some(x), Some(y));
    }
    
    update_canvas_activity(ctx, canvas_id);
    Ok(())
}

#[reducer]
pub fn update_element(ctx: &ReducerContext, element_id: u64, x: f64, y: f64, width: f64, height: f64, rotation: f64, stroke_color: String, fill_color: String, stroke_width: f64, text_content: Option<String>) -> Result<(), String> {
    let element = ctx.db.element().id().find(element_id)
        .ok_or("Element not found")?;
    
    check_editor_permission(ctx, element.canvas_id)?;
    check_layer_editable(ctx, element.layer_id)?;
    
    ctx.db.element().id().update(Element {
        x,
        y,
        width,
        height,
        rotation,
        stroke_color,
        fill_color,
        stroke_width,
        text_content,
        ..element
    });
    
    update_canvas_activity(ctx, element.canvas_id);
    Ok(())
}

#[reducer]
pub fn delete_element(ctx: &ReducerContext, element_id: u64) -> Result<(), String> {
    let element = ctx.db.element().id().find(element_id)
        .ok_or("Element not found")?;
    
    check_editor_permission(ctx, element.canvas_id)?;
    check_layer_editable(ctx, element.layer_id)?;
    
    ctx.db.element().id().delete(element_id);
    
    update_canvas_activity(ctx, element.canvas_id);
    Ok(())
}

#[reducer]
pub fn select_elements(ctx: &ReducerContext, canvas_id: u64, element_ids_json: String) -> Result<(), String> {
    // Clear existing selection for this user on this canvas
    for sel in ctx.db.user_selection().canvas_id().filter(canvas_id) {
        if sel.user_identity == ctx.sender {
            ctx.db.user_selection().id().delete(sel.id);
        }
    }
    
    // Create new selection
    ctx.db.user_selection().insert(UserSelection {
        id: 0,
        user_identity: ctx.sender,
        canvas_id,
        element_ids_json,
        updated_at: ctx.timestamp,
    });
    
    if let Some(user) = ctx.db.user().identity().find(ctx.sender) {
        ctx.db.user().identity().update(User {
            last_active: ctx.timestamp,
            status: "active".to_string(),
            ..user
        });
    }
    
    Ok(())
}

#[reducer]
pub fn deselect_all(ctx: &ReducerContext, canvas_id: u64) -> Result<(), String> {
    for sel in ctx.db.user_selection().canvas_id().filter(canvas_id) {
        if sel.user_identity == ctx.sender {
            ctx.db.user_selection().id().delete(sel.id);
        }
    }
    
    Ok(())
}

#[reducer]
pub fn clear_canvas(ctx: &ReducerContext, canvas_id: u64) -> Result<(), String> {
    let canvas = ctx.db.canvas().id().find(canvas_id)
        .ok_or("Canvas not found")?;
    
    if canvas.owner != ctx.sender {
        return Err("Only the owner can clear the canvas".to_string());
    }
    
    // Save a version before clearing
    save_version_internal(ctx, canvas_id, Some("Before clear".to_string()), None, false)?;
    
    // Delete all strokes and elements
    for stroke in ctx.db.stroke().canvas_id().filter(canvas_id) {
        ctx.db.stroke().id().delete(stroke.id);
    }
    for element in ctx.db.element().canvas_id().filter(canvas_id) {
        ctx.db.element().id().delete(element.id);
    }
    
    if let Some(user) = ctx.db.user().identity().find(ctx.sender) {
        log_activity_internal(ctx, canvas_id, "cleared", &format!("{} cleared the canvas", user.name), None, None);
    }
    
    update_canvas_activity(ctx, canvas_id);
    Ok(())
}

// ============================================================================
// COMMENT REDUCERS
// ============================================================================

#[reducer]
pub fn add_comment(ctx: &ReducerContext, canvas_id: u64, x: f64, y: f64, text: String) -> Result<(), String> {
    if text.is_empty() || text.len() > 1000 {
        return Err("Comment must be 1-1000 characters".to_string());
    }
    
    // Any member can comment
    let is_member = ctx.db.canvas_member().canvas_id().filter(canvas_id)
        .any(|m| m.user_identity == ctx.sender);
    
    if !is_member {
        return Err("You must be a member to comment".to_string());
    }
    
    ctx.db.comment().insert(Comment {
        id: 0,
        canvas_id,
        parent_id: None,
        x,
        y,
        text,
        author: ctx.sender,
        resolved: false,
        created_at: ctx.timestamp,
    });
    
    if let Some(user) = ctx.db.user().identity().find(ctx.sender) {
        log_activity_internal(ctx, canvas_id, "commented", &format!("{} added a comment", user.name), Some(x), Some(y));
    }
    
    update_canvas_activity(ctx, canvas_id);
    Ok(())
}

#[reducer]
pub fn reply_to_comment(ctx: &ReducerContext, parent_id: u64, text: String) -> Result<(), String> {
    if text.is_empty() || text.len() > 1000 {
        return Err("Reply must be 1-1000 characters".to_string());
    }
    
    let parent = ctx.db.comment().id().find(parent_id)
        .ok_or("Parent comment not found")?;
    
    // Any member can reply
    let is_member = ctx.db.canvas_member().canvas_id().filter(parent.canvas_id)
        .any(|m| m.user_identity == ctx.sender);
    
    if !is_member {
        return Err("You must be a member to reply".to_string());
    }
    
    ctx.db.comment().insert(Comment {
        id: 0,
        canvas_id: parent.canvas_id,
        parent_id: Some(parent_id),
        x: parent.x,
        y: parent.y,
        text,
        author: ctx.sender,
        resolved: false,
        created_at: ctx.timestamp,
    });
    
    update_canvas_activity(ctx, parent.canvas_id);
    Ok(())
}

#[reducer]
pub fn resolve_comment(ctx: &ReducerContext, comment_id: u64) -> Result<(), String> {
    let comment = ctx.db.comment().id().find(comment_id)
        .ok_or("Comment not found")?;
    
    // Author or canvas owner can resolve
    let canvas = ctx.db.canvas().id().find(comment.canvas_id)
        .ok_or("Canvas not found")?;
    
    if comment.author != ctx.sender && canvas.owner != ctx.sender {
        return Err("Only the author or canvas owner can resolve comments".to_string());
    }
    
    ctx.db.comment().id().update(Comment {
        resolved: true,
        ..comment
    });
    
    update_canvas_activity(ctx, comment.canvas_id);
    Ok(())
}

// ============================================================================
// VERSION REDUCERS
// ============================================================================

fn save_version_internal(ctx: &ReducerContext, canvas_id: u64, name: Option<String>, description: Option<String>, is_auto: bool) -> Result<(), String> {
    // Build snapshot JSON
    let elements: Vec<_> = ctx.db.element().canvas_id().filter(canvas_id).collect();
    let strokes: Vec<_> = ctx.db.stroke().canvas_id().filter(canvas_id).collect();
    let layers: Vec<_> = ctx.db.layer().canvas_id().filter(canvas_id).collect();
    
    #[derive(Serialize)]
    struct Snapshot {
        elements: Vec<ElementData>,
        strokes: Vec<StrokeData>,
        layers: Vec<LayerData>,
    }
    
    #[derive(Serialize)]
    struct ElementData {
        id: u64,
        layer_id: u64,
        element_type: String,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        rotation: f64,
        stroke_color: String,
        fill_color: String,
        stroke_width: f64,
        text_content: Option<String>,
        font_size: String,
        points_json: Option<String>,
    }
    
    #[derive(Serialize)]
    struct StrokeData {
        id: u64,
        layer_id: u64,
        points_json: String,
        color: String,
        size: f64,
        tool: String,
    }
    
    #[derive(Serialize)]
    struct LayerData {
        id: u64,
        name: String,
        order_index: i32,
        visible: bool,
        opacity: f64,
    }
    
    let snapshot = Snapshot {
        elements: elements.iter().map(|e| ElementData {
            id: e.id,
            layer_id: e.layer_id,
            element_type: e.element_type.clone(),
            x: e.x,
            y: e.y,
            width: e.width,
            height: e.height,
            rotation: e.rotation,
            stroke_color: e.stroke_color.clone(),
            fill_color: e.fill_color.clone(),
            stroke_width: e.stroke_width,
            text_content: e.text_content.clone(),
            font_size: e.font_size.clone(),
            points_json: e.points_json.clone(),
        }).collect(),
        strokes: strokes.iter().map(|s| StrokeData {
            id: s.id,
            layer_id: s.layer_id,
            points_json: s.points_json.clone(),
            color: s.color.clone(),
            size: s.size,
            tool: s.tool.clone(),
        }).collect(),
        layers: layers.iter().map(|l| LayerData {
            id: l.id,
            name: l.name.clone(),
            order_index: l.order_index,
            visible: l.visible,
            opacity: l.opacity,
        }).collect(),
    };
    
    let snapshot_json = serde_json::to_string(&snapshot)
        .map_err(|e| format!("Failed to serialize snapshot: {}", e))?;
    
    ctx.db.version().insert(Version {
        id: 0,
        canvas_id,
        name,
        description,
        snapshot_json,
        created_by: if is_auto { None } else { Some(ctx.sender) },
        is_auto_save: is_auto,
        created_at: ctx.timestamp,
    });
    
    Ok(())
}

#[reducer]
pub fn save_version(ctx: &ReducerContext, canvas_id: u64, name: Option<String>, description: Option<String>) -> Result<(), String> {
    check_editor_permission(ctx, canvas_id)?;
    
    save_version_internal(ctx, canvas_id, name, description, false)?;
    
    if let Some(user) = ctx.db.user().identity().find(ctx.sender) {
        log_activity_internal(ctx, canvas_id, "saved_version", &format!("{} saved a version", user.name), None, None);
    }
    
    Ok(())
}

#[reducer]
pub fn restore_version(ctx: &ReducerContext, version_id: u64) -> Result<(), String> {
    let version = ctx.db.version().id().find(version_id)
        .ok_or("Version not found")?;
    
    check_editor_permission(ctx, version.canvas_id)?;
    
    // Save current state first
    save_version_internal(ctx, version.canvas_id, Some("Before restore".to_string()), None, false)?;
    
    // Parse snapshot
    #[derive(Deserialize)]
    struct Snapshot {
        elements: Vec<ElementData>,
        strokes: Vec<StrokeData>,
        layers: Vec<LayerData>,
    }
    
    #[derive(Deserialize)]
    struct ElementData {
        layer_id: u64,
        element_type: String,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        rotation: f64,
        stroke_color: String,
        fill_color: String,
        stroke_width: f64,
        text_content: Option<String>,
        font_size: String,
        points_json: Option<String>,
    }
    
    #[derive(Deserialize)]
    struct StrokeData {
        layer_id: u64,
        points_json: String,
        color: String,
        size: f64,
        tool: String,
    }
    
    #[derive(Deserialize)]
    struct LayerData {
        name: String,
        order_index: i32,
        visible: bool,
        opacity: f64,
    }
    
    let snapshot: Snapshot = serde_json::from_str(&version.snapshot_json)
        .map_err(|e| format!("Failed to parse snapshot: {}", e))?;
    
    // Clear current content
    for stroke in ctx.db.stroke().canvas_id().filter(version.canvas_id) {
        ctx.db.stroke().id().delete(stroke.id);
    }
    for element in ctx.db.element().canvas_id().filter(version.canvas_id) {
        ctx.db.element().id().delete(element.id);
    }
    for layer in ctx.db.layer().canvas_id().filter(version.canvas_id) {
        ctx.db.layer().id().delete(layer.id);
    }
    
    // Restore layers
    let mut layer_id_map = std::collections::HashMap::new();
    for layer_data in snapshot.layers {
        let layer = ctx.db.layer().insert(Layer {
            id: 0,
            canvas_id: version.canvas_id,
            name: layer_data.name,
            order_index: layer_data.order_index,
            visible: layer_data.visible,
            opacity: layer_data.opacity,
            locked_by: None,
            locked_at: None,
        });
        layer_id_map.insert(layer_data.order_index, layer.id);
    }
    
    // Use first layer as default if mapping fails
    let default_layer_id = ctx.db.layer().canvas_id().filter(version.canvas_id)
        .next()
        .map(|l| l.id)
        .unwrap_or(0);
    
    // Restore strokes
    for stroke_data in snapshot.strokes {
        let layer_id = layer_id_map.values().next().copied().unwrap_or(default_layer_id);
        ctx.db.stroke().insert(Stroke {
            id: 0,
            canvas_id: version.canvas_id,
            layer_id,
            points_json: stroke_data.points_json,
            color: stroke_data.color,
            size: stroke_data.size,
            tool: stroke_data.tool,
            created_by: ctx.sender,
            created_at: ctx.timestamp,
        });
    }
    
    // Restore elements
    for elem_data in snapshot.elements {
        let layer_id = layer_id_map.values().next().copied().unwrap_or(default_layer_id);
        ctx.db.element().insert(Element {
            id: 0,
            canvas_id: version.canvas_id,
            layer_id,
            element_type: elem_data.element_type,
            x: elem_data.x,
            y: elem_data.y,
            width: elem_data.width,
            height: elem_data.height,
            rotation: elem_data.rotation,
            stroke_color: elem_data.stroke_color,
            fill_color: elem_data.fill_color,
            stroke_width: elem_data.stroke_width,
            text_content: elem_data.text_content,
            font_size: elem_data.font_size,
            points_json: elem_data.points_json,
            created_by: ctx.sender,
            created_at: ctx.timestamp,
        });
    }
    
    // Save restored version marker
    save_version_internal(ctx, version.canvas_id, Some(format!("Restored from version {}", version_id)), None, false)?;
    
    if let Some(user) = ctx.db.user().identity().find(ctx.sender) {
        log_activity_internal(ctx, version.canvas_id, "restored", &format!("{} restored a version", user.name), None, None);
    }
    
    Ok(())
}

// ============================================================================
// CHAT REDUCERS
// ============================================================================

#[reducer]
pub fn send_chat_message(ctx: &ReducerContext, canvas_id: u64, text: String) -> Result<(), String> {
    if text.is_empty() || text.len() > 1000 {
        return Err("Message must be 1-1000 characters".to_string());
    }
    
    let is_member = ctx.db.canvas_member().canvas_id().filter(canvas_id)
        .any(|m| m.user_identity == ctx.sender);
    
    if !is_member {
        return Err("You must be a member to chat".to_string());
    }
    
    ctx.db.chat_message().insert(ChatMessage {
        id: 0,
        canvas_id,
        sender: ctx.sender,
        text,
        created_at: ctx.timestamp,
    });
    
    // Clear typing indicator
    for indicator in ctx.db.typing_indicator().canvas_id().filter(canvas_id) {
        if indicator.user_identity == ctx.sender {
            ctx.db.typing_indicator().id().update(TypingIndicator {
                is_typing: false,
                updated_at: ctx.timestamp,
                ..indicator
            });
        }
    }
    
    Ok(())
}

#[reducer]
pub fn set_typing(ctx: &ReducerContext, canvas_id: u64, is_typing: bool) -> Result<(), String> {
    // Find existing indicator
    let existing = ctx.db.typing_indicator().canvas_id().filter(canvas_id)
        .find(|t| t.user_identity == ctx.sender);
    
    if let Some(indicator) = existing {
        ctx.db.typing_indicator().id().update(TypingIndicator {
            is_typing,
            updated_at: ctx.timestamp,
            ..indicator
        });
    } else {
        ctx.db.typing_indicator().insert(TypingIndicator {
            id: 0,
            canvas_id,
            user_identity: ctx.sender,
            is_typing,
            updated_at: ctx.timestamp,
        });
    }
    
    Ok(())
}

// ============================================================================
// NOTIFICATION REDUCERS
// ============================================================================

#[reducer]
pub fn mark_notification_read(ctx: &ReducerContext, notification_id: u64) -> Result<(), String> {
    let notification = ctx.db.notification().id().find(notification_id)
        .ok_or("Notification not found")?;
    
    if notification.user_identity != ctx.sender {
        return Err("Not your notification".to_string());
    }
    
    ctx.db.notification().id().update(Notification {
        read: true,
        ..notification
    });
    
    Ok(())
}

#[reducer]
pub fn mark_all_notifications_read(ctx: &ReducerContext) -> Result<(), String> {
    for notification in ctx.db.notification().iter() {
        if notification.user_identity == ctx.sender && !notification.read {
            ctx.db.notification().id().update(Notification {
                read: true,
                ..notification
            });
        }
    }
    
    Ok(())
}

// ============================================================================
// SCHEDULED REDUCERS
// ============================================================================

#[reducer]
pub fn run_cleanup(ctx: &ReducerContext, job: ScheduledCleanup) {
    log::info!("Running cleanup for canvas {}", job.canvas_id);
    
    if let Some(canvas) = ctx.db.canvas().id().find(job.canvas_id) {
        if canvas.keep_forever {
            return;
        }
        
        // Check if canvas was active in last 30 days
        let thirty_days_ago = ctx.timestamp - std::time::Duration::from_secs(30 * 24 * 60 * 60);
        
        if canvas.last_active < thirty_days_ago {
            // Check if we already sent a 7-day warning
            let seven_days_ago = ctx.timestamp - std::time::Duration::from_secs(7 * 24 * 60 * 60);
            
            // Send warning notifications to all members
            for member in ctx.db.canvas_member().canvas_id().filter(job.canvas_id) {
                ctx.db.notification().insert(Notification {
                    id: 0,
                    user_identity: member.user_identity,
                    message: format!("Canvas '{}' will be deleted due to inactivity", canvas.name),
                    canvas_id: Some(job.canvas_id),
                    read: false,
                    created_at: ctx.timestamp,
                });
            }
            
            // Schedule actual deletion in 7 days
            let delete_time = ctx.timestamp + std::time::Duration::from_secs(7 * 24 * 60 * 60);
            ctx.db.scheduled_cleanup().insert(ScheduledCleanup {
                scheduled_id: 0,
                scheduled_at: ScheduleAt::Time(delete_time),
                canvas_id: job.canvas_id,
            });
        } else {
            // Reschedule check for 30 days from last activity
            let next_check = canvas.last_active + std::time::Duration::from_secs(30 * 24 * 60 * 60);
            ctx.db.scheduled_cleanup().insert(ScheduledCleanup {
                scheduled_id: 0,
                scheduled_at: ScheduleAt::Time(next_check),
                canvas_id: job.canvas_id,
            });
        }
    }
}

#[reducer]
pub fn run_auto_save(ctx: &ReducerContext, job: ScheduledAutoSave) {
    log::info!("Running auto-save for canvas {}", job.canvas_id);
    
    if let Some(canvas) = ctx.db.canvas().id().find(job.canvas_id) {
        // Check if canvas has active users
        let has_active_users = ctx.db.user().iter()
            .any(|u| u.current_canvas_id == Some(job.canvas_id) && u.status == "active");
        
        if has_active_users {
            // Save version
            let _ = save_version_internal(ctx, job.canvas_id, None, None, true);
            
            // Schedule next auto-save in 5 minutes
            let next_save = ctx.timestamp + std::time::Duration::from_secs(300);
            ctx.db.scheduled_auto_save().insert(ScheduledAutoSave {
                scheduled_id: 0,
                scheduled_at: ScheduleAt::Time(next_save),
                canvas_id: job.canvas_id,
            });
        }
    }
}

#[reducer]
pub fn run_inactivity_check(ctx: &ReducerContext, job: ScheduledInactivity) {
    if let Some(user) = ctx.db.user().identity().find(job.user_identity) {
        let two_minutes_ago = ctx.timestamp - std::time::Duration::from_secs(120);
        
        if user.last_active < two_minutes_ago && user.status == "active" {
            ctx.db.user().identity().update(User {
                status: "away".to_string(),
                ..user
            });
        }
        
        // Reschedule check
        let next_check = ctx.timestamp + std::time::Duration::from_secs(120);
        ctx.db.scheduled_inactivity().insert(ScheduledInactivity {
            scheduled_id: 0,
            scheduled_at: ScheduleAt::Time(next_check),
            user_identity: job.user_identity,
        });
    }
}

#[reducer]
pub fn run_layer_unlock(ctx: &ReducerContext, job: ScheduledLayerUnlock) {
    if let Some(layer) = ctx.db.layer().id().find(job.layer_id) {
        if layer.locked_at.is_some() {
            let five_minutes_ago = ctx.timestamp - std::time::Duration::from_secs(300);
            
            if let Some(locked_at) = layer.locked_at {
                if locked_at < five_minutes_ago {
                    ctx.db.layer().id().update(Layer {
                        locked_by: None,
                        locked_at: None,
                        ..layer
                    });
                }
            }
        }
    }
}

// ============================================================================
// HELPER FUNCTIONS
// ============================================================================

fn check_editor_permission(ctx: &ReducerContext, canvas_id: u64) -> Result<(), String> {
    let member = ctx.db.canvas_member().canvas_id().filter(canvas_id)
        .find(|m| m.user_identity == ctx.sender)
        .ok_or("Not a member of this canvas")?;
    
    if member.role == "viewer" {
        return Err("Viewers cannot edit".to_string());
    }
    
    Ok(())
}

fn check_layer_editable(ctx: &ReducerContext, layer_id: u64) -> Result<(), String> {
    let layer = ctx.db.layer().id().find(layer_id)
        .ok_or("Layer not found")?;
    
    if let Some(locked_by) = layer.locked_by {
        if locked_by != ctx.sender {
            return Err("Layer is locked by another user".to_string());
        }
    }
    
    Ok(())
}

fn update_canvas_activity(ctx: &ReducerContext, canvas_id: u64) {
    if let Some(canvas) = ctx.db.canvas().id().find(canvas_id) {
        ctx.db.canvas().id().update(Canvas {
            last_active: ctx.timestamp,
            ..canvas
        });
    }
    
    if let Some(user) = ctx.db.user().identity().find(ctx.sender) {
        ctx.db.user().identity().update(User {
            last_active: ctx.timestamp,
            status: "active".to_string(),
            ..user
        });
    }
}

fn log_activity_internal(ctx: &ReducerContext, canvas_id: u64, action: &str, description: &str, x: Option<f64>, y: Option<f64>) {
    ctx.db.activity_entry().insert(ActivityEntry {
        id: 0,
        canvas_id,
        user_identity: ctx.sender,
        action: action.to_string(),
        description: description.to_string(),
        location_x: x,
        location_y: y,
        created_at: ctx.timestamp,
    });
    
    // Keep only last 100 entries per canvas
    let entries: Vec<_> = ctx.db.activity_entry().canvas_id().filter(canvas_id).collect();
    if entries.len() > 100 {
        let mut sorted = entries;
        sorted.sort_by_key(|e| e.created_at);
        for entry in sorted.iter().take(sorted.len() - 100) {
            ctx.db.activity_entry().id().delete(entry.id);
        }
    }
}

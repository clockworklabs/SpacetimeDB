use axum::{
    extract::State,
    http::StatusCode,
    response::{Html, IntoResponse, Json},
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use tower_http::cors::CorsLayer;

mod module_bindings;
use module_bindings::*;
use spacetimedb_sdk::{DbContext, Identity, Table};

// ============================================================================
// STATE
// ============================================================================

struct AppState {
    conn: DbConnection,
}

type SharedState = Arc<Mutex<AppState>>;

// ============================================================================
// API TYPES
// ============================================================================

#[derive(Serialize)]
struct UserInfo {
    identity: String,
    name: Option<String>,
    online: bool,
}

#[derive(Serialize)]
struct RoomInfo {
    id: u64,
    name: String,
    member_count: usize,
    unread_count: usize,
}

#[derive(Serialize)]
struct MessageInfo {
    id: u64,
    sender_name: String,
    sender_identity: String,
    text: String,
    sent_at: String,
    edited: bool,
    ephemeral: bool,
    reactions: Vec<ReactionInfo>,
    read_by: Vec<String>,
}

#[derive(Serialize)]
struct ReactionInfo {
    emoji: String,
    count: usize,
}

#[derive(Serialize)]
struct TypingInfo {
    users: Vec<String>,
}

#[derive(Serialize)]
struct ScheduledInfo {
    id: u64,
    text: String,
    room_id: u64,
}

#[derive(Deserialize)]
struct SetNameRequest {
    name: String,
}

#[derive(Deserialize)]
struct CreateRoomRequest {
    name: String,
}

#[derive(Deserialize)]
struct JoinRoomRequest {
    room_id: u64,
}

#[derive(Deserialize)]
struct SendMessageRequest {
    room_id: u64,
    text: String,
}

#[derive(Deserialize)]
struct EditMessageRequest {
    message_id: u64,
    text: String,
}

#[derive(Deserialize)]
struct EphemeralRequest {
    room_id: u64,
    text: String,
    duration_secs: u64,
}

#[derive(Deserialize)]
struct ScheduleRequest {
    room_id: u64,
    text: String,
    delay_secs: u64,
}

#[derive(Deserialize)]
struct ReactRequest {
    message_id: u64,
    emoji: String,
}

#[derive(Deserialize)]
struct MarkReadRequest {
    message_id: u64,
}

#[derive(Deserialize)]
struct TypingRequest {
    room_id: u64,
}

#[derive(Deserialize)]
struct CancelScheduledRequest {
    id: u64,
}

#[derive(Serialize)]
struct ApiResponse {
    success: bool,
    message: String,
}

// ============================================================================
// HANDLERS
// ============================================================================

async fn index() -> Html<&'static str> {
    Html(include_str!("index.html"))
}

async fn get_identity(State(state): State<SharedState>) -> impl IntoResponse {
    let state = state.lock().unwrap();
    if let Some(identity) = state.conn.try_identity() {
        let name = state.conn.db.user().identity().find(&identity)
            .and_then(|u| u.name.clone());
        Json(serde_json::json!({
            "identity": format!("{:?}", identity),
            "name": name
        }))
    } else {
        Json(serde_json::json!({
            "identity": null,
            "name": null
        }))
    }
}

async fn get_rooms(State(state): State<SharedState>) -> impl IntoResponse {
    let state = state.lock().unwrap();
    let identity = state.conn.try_identity();
    
    let rooms: Vec<RoomInfo> = state.conn.db.room().iter().map(|room| {
        let member_count = state.conn.db.room_member().iter()
            .filter(|m| m.room_id == room.id)
            .count();
        
        let unread_count = if let Some(id) = identity {
            let status = state.conn.db.user_room_status().iter()
                .find(|s| s.user_identity == id && s.room_id == room.id);
            let last_read = status.map(|s| s.last_read_message_id).unwrap_or(0);
            state.conn.db.message().iter()
                .filter(|m| m.room_id == room.id && m.id > last_read)
                .count()
        } else {
            0
        };
        
        RoomInfo {
            id: room.id,
            name: room.name.clone(),
            member_count,
            unread_count,
        }
    }).collect();
    
    Json(rooms)
}

async fn get_messages(
    State(state): State<SharedState>,
    axum::extract::Path(room_id): axum::extract::Path<u64>,
) -> impl IntoResponse {
    let state = state.lock().unwrap();
    
    let messages: Vec<MessageInfo> = state.conn.db.message().iter()
        .filter(|m| m.room_id == room_id)
        .map(|msg| {
            let sender_name = state.conn.db.user().identity().find(&msg.sender)
                .and_then(|u| u.name.clone())
                .unwrap_or_else(|| format!("{:?}", msg.sender).chars().take(8).collect());
            
            let mut reaction_counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
            for r in state.conn.db.message_reaction().iter().filter(|r| r.message_id == msg.id) {
                *reaction_counts.entry(r.emoji.clone()).or_insert(0) += 1;
            }
            
            let reactions: Vec<ReactionInfo> = reaction_counts.into_iter()
                .map(|(emoji, count)| ReactionInfo { emoji, count })
                .collect();
            
            let read_by: Vec<String> = state.conn.db.read_receipt().iter()
                .filter(|r| r.message_id == msg.id)
                .filter_map(|r| {
                    state.conn.db.user().identity().find(&r.user_identity)
                        .and_then(|u| u.name.clone())
                })
                .collect();
            
            let micros = msg.sent_at.to_micros_since_unix_epoch();
            let secs = micros / 1_000_000;
            let datetime = chrono::DateTime::from_timestamp(secs as i64, 0)
                .unwrap_or_else(|| chrono::DateTime::UNIX_EPOCH);
            
            MessageInfo {
                id: msg.id,
                sender_name,
                sender_identity: format!("{:?}", msg.sender),
                text: msg.text.clone(),
                sent_at: datetime.format("%H:%M").to_string(),
                edited: msg.edited_at.is_some(),
                ephemeral: msg.disappear_at.is_some(),
                reactions,
                read_by,
            }
        })
        .collect();
    
    Json(messages)
}

async fn get_typing(
    State(state): State<SharedState>,
    axum::extract::Path(room_id): axum::extract::Path<u64>,
) -> impl IntoResponse {
    let state = state.lock().unwrap();
    let identity = state.conn.try_identity();
    
    let users: Vec<String> = state.conn.db.typing_indicator().iter()
        .filter(|t| t.room_id == room_id)
        .filter(|t| identity.map(|id| id != t.user_identity).unwrap_or(true))
        .filter_map(|t| {
            state.conn.db.user().identity().find(&t.user_identity)
                .and_then(|u| u.name.clone())
        })
        .collect();
    
    Json(TypingInfo { users })
}

async fn get_scheduled(State(state): State<SharedState>) -> impl IntoResponse {
    let state = state.lock().unwrap();
    let identity = state.conn.try_identity();
    
    let scheduled: Vec<ScheduledInfo> = if let Some(id) = identity {
        state.conn.db.scheduled_message().iter()
            .filter(|m| m.sender == id)
            .map(|m| ScheduledInfo {
                id: m.id,
                text: m.text.clone(),
                room_id: m.room_id,
            })
            .collect()
    } else {
        vec![]
    };
    
    Json(scheduled)
}

async fn get_edit_history(
    State(state): State<SharedState>,
    axum::extract::Path(message_id): axum::extract::Path<u64>,
) -> impl IntoResponse {
    let state = state.lock().unwrap();
    
    let edits: Vec<serde_json::Value> = state.conn.db.message_edit().iter()
        .filter(|e| e.message_id == message_id)
        .map(|e| {
            let micros = e.edited_at.to_micros_since_unix_epoch();
            let secs = micros / 1_000_000;
            let datetime = chrono::DateTime::from_timestamp(secs as i64, 0)
                .unwrap_or_else(|| chrono::DateTime::UNIX_EPOCH);
            serde_json::json!({
                "text": e.previous_text,
                "time": datetime.format("%H:%M").to_string()
            })
        })
        .collect();
    
    Json(edits)
}

async fn set_name(
    State(state): State<SharedState>,
    Json(req): Json<SetNameRequest>,
) -> impl IntoResponse {
    let state = state.lock().unwrap();
    match state.conn.reducers.set_name(req.name) {
        Ok(_) => Json(ApiResponse { success: true, message: "Name set".to_string() }),
        Err(e) => Json(ApiResponse { success: false, message: format!("{:?}", e) }),
    }
}

async fn create_room(
    State(state): State<SharedState>,
    Json(req): Json<CreateRoomRequest>,
) -> impl IntoResponse {
    let state = state.lock().unwrap();
    match state.conn.reducers.create_room(req.name) {
        Ok(_) => Json(ApiResponse { success: true, message: "Room created".to_string() }),
        Err(e) => Json(ApiResponse { success: false, message: format!("{:?}", e) }),
    }
}

async fn join_room(
    State(state): State<SharedState>,
    Json(req): Json<JoinRoomRequest>,
) -> impl IntoResponse {
    let state = state.lock().unwrap();
    match state.conn.reducers.join_room(req.room_id) {
        Ok(_) => Json(ApiResponse { success: true, message: "Joined room".to_string() }),
        Err(e) => Json(ApiResponse { success: false, message: format!("{:?}", e) }),
    }
}

async fn leave_room(
    State(state): State<SharedState>,
    Json(req): Json<JoinRoomRequest>,
) -> impl IntoResponse {
    let state = state.lock().unwrap();
    match state.conn.reducers.leave_room(req.room_id) {
        Ok(_) => Json(ApiResponse { success: true, message: "Left room".to_string() }),
        Err(e) => Json(ApiResponse { success: false, message: format!("{:?}", e) }),
    }
}

async fn send_message(
    State(state): State<SharedState>,
    Json(req): Json<SendMessageRequest>,
) -> impl IntoResponse {
    let state = state.lock().unwrap();
    let _ = state.conn.reducers.stop_typing(req.room_id);
    match state.conn.reducers.send_message(req.room_id, req.text) {
        Ok(_) => Json(ApiResponse { success: true, message: "Message sent".to_string() }),
        Err(e) => Json(ApiResponse { success: false, message: format!("{:?}", e) }),
    }
}

async fn edit_message(
    State(state): State<SharedState>,
    Json(req): Json<EditMessageRequest>,
) -> impl IntoResponse {
    let state = state.lock().unwrap();
    match state.conn.reducers.edit_message(req.message_id, req.text) {
        Ok(_) => Json(ApiResponse { success: true, message: "Message edited".to_string() }),
        Err(e) => Json(ApiResponse { success: false, message: format!("{:?}", e) }),
    }
}

async fn send_ephemeral(
    State(state): State<SharedState>,
    Json(req): Json<EphemeralRequest>,
) -> impl IntoResponse {
    let state = state.lock().unwrap();
    match state.conn.reducers.send_ephemeral_message(req.room_id, req.text, req.duration_secs) {
        Ok(_) => Json(ApiResponse { success: true, message: "Ephemeral message sent".to_string() }),
        Err(e) => Json(ApiResponse { success: false, message: format!("{:?}", e) }),
    }
}

async fn schedule_message(
    State(state): State<SharedState>,
    Json(req): Json<ScheduleRequest>,
) -> impl IntoResponse {
    let state = state.lock().unwrap();
    match state.conn.reducers.schedule_message(req.room_id, req.text, req.delay_secs) {
        Ok(_) => Json(ApiResponse { success: true, message: "Message scheduled".to_string() }),
        Err(e) => Json(ApiResponse { success: false, message: format!("{:?}", e) }),
    }
}

async fn cancel_scheduled(
    State(state): State<SharedState>,
    Json(req): Json<CancelScheduledRequest>,
) -> impl IntoResponse {
    let state = state.lock().unwrap();
    match state.conn.reducers.cancel_scheduled_message(req.id) {
        Ok(_) => Json(ApiResponse { success: true, message: "Cancelled".to_string() }),
        Err(e) => Json(ApiResponse { success: false, message: format!("{:?}", e) }),
    }
}

async fn react(
    State(state): State<SharedState>,
    Json(req): Json<ReactRequest>,
) -> impl IntoResponse {
    let state = state.lock().unwrap();
    match state.conn.reducers.toggle_reaction(req.message_id, req.emoji) {
        Ok(_) => Json(ApiResponse { success: true, message: "Reaction toggled".to_string() }),
        Err(e) => Json(ApiResponse { success: false, message: format!("{:?}", e) }),
    }
}

async fn mark_read(
    State(state): State<SharedState>,
    Json(req): Json<MarkReadRequest>,
) -> impl IntoResponse {
    let state = state.lock().unwrap();
    match state.conn.reducers.mark_message_read(req.message_id) {
        Ok(_) => Json(ApiResponse { success: true, message: "Marked as read".to_string() }),
        Err(e) => Json(ApiResponse { success: false, message: format!("{:?}", e) }),
    }
}

async fn start_typing(
    State(state): State<SharedState>,
    Json(req): Json<TypingRequest>,
) -> impl IntoResponse {
    let state = state.lock().unwrap();
    let _ = state.conn.reducers.start_typing(req.room_id);
    Json(ApiResponse { success: true, message: "".to_string() })
}

async fn stop_typing(
    State(state): State<SharedState>,
    Json(req): Json<TypingRequest>,
) -> impl IntoResponse {
    let state = state.lock().unwrap();
    let _ = state.conn.reducers.stop_typing(req.room_id);
    Json(ApiResponse { success: true, message: "".to_string() })
}

// ============================================================================
// MAIN
// ============================================================================

#[tokio::main]
async fn main() {
    println!("Connecting to SpacetimeDB...");
    
    // Connect to SpacetimeDB
    let conn = DbConnection::builder()
        .with_uri("http://localhost:3000")
        .with_module_name("chat-app-20260109-120000")
        .on_connect(|conn, _identity, token| {
            if let Ok(mut file) = std::fs::File::create(".spacetime_token") {
                use std::io::Write;
                let _ = file.write_all(token.as_bytes());
            }
            conn.subscription_builder()
                .on_applied(|_| println!("Subscriptions applied!"))
                .on_error(|_, err| eprintln!("Subscription error: {}", err))
                .subscribe_to_all_tables();
        })
        .on_connect_error(|_, err| eprintln!("Connection error: {:?}", err))
        .on_disconnect(|_, err| {
            if let Some(e) = err {
                eprintln!("Disconnected: {}", e);
            }
        })
        .with_token(std::fs::read_to_string(".spacetime_token").ok())
        .build()
        .expect("Failed to connect to SpacetimeDB");
    
    conn.run_threaded();
    
    // Wait for connection
    std::thread::sleep(std::time::Duration::from_millis(1000));
    println!("Connected!");
    
    let state = Arc::new(Mutex::new(AppState { conn }));
    
    // Spawn a task to periodically process messages
    let state_clone = state.clone();
    tokio::spawn(async move {
        loop {
            {
                let s = state_clone.lock().unwrap();
                let _ = s.conn.frame_tick();
            }
            tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
        }
    });
    
    let app = Router::new()
        .route("/", get(index))
        .route("/api/identity", get(get_identity))
        .route("/api/rooms", get(get_rooms))
        .route("/api/messages/:room_id", get(get_messages))
        .route("/api/typing/:room_id", get(get_typing))
        .route("/api/scheduled", get(get_scheduled))
        .route("/api/history/:message_id", get(get_edit_history))
        .route("/api/set-name", post(set_name))
        .route("/api/create-room", post(create_room))
        .route("/api/join-room", post(join_room))
        .route("/api/leave-room", post(leave_room))
        .route("/api/send-message", post(send_message))
        .route("/api/edit-message", post(edit_message))
        .route("/api/send-ephemeral", post(send_ephemeral))
        .route("/api/schedule-message", post(schedule_message))
        .route("/api/cancel-scheduled", post(cancel_scheduled))
        .route("/api/react", post(react))
        .route("/api/mark-read", post(mark_read))
        .route("/api/start-typing", post(start_typing))
        .route("/api/stop-typing", post(stop_typing))
        .layer(CorsLayer::permissive())
        .with_state(state);
    
    println!("\n🚀 Server running at http://localhost:8080");
    println!("Opening browser...\n");
    
    // Open browser
    let _ = open::that("http://localhost:8080");
    
    let listener = tokio::net::TcpListener::bind("127.0.0.1:8080").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

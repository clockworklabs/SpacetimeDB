//! Reference (oracle) SpacetimeDB module for the stack-bench team-chat task.
//!
//! Implements the full team-chat spec: users/presence/credits, rooms with
//! owners, membership with server-maintained unread counters, messages with
//! per-room gapless seq, idempotent sends, edit/tombstone-delete, and atomic
//! tips. SpacetimeDB reducers are serializable transactions, so the
//! concurrency checks (gapless seq, unread invariant, tip conservation) hold
//! by construction; the standalone server's commitlog provides restart
//! durability.

use spacetimedb::{reducer, table, ReducerContext, Table};

const STATUSES: [&str; 3] = ["online", "away", "offline"];
const MAX_TEXT: usize = 4000;

#[table(accessor = user, public)]
pub struct User {
    #[primary_key]
    pub username: String,
    pub status: String,
    pub balance: i32,
}

#[table(accessor = room, public)]
pub struct Room {
    #[primary_key]
    pub name: String,
    pub owner: String,
    pub next_seq: u32,
}

#[table(accessor = member, public,
        index(accessor = by_room, btree(columns = [room])),
        index(accessor = by_room_user, btree(columns = [room, user])))]
pub struct Member {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    pub room: String,
    pub user: String,
    pub last_read_seq: u32,
    pub unread: u32,
}

#[table(accessor = message, public,
        index(accessor = by_room, btree(columns = [room])),
        index(accessor = by_room_client, btree(columns = [room, client_msg_id])))]
pub struct Message {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    pub room: String,
    pub seq: u32,
    pub client_msg_id: String,
    pub sender: String,
    pub text: String,
    pub edited: bool,
    pub deleted: bool,
    pub sent_at_micros: i64,
}

// ---- helpers ----

fn get_user(ctx: &ReducerContext, username: &str) -> Result<User, String> {
    ctx.db
        .user()
        .username()
        .find(username.to_owned())
        .ok_or_else(|| format!("unknown user: {username}"))
}

fn get_room(ctx: &ReducerContext, name: &str) -> Result<Room, String> {
    ctx.db
        .room()
        .name()
        .find(name.to_owned())
        .ok_or_else(|| format!("unknown room: {name}"))
}

fn find_member(ctx: &ReducerContext, room: &str, user: &str) -> Option<Member> {
    ctx.db
        .member()
        .by_room_user()
        .filter((room, user))
        .next()
}

fn validate_text(text: &str) -> Result<(), String> {
    if text.is_empty() {
        return Err("text must not be empty".into());
    }
    if text.chars().count() > MAX_TEXT {
        return Err(format!("text exceeds {MAX_TEXT} characters"));
    }
    Ok(())
}

// ---- reducers ----

#[reducer]
pub fn register(ctx: &ReducerContext, username: String) -> Result<(), String> {
    if username.is_empty() {
        return Err("username must not be empty".into());
    }
    // Idempotent: existing users are left completely unchanged.
    if ctx.db.user().username().find(&username).is_none() {
        ctx.db.user().insert(User {
            username,
            status: "online".into(),
            balance: 100,
        });
    }
    Ok(())
}

#[reducer]
pub fn set_status(ctx: &ReducerContext, username: String, status: String) -> Result<(), String> {
    if !STATUSES.contains(&status.as_str()) {
        return Err(format!("invalid status: {status}"));
    }
    let mut user = get_user(ctx, &username)?;
    user.status = status;
    ctx.db.user().username().update(user);
    Ok(())
}

#[reducer]
pub fn create_room(ctx: &ReducerContext, username: String, room: String) -> Result<(), String> {
    get_user(ctx, &username)?;
    if room.is_empty() {
        return Err("room name must not be empty".into());
    }
    if ctx.db.room().name().find(&room).is_some() {
        return Err(format!("room already exists: {room}"));
    }
    ctx.db.room().insert(Room {
        name: room.clone(),
        owner: username.clone(),
        next_seq: 1,
    });
    ctx.db.member().insert(Member {
        id: 0,
        room,
        user: username,
        last_read_seq: 0,
        unread: 0,
    });
    Ok(())
}

#[reducer]
pub fn join_room(ctx: &ReducerContext, username: String, room: String) -> Result<(), String> {
    get_user(ctx, &username)?;
    get_room(ctx, &room)?;
    // Idempotent: existing membership state is preserved.
    if find_member(ctx, &room, &username).is_some() {
        return Ok(());
    }
    let unread = ctx
        .db
        .message()
        .by_room()
        .filter(&room)
        .filter(|m| m.sender != username)
        .count() as u32;
    ctx.db.member().insert(Member {
        id: 0,
        room,
        user: username,
        last_read_seq: 0,
        unread,
    });
    Ok(())
}

#[reducer]
pub fn leave_room(ctx: &ReducerContext, username: String, room: String) -> Result<(), String> {
    get_user(ctx, &username)?;
    let r = get_room(ctx, &room)?;
    if r.owner == username {
        return Err("the room owner cannot leave".into());
    }
    let member = find_member(ctx, &room, &username).ok_or("not a member")?;
    ctx.db.member().id().delete(member.id);
    Ok(())
}

#[reducer]
pub fn kick(ctx: &ReducerContext, actor: String, room: String, target: String) -> Result<(), String> {
    get_user(ctx, &actor)?;
    let r = get_room(ctx, &room)?;
    if r.owner != actor {
        return Err("only the room owner can kick".into());
    }
    if target == r.owner {
        return Err("cannot kick the room owner".into());
    }
    let member = find_member(ctx, &room, &target).ok_or("target is not a member")?;
    ctx.db.member().id().delete(member.id);
    Ok(())
}

#[reducer]
pub fn send_message(
    ctx: &ReducerContext,
    sender: String,
    room: String,
    text: String,
    client_msg_id: String,
) -> Result<(), String> {
    get_user(ctx, &sender)?;
    let mut r = get_room(ctx, &room)?;
    find_member(ctx, &room, &sender).ok_or("sender is not a member")?;
    validate_text(&text)?;
    // Idempotent retry: same client_msg_id in this room -> success, no new row.
    if ctx
        .db
        .message()
        .by_room_client()
        .filter((&room, &client_msg_id))
        .next()
        .is_some()
    {
        return Ok(());
    }
    let seq = r.next_seq;
    r.next_seq += 1;
    ctx.db.room().name().update(r);
    ctx.db.message().insert(Message {
        id: 0,
        room: room.clone(),
        seq,
        client_msg_id,
        sender: sender.clone(),
        text,
        edited: false,
        deleted: false,
        sent_at_micros: ctx.timestamp.to_micros_since_unix_epoch(),
    });
    // Atomic with the insert (same reducer transaction): bump unread for
    // every member except the sender.
    let members: Vec<Member> = ctx.db.member().by_room().filter(&room).collect();
    for mut m in members {
        if m.user != sender {
            m.unread += 1;
            ctx.db.member().id().update(m);
        }
    }
    Ok(())
}

fn find_message(ctx: &ReducerContext, room: &str, client_msg_id: &str) -> Result<Message, String> {
    ctx.db
        .message()
        .by_room_client()
        .filter((room, client_msg_id))
        .next()
        .ok_or_else(|| format!("no message {client_msg_id} in {room}"))
}

#[reducer]
pub fn edit_message(
    ctx: &ReducerContext,
    actor: String,
    room: String,
    client_msg_id: String,
    new_text: String,
) -> Result<(), String> {
    get_user(ctx, &actor)?;
    get_room(ctx, &room)?;
    let mut msg = find_message(ctx, &room, &client_msg_id)?;
    if msg.sender != actor {
        return Err("only the original sender can edit".into());
    }
    if msg.deleted {
        return Err("cannot edit a deleted message".into());
    }
    validate_text(&new_text)?;
    msg.text = new_text;
    msg.edited = true;
    ctx.db.message().id().update(msg);
    Ok(())
}

#[reducer]
pub fn delete_message(
    ctx: &ReducerContext,
    actor: String,
    room: String,
    client_msg_id: String,
) -> Result<(), String> {
    get_user(ctx, &actor)?;
    let r = get_room(ctx, &room)?;
    let mut msg = find_message(ctx, &room, &client_msg_id)?;
    if msg.sender != actor && r.owner != actor {
        return Err("only the sender or the room owner can delete".into());
    }
    if msg.deleted {
        return Err("message is already deleted".into());
    }
    msg.deleted = true;
    msg.text = String::new();
    ctx.db.message().id().update(msg);
    Ok(())
}

#[reducer]
pub fn mark_read(ctx: &ReducerContext, user: String, room: String, up_to_seq: u32) -> Result<(), String> {
    get_user(ctx, &user)?;
    get_room(ctx, &room)?;
    let mut member = find_member(ctx, &room, &user).ok_or("not a member")?;
    // Monotone: last_read_seq never decreases.
    member.last_read_seq = member.last_read_seq.max(up_to_seq);
    member.unread = ctx
        .db
        .message()
        .by_room()
        .filter(&room)
        .filter(|m| m.seq > member.last_read_seq && m.sender != user)
        .count() as u32;
    ctx.db.member().id().update(member);
    Ok(())
}

#[reducer]
pub fn tip(ctx: &ReducerContext, from_user: String, to_user: String, amount: i32) -> Result<(), String> {
    if from_user == to_user {
        return Err("cannot tip yourself".into());
    }
    if amount <= 0 {
        return Err("amount must be positive".into());
    }
    let mut from = get_user(ctx, &from_user)?;
    let mut to = get_user(ctx, &to_user)?;
    if from.balance < amount {
        return Err("insufficient balance".into());
    }
    from.balance -= amount;
    to.balance += amount;
    ctx.db.user().username().update(from);
    ctx.db.user().username().update(to);
    Ok(())
}

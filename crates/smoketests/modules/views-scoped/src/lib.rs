use spacetimedb::{log, AnonymousViewContext, Identity, Query, ReducerContext, SpacetimeType, Table, ViewContext};

#[spacetimedb::table(accessor = player)]
pub struct Player {
    #[primary_key]
    identity: Identity,
    name: String,
    team_id: u64,
    chunk_x: i32,
    chunk_y: i32,
}

#[spacetimedb::table(accessor = chat_message)]
pub struct ChatMessage {
    #[primary_key]
    #[auto_inc]
    id: u64,
    #[index(btree)]
    team_id: u64,
    text: String,
}

#[spacetimedb::table(accessor = entity)]
pub struct Entity {
    #[primary_key]
    #[auto_inc]
    id: u64,
    #[index(btree)]
    chunk_x: i32,
    chunk_y: i32,
    name: String,
}

/// The scope key of `regional_entities`: a chunk coordinate.
#[derive(SpacetimeType, Clone, Copy)]
pub struct ChunkKey {
    chunk_x: i32,
    chunk_y: i32,
}

fn team_scope(ctx: &ViewContext) -> Option<u64> {
    ctx.db.player().identity().find(ctx.sender()).map(|p| p.team_id)
}

/// Every player on a team shares one materialization of the team's chat.
#[spacetimedb::view(accessor = team_chat, public, scope = team_scope)]
pub fn team_chat(ctx: &AnonymousViewContext, team_id: u64) -> Vec<ChatMessage> {
    log::info!("team_chat body evaluated for team {team_id}");
    ctx.db.chat_message().team_id().filter(team_id).collect()
}

/// Like `team_chat`, but the body returns a query.
#[spacetimedb::view(accessor = team_chat_query, public, scope = team_scope)]
pub fn team_chat_query(ctx: &AnonymousViewContext, team_id: u64) -> impl Query<ChatMessage> {
    ctx.from.chat_message().r#where(move |m| m.team_id.eq(team_id))
}

fn chunk_scope(ctx: &ViewContext) -> Option<ChunkKey> {
    ctx.db.player().identity().find(ctx.sender()).map(|p| ChunkKey {
        chunk_x: p.chunk_x,
        chunk_y: p.chunk_y,
    })
}

/// Every player in a chunk shares one materialization of the chunk's entities.
#[spacetimedb::view(accessor = regional_entities, public, scope = chunk_scope)]
pub fn regional_entities(ctx: &AnonymousViewContext, key: ChunkKey) -> Vec<Entity> {
    log::info!("regional_entities body evaluated for chunk ({}, {})", key.chunk_x, key.chunk_y);
    ctx.db
        .entity()
        .chunk_x()
        .filter(key.chunk_x)
        .filter(|e| e.chunk_y == key.chunk_y)
        .collect()
}

#[spacetimedb::reducer]
pub fn join(ctx: &ReducerContext, name: String, team_id: u64) {
    ctx.db.player().insert(Player {
        identity: ctx.sender(),
        name,
        team_id,
        chunk_x: 0,
        chunk_y: 0,
    });
}

#[spacetimedb::reducer]
pub fn leave(ctx: &ReducerContext) {
    ctx.db.player().identity().delete(ctx.sender());
}

#[spacetimedb::reducer]
pub fn set_team(ctx: &ReducerContext, team_id: u64) {
    if let Some(player) = ctx.db.player().identity().find(ctx.sender()) {
        ctx.db.player().identity().update(Player { team_id, ..player });
    }
}

#[spacetimedb::reducer]
pub fn move_to(ctx: &ReducerContext, chunk_x: i32, chunk_y: i32) {
    if let Some(player) = ctx.db.player().identity().find(ctx.sender()) {
        ctx.db.player().identity().update(Player {
            chunk_x,
            chunk_y,
            ..player
        });
    }
}

#[spacetimedb::reducer]
pub fn send(ctx: &ReducerContext, team_id: u64, text: String) {
    ctx.db.chat_message().insert(ChatMessage { id: 0, team_id, text });
}

#[spacetimedb::reducer]
pub fn spawn(ctx: &ReducerContext, name: String, chunk_x: i32, chunk_y: i32) {
    ctx.db.entity().insert(Entity {
        id: 0,
        chunk_x,
        chunk_y,
        name,
    });
}

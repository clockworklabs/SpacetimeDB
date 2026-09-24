use spacetimedb::{AnonymousViewContext, Identity, ReducerContext, Table, ViewContext};

#[spacetimedb::table(accessor = player)]
pub struct Player {
    #[primary_key]
    identity: Identity,
    team_id: u64,
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

/// The team chat as a per-user view: computed once per subscriber.
#[spacetimedb::view(accessor = team_chat_per_user, public)]
pub fn team_chat_per_user(ctx: &ViewContext) -> Vec<ChatMessage> {
    match ctx.db.player().identity().find(ctx.sender()) {
        Some(player) => ctx.db.chat_message().team_id().filter(player.team_id).collect(),
        None => vec![],
    }
}

fn team_scope(ctx: &ViewContext) -> Option<u64> {
    ctx.db.player().identity().find(ctx.sender()).map(|p| p.team_id)
}

/// The team chat as a scoped view: computed once per team.
#[spacetimedb::view(accessor = team_chat_scoped, public, scope = team_scope)]
pub fn team_chat_scoped(ctx: &AnonymousViewContext, team_id: u64) -> Vec<ChatMessage> {
    ctx.db.chat_message().team_id().filter(team_id).collect()
}

#[spacetimedb::reducer]
pub fn join(ctx: &ReducerContext, team_id: u64) {
    ctx.db.player().insert(Player {
        identity: ctx.sender(),
        team_id,
    });
}

#[spacetimedb::reducer]
pub fn set_team(ctx: &ReducerContext, team_id: u64) {
    if let Some(player) = ctx.db.player().identity().find(ctx.sender()) {
        ctx.db.player().identity().update(Player { team_id, ..player });
    }
}

#[spacetimedb::reducer]
pub fn send(ctx: &ReducerContext, team_id: u64, text: String) {
    ctx.db.chat_message().insert(ChatMessage { id: 0, team_id, text });
}

use spacetimedb::{
    reducer, table, view, AnonymousViewContext, Identity, ReducerContext, SpacetimeType, Table, ViewContext,
};

#[table(name = player)]
struct Player {
    #[primary_key]
    #[auto_inc]
    entity_id: u64,
    #[unique]
    identity: Identity,
}

#[table(name = player_level)]
struct PlayerLevel {
    #[unique]
    entity_id: u64,
    #[index(btree)]
    level: u64,
}

#[derive(SpacetimeType)]
struct PlayerAndLevel {
    entity_id: u64,
    identity: Identity,
    level: u64,
}

#[reducer]
fn insert_player(ctx: &ReducerContext, identity: Identity, level: u64) {
    let Player { entity_id, .. } = ctx.db.player().insert(Player { entity_id: 0, identity });
    ctx.db.player_level().insert(PlayerLevel { entity_id, level });
}

#[reducer]
fn delete_player(ctx: &ReducerContext, identity: Identity) {
    if let Some(player) = ctx.db.player().identity().find(identity) {
        ctx.db.player().entity_id().delete(player.entity_id);
        ctx.db.player_level().entity_id().delete(player.entity_id);
    }
}

#[view(name = my_player, public)]
fn my_player(ctx: &ViewContext) -> Option<Player> {
    ctx.db.player().identity().find(ctx.sender)
}

#[view(name = my_player_and_level, public)]
fn my_player_and_level(ctx: &ViewContext) -> Option<PlayerAndLevel> {
    ctx.db
        .player()
        .identity()
        .find(ctx.sender)
        .and_then(|Player { entity_id, identity }| {
            ctx.db
                .player_level()
                .entity_id()
                .find(entity_id)
                .map(|PlayerLevel { level, .. }| PlayerAndLevel {
                    entity_id,
                    identity,
                    level,
                })
        })
}

#[view(name = players_at_level_0, public)]
fn players_at_level_0(ctx: &AnonymousViewContext) -> Vec<Player> {
    ctx.db
        .player_level()
        .level()
        .filter(0u64)
        .filter_map(|p| ctx.db.player().entity_id().find(p.entity_id))
        .collect()
}

use spacetimedb::{
    reducer, table, view, AnonymousViewContext, Identity, ReducerContext, SpacetimeType, Table, ViewContext,
};

#[table(name = player, public)]
struct Player {
    #[primary_key]
    #[auto_inc]
    entity_id: u64,
    #[unique]
    identity: Identity,
}

#[table(name = player_level, public)]
struct PlayerLevel {
    #[unique]
    entity_id: u64,
    #[index(btree)]
    level: u64,
}

#[table(name = player_location)]
pub struct PlayerLocation {
    #[unique]
    pub entity_id: u64,
    #[index(btree)]
    pub active: bool,
    pub x: i32,
    pub y: i32,
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

#[reducer]
pub fn move_player(ctx: &ReducerContext, dx: i32, dy: i32) {
    let my_player = ctx.db.player().identity().find(ctx.sender).unwrap_or_else(|| {
        ctx.db.player().insert(Player {
            entity_id: 0,
            identity: ctx.sender,
        })
    });
    match ctx.db.player_location().entity_id().find(my_player.entity_id) {
        Some(loc @ PlayerLocation { mut x, mut y, .. }) => {
            x += dx;
            y += dy;
            ctx.db.player_location().entity_id().update(PlayerLocation {
                entity_id: loc.entity_id,
                active: loc.active,
                x,
                y,
            });
        }
        None => {
            ctx.db.player_location().insert(PlayerLocation {
                entity_id: my_player.entity_id,
                active: true,
                x: dx,
                y: dy,
            });
        }
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

#[view(name = nearby_players, public)]
pub fn nearby_players(ctx: &ViewContext) -> Vec<PlayerLocation> {
    ctx.db
        .player()
        .identity()
        .find(ctx.sender)
        .and_then(|my_player| ctx.db.player_location().entity_id().find(my_player.entity_id))
        .iter()
        .flat_map(|my_loc| {
            ctx.db
                .player_location()
                .active()
                .filter(true)
                .filter(|loc| loc.entity_id != my_loc.entity_id)
                .filter(|loc| {
                    let dx = (loc.x - my_loc.x).abs();
                    let dy = (loc.y - my_loc.y).abs();
                    dx < 5 && dy < 5
                })
        })
        .collect()
}

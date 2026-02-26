use spacetimedb::{AnonymousViewContext, ReducerContext, Table, ViewContext};

#[derive(Copy, Clone)]
#[spacetimedb::table(accessor = player_state)]
#[spacetimedb::table(accessor = player_level)]
pub struct PlayerState {
    #[primary_key]
    id: u64,
    #[index(btree)]
    level: u64,
}

#[derive(Clone)]
#[spacetimedb::table(accessor = player_info, index(accessor=age_level_index, btree(columns = [age, level])))]
pub struct PlayerInfo {
    #[primary_key]
    id: u64,
    age: u64,
    level: u64,
}

#[spacetimedb::reducer]
pub fn add_player_level(ctx: &ReducerContext, id: u64, level: u64) {
    ctx.db.player_level().insert(PlayerState { id, level });
}

#[spacetimedb::view(accessor = my_player_and_level, public)]
pub fn my_player_and_level(ctx: &AnonymousViewContext) -> Option<PlayerState> {
    ctx.db.player_level().id().find(0)
}

#[spacetimedb::view(accessor = player_and_level, public)]
pub fn player_and_level(ctx: &AnonymousViewContext) -> Vec<PlayerState> {
    ctx.db.player_level().level().filter(2u64).collect()
}

#[spacetimedb::view(accessor = player, public)]
pub fn player(ctx: &ViewContext) -> Option<PlayerState> {
    log::info!("player view called");
    ctx.db.player_state().id().find(42)
}

#[spacetimedb::view(accessor = player_none, public)]
pub fn player_none(_ctx: &ViewContext) -> Option<PlayerState> {
    None
}

#[spacetimedb::view(accessor = player_vec, public)]
pub fn player_vec(ctx: &ViewContext) -> Vec<PlayerState> {
    let first = ctx.db.player_state().id().find(42).unwrap();
    let second = PlayerState { id: 7, level: 3 };
    vec![first, second]
}

#[spacetimedb::view(accessor = player_info_multi_index, public)]
pub fn player_info_view(ctx: &ViewContext) -> Option<PlayerInfo> {
    log::info!("player_info called");
    ctx.db.player_info().age_level_index().filter((25u64, 7u64)).next()
}

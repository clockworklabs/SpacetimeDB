use spacetimedb::ViewContext;

#[derive(Copy, Clone)]
#[spacetimedb::table(accessor = player_state)]
pub struct PlayerState {
    #[primary_key]
    id: u64,
    #[index(btree)]
    level: u64,
}

#[spacetimedb::view(accessor = player, public)]
pub fn player(_ctx: &ViewContext) -> Option<PlayerState> {
    panic!("This view is trapped")
}

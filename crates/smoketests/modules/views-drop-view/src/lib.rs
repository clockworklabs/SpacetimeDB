#[derive(Copy, Clone)]
#[spacetimedb::table(name = player_state)]
pub struct PlayerState {
    #[primary_key]
    id: u64,
    #[index(btree)]
    level: u64,
}

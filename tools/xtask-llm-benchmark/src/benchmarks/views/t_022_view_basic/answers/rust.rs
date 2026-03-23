use spacetimedb::{table, view, AnonymousViewContext};

#[table(accessor = player, public)]
pub struct Player {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    pub name: String,
    pub score: u32,
}

#[view(accessor = all_players, public)]
fn all_players(ctx: &AnonymousViewContext) -> Vec<Player> {
    ctx.db.player().iter().collect()
}

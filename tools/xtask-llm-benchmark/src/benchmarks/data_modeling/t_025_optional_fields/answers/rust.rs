use spacetimedb::{reducer, table, ReducerContext, Table};

#[table(accessor = player)]
pub struct Player {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    pub name: String,
    pub nickname: Option<String>,
    pub high_score: Option<u32>,
}

#[reducer]
pub fn create_player(ctx: &ReducerContext, name: String, nickname: Option<String>, high_score: Option<u32>) {
    ctx.db.player().insert(Player {
        id: 0,
        name,
        nickname,
        high_score,
    });
}

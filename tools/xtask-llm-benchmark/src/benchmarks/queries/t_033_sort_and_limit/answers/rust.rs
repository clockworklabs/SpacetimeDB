use spacetimedb::{reducer, table, ReducerContext, Table};

#[table(accessor = player)]
pub struct Player {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    pub name: String,
    pub score: u64,
}

#[table(accessor = leaderboard)]
pub struct LeaderboardEntry {
    #[primary_key]
    pub rank: u32,
    pub player_name: String,
    pub score: u64,
}

#[reducer]
pub fn build_leaderboard(ctx: &ReducerContext, limit: u32) {
    let mut players: Vec<Player> = ctx.db.player().iter().collect();
    players.sort_by(|a, b| b.score.cmp(&a.score));

    for (i, p) in players.into_iter().take(limit as usize).enumerate() {
        ctx.db.leaderboard().insert(LeaderboardEntry {
            rank: (i as u32) + 1,
            player_name: p.name,
            score: p.score,
        });
    }
}

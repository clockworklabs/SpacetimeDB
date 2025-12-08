use spacetimedb::{table, view, ViewContext};

#[table(name = player)]
struct Player {
    #[primary_key]
    player_id: u64,
}
/// Cannot use a view as a scheduled function
#[spacetimedb::table(name = sched_table, scheduled(scheduled_table_view))]
struct ScheduledTable {
    #[primary_key]
    #[auto_inc]
    scheduled_id: u64,
    scheduled_at: spacetimedb::ScheduleAt,
    x: u8,
    y: u8,
}

/// Cannot use a view as a scheduled function
#[view(name = sched_table_view, public)]
fn sched_table_view(_: &ViewContext, _args: ScheduledTable) -> Vec<Player> {
    vec![]
}

fn main() {}

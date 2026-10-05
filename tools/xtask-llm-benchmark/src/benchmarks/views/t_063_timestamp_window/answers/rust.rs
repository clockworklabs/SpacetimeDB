use spacetimedb::{reducer, table, view, AnonymousViewContext, ReducerContext, Table, Timestamp};

#[table(accessor = event, public)]
pub struct Event {
    #[primary_key]
    pub id: u64,
    #[index(btree)]
    pub occurred_at: Timestamp,
    pub label: String,
}

#[reducer]
pub fn seed(ctx: &ReducerContext) {
    for (id, micros) in [100, 200, 300, 400, 500].into_iter().enumerate() {
        ctx.db.event().insert(Event {
            id: id as u64 + 1,
            occurred_at: Timestamp::from_micros_since_unix_epoch(micros),
            label: format!("event-{micros}"),
        });
    }
}

#[view(accessor = window_event, public)]
pub fn window_event(ctx: &AnonymousViewContext) -> Vec<Event> {
    let start = Timestamp::from_micros_since_unix_epoch(200);
    let end = Timestamp::from_micros_since_unix_epoch(400);
    ctx.db.event().occurred_at().filter(start..=end).collect()
}

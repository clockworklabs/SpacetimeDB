use spacetimedb::{reducer, table, ReducerContext, Table};

#[table(
    accessor = event_log,
    index(accessor = by_category_severity, btree(columns = [category, severity]))
)]
pub struct EventLog {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    pub category: String,
    pub severity: u32,
    pub message: String,
}

#[table(accessor = filtered_event)]
pub struct FilteredEvent {
    #[primary_key]
    pub event_id: u64,
    pub message: String,
}

#[reducer]
pub fn filter_events(ctx: &ReducerContext, category: String, severity: u32) {
    for e in ctx.db.event_log().iter() {
        if e.category == category && e.severity == severity {
            ctx.db.filtered_event().insert(FilteredEvent {
                event_id: e.id,
                message: e.message,
            });
        }
    }
}

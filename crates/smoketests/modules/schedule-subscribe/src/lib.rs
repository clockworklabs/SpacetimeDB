use spacetimedb::{duration, log, Identity, Query, ReducerContext, Table, Timestamp, ViewContext};

#[spacetimedb::table(accessor = scheduled_table, public, scheduled(my_reducer, at = sched_at))]
pub struct ScheduledTable {
    #[primary_key]
    #[auto_inc]
    scheduled_id: u64,
    sched_at: spacetimedb::ScheduleAt,
    prev: Timestamp,
}

#[spacetimedb::table(accessor = failing_scheduled_table, public, scheduled(failing_reducer, at = sched_at))]
pub struct FailingScheduledTable {
    #[primary_key]
    #[auto_inc]
    scheduled_id: u64,
    sched_at: spacetimedb::ScheduleAt,
    prev: Timestamp,
}

#[spacetimedb::table(accessor = player_entity, public)]
pub struct PlayerEntity {
    #[primary_key]
    entity_id: u64,
    owner: Identity,
}

#[spacetimedb::view(accessor = scheduled_view, public)]
fn scheduled_view(ctx: &ViewContext) -> impl Query<ScheduledTable> {
    ctx.from.scheduled_table().build()
}

#[spacetimedb::view(accessor = scheduled_sender_view, public)]
fn scheduled_sender_view(ctx: &ViewContext) -> impl Query<ScheduledTable> {
    ctx.from
        .player_entity()
        .r#where(|pe| pe.owner.eq(ctx.sender()))
        .right_semijoin(ctx.from.scheduled_table(), |pe, st| pe.entity_id.eq(st.scheduled_id))
        .build()
}

#[spacetimedb::view(accessor = failing_scheduled_sender_view, public)]
fn failing_scheduled_sender_view(ctx: &ViewContext) -> impl Query<FailingScheduledTable> {
    ctx.from
        .player_entity()
        .r#where(|pe| pe.owner.eq(ctx.sender()))
        .right_semijoin(ctx.from.failing_scheduled_table(), |pe, st| {
            pe.entity_id.eq(st.scheduled_id)
        })
        .build()
}

#[spacetimedb::reducer]
fn schedule_reducer(ctx: &ReducerContext) {
    ctx.db.scheduled_table().insert(ScheduledTable {
        prev: Timestamp::from_micros_since_unix_epoch(0),
        scheduled_id: 2,
        sched_at: Timestamp::from_micros_since_unix_epoch(0).into(),
    });
}

#[spacetimedb::reducer]
fn schedule_failing_reducer(ctx: &ReducerContext) {
    ctx.db.failing_scheduled_table().insert(FailingScheduledTable {
        prev: Timestamp::from_micros_since_unix_epoch(0),
        scheduled_id: 3,
        sched_at: Timestamp::from_micros_since_unix_epoch(0).into(),
    });
}

#[spacetimedb::reducer]
fn schedule_repeated_reducer(ctx: &ReducerContext) {
    ctx.db.scheduled_table().insert(ScheduledTable {
        prev: Timestamp::from_micros_since_unix_epoch(0),
        scheduled_id: 1,
        sched_at: duration!(100ms).into(),
    });
}

#[spacetimedb::reducer]
fn seed_player_entity(ctx: &ReducerContext, entity_id: u64) {
    ctx.db.player_entity().entity_id().delete(&entity_id);
    ctx.db.player_entity().insert(PlayerEntity {
        entity_id,
        owner: ctx.sender(),
    });
}

#[spacetimedb::reducer]
pub fn my_reducer(ctx: &ReducerContext, arg: ScheduledTable) {
    log::info!(
        "Invoked: ts={:?}, delta={:?}",
        ctx.timestamp,
        ctx.timestamp.duration_since(arg.prev)
    );
}

#[spacetimedb::reducer]
pub fn failing_reducer(_ctx: &ReducerContext, _arg: FailingScheduledTable) -> Result<(), String> {
    Err("scheduled reducer failed".into())
}

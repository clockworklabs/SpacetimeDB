//! Checks that `is_internal()` is true exactly when the sender is this database.
use spacetimedb::{ProcedureContext, ReducerContext, Table};

#[spacetimedb::reducer(init)]
pub fn init(ctx: &ReducerContext) {
    // The sender of `init` is the database's owner, so it is not internal.
    assert_ne!(ctx.sender(), ctx.database_identity());
    assert!(!ctx.sender_auth().is_internal());
}

#[spacetimedb::reducer]
pub fn check(ctx: &ReducerContext) {
    assert_eq!(ctx.sender_auth().is_internal(), ctx.sender() == ctx.database_identity());
}

#[spacetimedb::procedure]
pub fn check_procedure(ctx: &mut ProcedureContext) -> bool {
    let sender = ctx.sender();
    let is_self = sender == ctx.database_identity();
    ctx.with_tx(|tx| {
        assert_eq!(tx.sender(), sender);
        assert_eq!(tx.sender_auth().is_internal(), is_self);
    });
    true
}

#[spacetimedb::table(accessor = jobs, scheduled(scheduled))]
pub struct Job {
    #[primary_key]
    #[auto_inc]
    id: u64,
    scheduled_at: spacetimedb::ScheduleAt,
}

#[spacetimedb::table(accessor = finished)]
pub struct Finished {
    #[primary_key]
    id: u64,
}

#[spacetimedb::reducer]
pub fn schedule(ctx: &ReducerContext) {
    ctx.db.jobs().insert(Job {
        id: 0,
        scheduled_at: ctx.timestamp.into(),
    });
}

#[spacetimedb::reducer]
pub fn scheduled(ctx: &ReducerContext, job: Job) {
    // The database is the sender of its scheduled reducers.
    assert_eq!(ctx.sender(), ctx.database_identity());
    assert!(ctx.sender_auth().is_internal());
    assert!(!ctx.sender_auth().has_jwt());
    ctx.db.finished().insert(Finished { id: job.id });
}

#[spacetimedb::procedure]
pub fn scheduled_finished(ctx: &mut ProcedureContext) -> bool {
    ctx.with_tx(|tx| tx.db.finished().iter().next().is_some())
}

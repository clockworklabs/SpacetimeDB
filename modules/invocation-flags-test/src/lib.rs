//! Real Wasm fixture for generic host invocation authority.
use spacetimedb::{ProcedureContext, ReducerContext, Table};

#[spacetimedb::reducer(init)]
pub fn init(ctx: &ReducerContext) {
    assert!(ctx.sender_auth().is_internal());
    assert!(!ctx.sender_auth().has_jwt());
}

#[spacetimedb::reducer]
pub fn external(ctx: &ReducerContext) {
    assert!(!ctx.sender_auth().is_internal());
    assert_eq!(ctx.connection_id(), None);
    assert!(!ctx.sender_auth().has_jwt());
}

#[spacetimedb::reducer(internal)]
pub fn internal(ctx: &ReducerContext) {
    assert!(ctx.sender_auth().is_internal());
}

#[spacetimedb::reducer(private)]
pub fn private(_ctx: &ReducerContext) {}

#[spacetimedb::procedure]
pub fn external_procedure(ctx: &mut ProcedureContext) -> bool {
    assert!(!ctx.sender_auth().is_internal());
    let sender = ctx.sender();
    let connection = ctx.connection_id();
    ctx.with_tx(|tx| {
        assert!(!tx.sender_auth().is_internal());
        assert_eq!(tx.sender(), sender);
        assert_eq!(tx.connection_id(), connection);
    });
    true
}

#[spacetimedb::procedure(internal)]
pub fn internal_procedure(ctx: &mut ProcedureContext) -> bool {
    assert!(ctx.sender_auth().is_internal());
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

#[spacetimedb::reducer(internal)]
pub fn scheduled(ctx: &ReducerContext, job: Job) {
    assert!(ctx.sender_auth().is_internal());
    assert_eq!(ctx.sender(), ctx.database_identity());
    assert_eq!(ctx.connection_id(), None);
    assert!(!ctx.sender_auth().has_jwt());
    ctx.db.finished().insert(Finished { id: job.id });
}

#[spacetimedb::procedure]
pub fn scheduled_finished(ctx: &mut ProcedureContext) -> bool {
    ctx.with_tx(|tx| tx.db.finished().iter().next().is_some())
}

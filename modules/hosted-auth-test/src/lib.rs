//! Actual host integration fixture for verified container authentication.
use spacetimedb::{ConnectionId, Identity, ProcedureContext, ReducerContext, Table};

#[spacetimedb::table(accessor = observations)]
pub struct Observation {
    #[primary_key]
    connection: ConnectionId,
    sender: Identity,
    internal: bool,
    jwt_identity: Identity,
    disconnected: bool,
}

#[spacetimedb::reducer(client_connected)]
pub fn connected(ctx: &ReducerContext) {
    ctx.db.observations().insert(Observation {
        connection: ctx.connection_id().unwrap(),
        sender: ctx.sender(),
        internal: ctx.sender_auth().is_internal(),
        jwt_identity: ctx.sender_auth().jwt().unwrap().identity(),
        disconnected: false,
    });
}

#[spacetimedb::reducer(client_disconnected)]
pub fn disconnected(ctx: &ReducerContext) -> Result<(), String> {
    let connection = ctx.connection_id().unwrap();
    let mut observation = ctx.db.observations().connection().find(connection).unwrap();
    assert_eq!(ctx.sender(), observation.sender);
    assert_eq!(ctx.sender_auth().is_internal(), observation.internal);
    assert_eq!(ctx.sender_auth().jwt().unwrap().identity(), observation.jwt_identity);
    // Exercise host fallback cleanup after a user callback rejects disconnect.
    if connection == ConnectionId::from_u128(999) {
        return Err("intentional disconnect failure".into());
    }
    observation.disconnected = true;
    ctx.db.observations().connection().update(observation);
    Ok(())
}

#[spacetimedb::reducer]
pub fn inspect_context(
    ctx: &ReducerContext,
    sender: Identity,
    connection: Option<ConnectionId>,
    internal: bool,
    jwt: bool,
) {
    assert_eq!(ctx.sender(), sender);
    assert_eq!(ctx.connection_id(), connection);
    assert_eq!(ctx.sender_auth().is_internal(), internal);
    assert_eq!(ctx.sender_auth().has_jwt(), jwt);
    if jwt {
        assert_eq!(ctx.sender_auth().jwt().unwrap().identity(), sender);
    }
}

#[spacetimedb::reducer(internal)]
pub fn internal_only(ctx: &ReducerContext) {
    assert!(ctx.sender_auth().is_internal());
}

#[spacetimedb::reducer(private)]
pub fn private_only(_ctx: &ReducerContext) {}

#[spacetimedb::reducer]
pub fn inspect_observation(
    ctx: &ReducerContext,
    connection: ConnectionId,
    sender: Identity,
    internal: bool,
    disconnected: bool,
) {
    let observation = ctx.db.observations().connection().find(connection).unwrap();
    assert_eq!(observation.sender, sender);
    assert_eq!(observation.jwt_identity, sender);
    assert_eq!(observation.internal, internal);
    assert_eq!(observation.disconnected, disconnected);
}

#[spacetimedb::procedure]
pub fn inspect_procedure(
    ctx: &mut ProcedureContext,
    sender: Identity,
    connection: Option<ConnectionId>,
    internal: bool,
) -> bool {
    assert_eq!(ctx.sender(), sender);
    assert_eq!(ctx.connection_id(), connection);
    assert_eq!(ctx.sender_auth().is_internal(), internal);
    assert_eq!(ctx.sender_auth().jwt().unwrap().identity(), sender);
    ctx.with_tx(|tx| {
        assert_eq!(tx.sender(), sender);
        assert_eq!(tx.connection_id(), connection);
        assert_eq!(tx.sender_auth().is_internal(), internal);
        assert_eq!(tx.sender_auth().jwt().unwrap().identity(), sender);
    });
    true
}

#[spacetimedb::table(accessor = scheduled_checks, scheduled(scheduled_check))]
pub struct ScheduledCheck {
    #[primary_key]
    #[auto_inc]
    id: u64,
    scheduled_at: spacetimedb::ScheduleAt,
}

#[spacetimedb::reducer]
pub fn schedule_check(ctx: &ReducerContext) {
    ctx.db.scheduled_checks().insert(ScheduledCheck {
        id: 0,
        scheduled_at: ctx.timestamp.into(),
    });
}

#[spacetimedb::reducer]
pub fn scheduled_check(ctx: &ReducerContext, _job: ScheduledCheck) {
    assert_eq!(ctx.sender(), ctx.database_identity());
    assert_eq!(ctx.connection_id(), None);
    assert!(ctx.sender_auth().is_internal());
    assert!(!ctx.sender_auth().has_jwt());
    ctx.db.observations().insert(Observation {
        connection: ConnectionId::from_u128(777),
        sender: ctx.sender(),
        internal: true,
        jwt_identity: ctx.sender(),
        disconnected: false,
    });
}

#[spacetimedb::procedure]
pub fn scheduled_finished(ctx: &mut ProcedureContext) -> bool {
    ctx.with_tx(|tx| {
        tx.db
            .observations()
            .connection()
            .find(ConnectionId::from_u128(777))
            .is_some()
    })
}

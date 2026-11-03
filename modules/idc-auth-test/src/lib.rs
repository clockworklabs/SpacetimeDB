use spacetimedb::{ConnectionId, Identity, ReducerContext, Table};

#[spacetimedb::table(accessor = idc_event)]
pub struct IdcEvent {
    #[primary_key]
    #[auto_inc]
    id: u64,
    connection_id: ConnectionId,
    phase: String,
}

fn record_event(ctx: &ReducerContext, phase: &str) {
    ctx.db.idc_event().insert(IdcEvent {
        id: 0,
        connection_id: ctx.connection_id().unwrap(),
        phase: phase.into(),
    });
}

fn assert_connected(ctx: &ReducerContext) {
    let connection_id = ctx.connection_id().unwrap();
    assert!(ctx
        .db
        .idc_event()
        .iter()
        .any(|event| { event.connection_id == connection_id && event.phase == "connected" }));
}

fn check_auth(ctx: &ReducerContext) -> Result<(), String> {
    if ctx.sender() != Identity::ONE {
        return Err("untrusted database".into());
    }
    assert!(ctx.connection_id().is_some());
    assert!(!ctx.sender_auth().is_internal());
    assert!(ctx.sender_auth().jwt().is_none());
    Ok(())
}

#[spacetimedb::reducer(client_connected)]
pub fn connected(ctx: &ReducerContext) -> Result<(), String> {
    // Ordinary clients still authenticate with a module-visible JWT.
    if ctx.sender_auth().jwt().is_some() {
        return Ok(());
    }
    check_auth(ctx)?;
    record_event(ctx, "connected");
    Ok(())
}

#[spacetimedb::reducer(client_disconnected)]
pub fn disconnected(ctx: &ReducerContext) -> Result<(), String> {
    if ctx.sender_auth().jwt().is_some() {
        return Ok(());
    }
    check_auth(ctx)?;
    assert_connected(ctx);
    record_event(ctx, "disconnected");
    Ok(())
}

#[spacetimedb::reducer]
pub fn receive(ctx: &ReducerContext, fail: bool) -> Result<(), String> {
    check_auth(ctx)?;
    assert_connected(ctx);
    if fail {
        return Err("delivery rejected".into());
    }
    record_event(ctx, "received");
    Ok(())
}

#[spacetimedb::reducer]
pub fn assert_counts(ctx: &ReducerContext, connected: u64, received: u64, disconnected: u64) {
    assert!(ctx.sender_auth().jwt().is_some());
    for (phase, expected) in [
        ("connected", connected),
        ("received", received),
        ("disconnected", disconnected),
    ] {
        assert_eq!(
            ctx.db.idc_event().iter().filter(|event| event.phase == phase).count() as u64,
            expected
        );
    }
}

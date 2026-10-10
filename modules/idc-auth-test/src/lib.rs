use spacetimedb::{Identity, ReducerContext};

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
    check_auth(ctx)
}

#[spacetimedb::reducer]
pub fn receive(ctx: &ReducerContext) -> Result<(), String> {
    check_auth(ctx)
}

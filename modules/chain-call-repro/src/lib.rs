use spacetimedb::{
    remote_reducer::{into_reducer_error_message, RemoteCallError},
    Identity, ReducerContext, SpacetimeType, Table,
};

#[derive(SpacetimeType, Clone)]
pub struct CallPayload {
    pub client_label: String,
    pub seq: u64,
    pub message: String,
}

#[spacetimedb::table(accessor = call_log, public)]
pub struct CallLog {
    #[primary_key]
    #[auto_inc]
    id: u64,
    kind: String,
    client_label: String,
    seq: u64,
    message: String,
}

fn log_entry(ctx: &ReducerContext, kind: &str, payload: &CallPayload) {
    ctx.db.call_log().insert(CallLog {
        id: 0,
        kind: kind.to_string(),
        client_label: payload.client_label.clone(),
        seq: payload.seq,
        message: payload.message.clone(),
    });
}

fn burn(iters: u64) {
    if iters == 0 {
        return;
    }

    let mut x = 1u64;
    for i in 0..iters {
        x = x.wrapping_mul(6364136223846793005u64).wrapping_add(i | 1);
    }
    if x == 0 {
        panic!("impossible burn result");
    }
}

#[spacetimedb::reducer]
pub fn record_on_b(ctx: &ReducerContext, payload: CallPayload, burn_iters: u64) -> Result<(), String> {
    burn(burn_iters);
    log_entry(ctx, "recv_from_a", &payload);
    Ok(())
}

#[spacetimedb::reducer]
pub fn record_on_c(ctx: &ReducerContext, payload: CallPayload, burn_iters: u64) -> Result<(), String> {
    burn(burn_iters);
    log_entry(ctx, "recv_from_b", &payload);
    Ok(())
}

#[spacetimedb::reducer]
pub fn call_b_from_a(
    ctx: &ReducerContext,
    b_hex: String,
    client_label: String,
    seq: u64,
    message: String,
    burn_iters: u64,
    hold_iters: u64,
) -> Result<(), String> {
    burn(burn_iters);

    let b = Identity::from_hex(&b_hex).expect("invalid B identity");
    let payload = CallPayload {
        client_label,
        seq,
        message,
    };
    let args = spacetimedb::spacetimedb_lib::bsatn::to_vec(&(payload.clone(), burn_iters))
        .expect("failed to encode args for record_on_b");
    spacetimedb::remote_reducer::call_reducer_on_db_2pc(b, "record_on_b", &args)
        .map_err(|e| match e {
            RemoteCallError::Wounded(_) => into_reducer_error_message(e),
            _ => format!("call_b_from_a: call to B failed: {e}"),
        })?;

    // Hold A open after B is prepared so B keeps its global-tx admission lock
    // long enough for concurrent work on B to contend and trigger wound flow.
    burn(hold_iters);

    log_entry(ctx, "sent_to_b", &payload);
    Ok(())
}

#[spacetimedb::reducer]
pub fn call_c_from_b(
    ctx: &ReducerContext,
    c_hex: String,
    client_label: String,
    seq: u64,
    message: String,
    burn_iters: u64,
    hold_iters: u64,
) -> Result<(), String> {
    burn(burn_iters);

    let c = Identity::from_hex(&c_hex).expect("invalid C identity");
    let payload = CallPayload {
        client_label,
        seq,
        message,
    };
    let args = spacetimedb::spacetimedb_lib::bsatn::to_vec(&(payload.clone(), burn_iters))
        .expect("failed to encode args for record_on_c");
    spacetimedb::remote_reducer::call_reducer_on_db_2pc(c, "record_on_c", &args)
        .map_err(|e| match e {
            RemoteCallError::Wounded(_) => into_reducer_error_message(e),
            _ => format!("call_c_from_b: call to C failed: {e}"),
        })?;

    // Hold B open after C is prepared so B remains the global-tx owner while
    // A-originated work attempts to prepare on B.
    burn(hold_iters);

    log_entry(ctx, "sent_to_c", &payload);
    Ok(())
}

#[spacetimedb::reducer]
pub fn assert_kind_count(ctx: &ReducerContext, kind: String, expected: u64) -> Result<(), String> {
    let actual = ctx.db.call_log().iter().filter(|row| row.kind == kind).count() as u64;
    if actual != expected {
        return Err(format!("expected kind '{kind}' count {expected}, got {actual}"));
    }
    Ok(())
}

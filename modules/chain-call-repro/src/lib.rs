use spacetimedb::{Identity, ReducerContext, SpacetimeType, Table};

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
        .map_err(|e| format!("call_b_from_a: call to B failed: {e}"))?;

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
        .map_err(|e| format!("call_c_from_b: call to C failed: {e}"))?;

    log_entry(ctx, "sent_to_c", &payload);
    Ok(())
}

// ---------------------------------------------------------------------------
// Cycle: A calls B, B calls back to A
// ---------------------------------------------------------------------------

/// Called on database A. Calls `cycle_b_calls_a` on database B, which calls back here.
#[spacetimedb::reducer]
pub fn cycle_a_calls_b(
    ctx: &ReducerContext,
    b_hex: String,
    a_hex: String,
    client_label: String,
    seq: u64,
    message: String,
    burn_iters: u64,
) -> Result<(), String> {
    burn(burn_iters);

    let payload = CallPayload {
        client_label,
        seq,
        message,
    };
    log_entry(ctx, "cycle_a_sent_to_b", &payload);

    let b = Identity::from_hex(&b_hex).expect("invalid B identity");
    let args = spacetimedb::spacetimedb_lib::bsatn::to_vec(&(
        a_hex,
        payload.clone(),
        burn_iters,
    ))
    .expect("failed to encode args for cycle_b_calls_a");
    spacetimedb::remote_reducer::call_reducer_on_db_2pc(b, "cycle_b_calls_a", &args)
        .map_err(|e| format!("cycle_a_calls_b: call to B failed: {e}"))?;

    Ok(())
}

/// Called on database B by `cycle_a_calls_b`. Calls `cycle_a_receives` back on A,
/// completing the A→B→A cycle.
#[spacetimedb::reducer]
pub fn cycle_b_calls_a(
    ctx: &ReducerContext,
    a_hex: String,
    payload: CallPayload,
    burn_iters: u64,
) -> Result<(), String> {
    burn(burn_iters);
    log_entry(ctx, "cycle_b_recv_from_a", &payload);

    let a = Identity::from_hex(&a_hex).expect("invalid A identity");
    let args = spacetimedb::spacetimedb_lib::bsatn::to_vec(&(payload.clone(), burn_iters))
        .expect("failed to encode args for cycle_a_receives");
    spacetimedb::remote_reducer::call_reducer_on_db_2pc(a, "cycle_a_receives", &args)
        .map_err(|e| format!("cycle_b_calls_a: call back to A failed: {e}"))?;

    log_entry(ctx, "cycle_b_sent_to_a", &payload);
    Ok(())
}

/// Terminal reducer on A, called by B to complete the cycle.
#[spacetimedb::reducer]
pub fn cycle_a_receives(
    ctx: &ReducerContext,
    payload: CallPayload,
    burn_iters: u64,
) -> Result<(), String> {
    burn(burn_iters);
    log_entry(ctx, "cycle_a_recv_from_b", &payload);
    Ok(())
}

// ---------------------------------------------------------------------------
// Busy-work reducers: no cross-database calls, just burn CPU and log
// ---------------------------------------------------------------------------

#[spacetimedb::reducer]
pub fn busy_work_small(ctx: &ReducerContext, client_label: String, seq: u64) -> Result<(), String> {
    burn(1_000);
    let payload = CallPayload {
        client_label,
        seq,
        message: "small".to_string(),
    };
    log_entry(ctx, "busy_small", &payload);
    Ok(())
}

#[spacetimedb::reducer]
pub fn busy_work_medium(ctx: &ReducerContext, client_label: String, seq: u64) -> Result<(), String> {
    burn(100_000);
    let payload = CallPayload {
        client_label,
        seq,
        message: "medium".to_string(),
    };
    log_entry(ctx, "busy_medium", &payload);
    Ok(())
}

#[spacetimedb::reducer]
pub fn busy_work_large(ctx: &ReducerContext, client_label: String, seq: u64) -> Result<(), String> {
    burn(10_000_000);
    let payload = CallPayload {
        client_label,
        seq,
        message: "large".to_string(),
    };
    log_entry(ctx, "busy_large", &payload);
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

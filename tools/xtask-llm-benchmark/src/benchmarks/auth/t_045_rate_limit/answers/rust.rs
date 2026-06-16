use spacetimedb::{reducer, table, Identity, ReducerContext, Table};

#[table(accessor = rate_limit)]
pub struct RateLimit {
    #[primary_key]
    pub identity: Identity,
    pub last_call_us: i64,
}

#[table(accessor = action_log, public)]
pub struct ActionLog {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    pub identity: Identity,
    pub payload: String,
}

#[reducer]
pub fn limited_action(ctx: &ReducerContext, payload: String) {
    let now = ctx.timestamp.to_micros_since_unix_epoch();
    if let Some(entry) = ctx.db.rate_limit().identity().find(ctx.sender()) {
        if now.saturating_sub(entry.last_call_us) < 1_000_000 {
            panic!("rate limited");
        }
        ctx.db.rate_limit().identity().update(RateLimit {
            identity: ctx.sender(),
            last_call_us: now,
        });
    } else {
        ctx.db.rate_limit().insert(RateLimit {
            identity: ctx.sender(),
            last_call_us: now,
        });
    }
    ctx.db.action_log().insert(ActionLog {
        id: 0,
        identity: ctx.sender(),
        payload,
    });
}

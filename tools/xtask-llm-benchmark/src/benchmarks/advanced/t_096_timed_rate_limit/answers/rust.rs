use spacetimedb::{reducer, table, Identity, ReducerContext, Table, Timestamp};
#[table(accessor = search_log, public, index(accessor = by_request, btree(columns = [owner, request_id])))]
pub struct SearchLog {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    pub owner: Identity,
    pub request_id: String,
    pub term: String,
}
#[table(accessor = search_bucket)]
pub struct SearchBucket {
    #[primary_key]
    pub owner: Identity,
    pub tokens: u32,
    pub refilled_at: Timestamp,
}
#[reducer]
pub fn log_search(ctx: &ReducerContext, request_id: String, term: String) -> Result<(), String> {
    if term.is_empty() {
        return Err("empty term".into());
    }
    if let Some(old) = ctx
        .db
        .search_log()
        .by_request()
        .filter((ctx.sender(), &request_id))
        .next()
    {
        return if old.term == term {
            Ok(())
        } else {
            Err("request conflict".into())
        };
    }
    let old = ctx.db.search_bucket().owner().find(ctx.sender());
    let exists = old.is_some();
    let mut bucket = old.unwrap_or(SearchBucket {
        owner: ctx.sender(),
        tokens: 3,
        refilled_at: ctx.timestamp,
    });
    let intervals =
        (ctx.timestamp.to_micros_since_unix_epoch() - bucket.refilled_at.to_micros_since_unix_epoch()) / 5_000_000;
    if intervals > 0 {
        bucket.tokens = (bucket.tokens as i64 + intervals).min(3) as u32;
        bucket.refilled_at = Timestamp::from_micros_since_unix_epoch(
            bucket.refilled_at.to_micros_since_unix_epoch() + intervals * 5_000_000,
        );
    }
    if bucket.tokens == 0 {
        return Err("rate limited".into());
    }
    bucket.tokens -= 1;
    if exists {
        ctx.db.search_bucket().owner().update(bucket);
    } else {
        ctx.db.search_bucket().insert(bucket);
    }
    ctx.db.search_log().insert(SearchLog {
        id: 0,
        owner: ctx.sender(),
        request_id,
        term,
    });
    Ok(())
}

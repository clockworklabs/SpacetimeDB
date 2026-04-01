use spacetimedb::{reducer, table, ReducerContext, Table};

const BUCKET_SIZE_MS: u64 = 1_000;

#[table(accessor = state, public)]
pub struct State {
    #[primary_key]
    pub id: i64,

    pub run_start_ms: u64,
    pub run_end_ms: u64,
    pub measure_start_ms: u64,
    pub measure_end_ms: u64,
    pub warehouse_count: u64,
}

#[table(accessor = txn_bucket, public)]
pub struct TxnBucket {
    #[primary_key]
    pub bucket_start_ms: u64,
    pub count: u64,
}

#[table(accessor = latency_bucket, public)]
pub struct LatencyBucket {
    #[primary_key]
    pub latency_ms: u16,
    pub count: u64,
}

fn clear_tables(ctx: &ReducerContext) {
    for row in ctx.db.state().iter() {
        ctx.db.state().id().delete(row.id);
    }

    for row in ctx.db.txn_bucket().iter() {
        ctx.db.txn_bucket().bucket_start_ms().delete(row.bucket_start_ms);
    }
}

#[reducer]
pub fn reset(
    ctx: &ReducerContext,
    warehouse_count: u64,
    warmup_duration_ms: u64,
    measure_start_ms: u64,
    measure_end_ms: u64,
) {
    clear_tables(ctx);

    let run_start_ms = measure_start_ms - warmup_duration_ms;
    let run_end_ms = measure_end_ms + warmup_duration_ms;

    ctx.db.state().insert(State {
        id: 0,
        run_start_ms,
        run_end_ms,
        measure_start_ms,
        measure_end_ms,
        warehouse_count,
    });
}

#[reducer]
pub fn record_txn(ctx: &ReducerContext, latency_ms: u16) {
    let current_time_ms = ctx.timestamp.to_duration_since_unix_epoch().unwrap().as_millis() as u64;
    let Some(state) = ctx.db.state().id().find(0) else {
        return;
    };

    let bucket_offset_ms = current_time_ms.saturating_sub(state.run_start_ms);
    let bucket_start_ms = state.run_start_ms + ((bucket_offset_ms / BUCKET_SIZE_MS) * BUCKET_SIZE_MS);

    if let Some(bucket) = ctx.db.txn_bucket().bucket_start_ms().find(bucket_start_ms) {
        ctx.db.txn_bucket().bucket_start_ms().update(TxnBucket {
            count: bucket.count.saturating_add(1),
            ..bucket
        });
    } else {
        ctx.db.txn_bucket().insert(TxnBucket {
            bucket_start_ms,
            count: 1,
        });
    }

    if let Some(bucket) = ctx.db.latency_bucket().latency_ms().find(latency_ms) {
        ctx.db.latency_bucket().latency_ms().update(LatencyBucket {
            count: bucket.count.saturating_add(1),
            ..bucket
        });
    } else {
        ctx.db.latency_bucket().insert(LatencyBucket { latency_ms, count: 1 });
    }
}

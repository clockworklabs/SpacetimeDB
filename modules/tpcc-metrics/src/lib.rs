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

#[table(accessor = txn, public)]
pub struct Txn {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    pub measurement_time_ms: u64,
    pub latency_ms: u16,
}

#[table(accessor = txn_bucket, public)]
pub struct TxnBucket {
    #[primary_key]
    pub bucket_start_ms: u64,
    pub count: u64,
}

fn clear_tables(ctx: &ReducerContext) {
    for row in ctx.db.state().iter() {
        ctx.db.state().id().delete(row.id);
    }

    for row in ctx.db.txn().iter() {
        ctx.db.txn().id().delete(row.id);
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

    ctx.db.state().insert(State {
        id: 0,
        run_start_ms: measure_start_ms - warmup_duration_ms,
        run_end_ms: measure_end_ms + warmup_duration_ms,
        measure_start_ms,
        measure_end_ms,
        warehouse_count,
    });
}

#[reducer]
pub fn clear_state(ctx: &ReducerContext) {
    clear_tables(ctx);
}

#[reducer]
pub fn record_txn(ctx: &ReducerContext, latency_ms: u16) {
    let current_time_ms = ctx.timestamp.to_duration_since_unix_epoch().unwrap().as_millis() as u64;

    ctx.db.txn().insert(Txn {
        id: 0,
        measurement_time_ms: current_time_ms,
        latency_ms,
    });
}

#[reducer]
pub fn record_txn_bucket(ctx: &ReducerContext) {
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
}

#[reducer]
pub fn record_txn_bucket_2(ctx: &ReducerContext, count: u64) {
    let current_time_ms = ctx.timestamp.to_duration_since_unix_epoch().unwrap().as_millis() as u64;
    let Some(state) = ctx.db.state().id().find(0) else {
        return;
    };

    let bucket_offset_ms = current_time_ms.saturating_sub(state.run_start_ms);
    let bucket_start_ms = state.run_start_ms + ((bucket_offset_ms / BUCKET_SIZE_MS) * BUCKET_SIZE_MS);

    if let Some(bucket) = ctx.db.txn_bucket().bucket_start_ms().find(bucket_start_ms) {
        ctx.db.txn_bucket().bucket_start_ms().update(TxnBucket {
            count: bucket.count.saturating_add(count),
            ..bucket
        });
    } else {
        ctx.db.txn_bucket().insert(TxnBucket {
            bucket_start_ms,
            count: count,
        });
    }
}

#[reducer]
pub fn record_txn_bucket_count(ctx: &ReducerContext, bucket_start_ms: u64, count: u64) {
    if count == 0 {
        return;
    }
    let Some(state) = ctx.db.state().id().find(0) else {
        return;
    };

    let bucket_offset_ms = bucket_start_ms.saturating_sub(state.run_start_ms);
    let bucket_start_ms = state.run_start_ms + ((bucket_offset_ms / BUCKET_SIZE_MS) * BUCKET_SIZE_MS);

    if let Some(bucket) = ctx.db.txn_bucket().bucket_start_ms().find(bucket_start_ms) {
        ctx.db.txn_bucket().bucket_start_ms().update(TxnBucket {
            count: bucket.count.saturating_add(count),
            ..bucket
        });
    } else {
        ctx.db.txn_bucket().insert(TxnBucket { bucket_start_ms, count });
    }
}

use spacetimedb::{reducer, table, ReducerContext, Table};

#[table(accessor = leased_job, public)]
pub struct LeasedJob {
    #[primary_key]
    pub id: u64,
    pub worker: String,
    pub generation: u64,
    pub expires_at: i64,
    pub done: bool,
    pub result: String,
}

#[reducer]
pub fn create_job(ctx: &ReducerContext, id: u64) -> Result<(), String> {
    ctx.db.leased_job().insert(LeasedJob {
        id,
        worker: String::new(),
        generation: 0,
        expires_at: 0,
        done: false,
        result: String::new(),
    });
    Ok(())
}

#[reducer]
pub fn claim(ctx: &ReducerContext, id: u64, worker: String, lease_ms: u64) -> Result<(), String> {
    if worker.is_empty() || lease_ms == 0 || lease_ms > 60000 {
        return Err("invalid lease".into());
    }
    let now = ctx.timestamp.to_micros_since_unix_epoch();
    let mut row = ctx.db.leased_job().id().find(id).ok_or("missing job")?;
    if row.done {
        return Err("already done".into());
    }
    if !row.worker.is_empty() && now < row.expires_at {
        return Err("lease busy".into());
    }
    row.generation += 1;
    row.worker = worker;
    row.expires_at = now + (lease_ms as i64) * 1000;
    ctx.db.leased_job().id().update(row);
    Ok(())
}

#[reducer]
pub fn complete(ctx: &ReducerContext, id: u64, worker: String, generation: u64, result: String) -> Result<(), String> {
    let mut row = ctx.db.leased_job().id().find(id).ok_or("missing job")?;
    if row.done {
        if row.worker == worker && row.generation == generation && row.result == result {
            return Ok(());
        }
        return Err("already done".into());
    }
    if worker.is_empty()
        || generation == 0
        || row.worker != worker
        || row.generation != generation
        || ctx.timestamp.to_micros_since_unix_epoch() >= row.expires_at
    {
        return Err("stale lease".into());
    }
    row.done = true;
    row.result = result;
    ctx.db.leased_job().id().update(row);
    Ok(())
}

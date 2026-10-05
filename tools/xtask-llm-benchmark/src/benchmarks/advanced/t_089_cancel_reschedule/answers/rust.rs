use spacetimedb::ScheduleAt;
use spacetimedb::{reducer, table, ReducerContext, Table};
#[table(accessor = timed_reservation, public)]
pub struct TimedReservation {
    #[primary_key]
    pub id: u64,
    pub generation: u64,
    pub status: String,
}

#[table(accessor = expiry_job, scheduled(expire_reservation))]
pub struct ExpiryJob {
    #[primary_key]
    #[auto_inc]
    pub scheduled_id: u64,
    pub scheduled_at: ScheduleAt,
    pub reservation_id: u64,
    pub generation: u64,
}

#[table(accessor = expiry_result, public)]
pub struct ExpiryResult {
    #[primary_key]
    pub scheduled_id: u64,
    pub reservation_id: u64,
    pub generation: u64,
    pub applied: bool,
}

#[reducer]
pub fn renew(ctx: &ReducerContext, id: u64, delay_ms: u64) -> Result<(), String> {
    if delay_ms == 0 || delay_ms > 60000 {
        return Err("invalid delay".into());
    }
    let generation = ctx.db.timed_reservation().id().find(id).map_or(1, |r| r.generation + 1);
    let next = TimedReservation {
        id,
        generation,
        status: "active".into(),
    };
    if ctx.db.timed_reservation().id().find(id).is_some() {
        ctx.db.timed_reservation().id().update(next);
    } else {
        ctx.db.timed_reservation().insert(next);
    }
    ctx.db.expiry_job().insert(ExpiryJob {
        scheduled_id: 0,
        scheduled_at: ScheduleAt::Time(ctx.timestamp + std::time::Duration::from_millis(delay_ms)),
        reservation_id: id,
        generation,
    });
    Ok(())
}

#[reducer]
pub fn cancel(ctx: &ReducerContext, id: u64) -> Result<(), String> {
    if let Some(mut row) = ctx.db.timed_reservation().id().find(id) {
        if row.status != "cancelled" {
            row.generation += 1;
            row.status = "cancelled".into();
            ctx.db.timed_reservation().id().update(row);
        }
    }
    Ok(())
}

#[reducer]
pub fn expire_reservation(ctx: &ReducerContext, job: ExpiryJob) -> Result<(), String> {
    if ctx.sender() != ctx.database_identity() {
        return Err("scheduler only".into());
    }
    if ctx.db.expiry_result().scheduled_id().find(job.scheduled_id).is_some() {
        return Ok(());
    }
    let mut applied = false;
    if let Some(mut row) = ctx.db.timed_reservation().id().find(job.reservation_id) {
        if row.status == "active" && row.generation == job.generation {
            row.status = "expired".into();
            ctx.db.timed_reservation().id().update(row);
            applied = true;
        }
    }
    ctx.db.expiry_result().insert(ExpiryResult {
        scheduled_id: job.scheduled_id,
        reservation_id: job.reservation_id,
        generation: job.generation,
        applied,
    });
    Ok(())
}

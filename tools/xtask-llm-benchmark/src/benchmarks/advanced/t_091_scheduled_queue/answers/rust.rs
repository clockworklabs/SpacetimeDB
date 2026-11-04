use spacetimedb::ScheduleAt;
use spacetimedb::{reducer, table, ReducerContext, Table};
#[table(accessor = queued_work, public)]
pub struct QueuedWork {
    #[primary_key]
    pub id: u64,
    pub input: i64,
    pub status: String,
    pub attempts: u64,
}

#[table(accessor = work_effect, public)]
pub struct WorkEffect {
    #[primary_key]
    pub id: u64,
    pub value: i64,
}

#[table(accessor = work_timer, scheduled(execute_work))]
pub struct WorkTimer {
    #[primary_key]
    #[auto_inc]
    pub scheduled_id: u64,
    pub scheduled_at: ScheduleAt,
    pub work_id: u64,
}

#[reducer]
pub fn enqueue(ctx: &ReducerContext, id: u64, input: i64) -> Result<(), String> {
    if let Some(old) = ctx.db.queued_work().id().find(id) {
        if old.input == input {
            return Ok(());
        }
        return Err("request conflict".into());
    }
    if !(-1000000..=1000000).contains(&input) {
        return Err("invalid input".into());
    }
    ctx.db.queued_work().insert(QueuedWork {
        id,
        input,
        status: "queued".into(),
        attempts: 0,
    });
    ctx.db.work_timer().insert(WorkTimer {
        scheduled_id: 0,
        scheduled_at: ScheduleAt::Time(ctx.timestamp + std::time::Duration::from_millis(1)),
        work_id: id,
    });
    Ok(())
}

#[reducer]
pub fn execute_work(ctx: &ReducerContext, job: WorkTimer) -> Result<(), String> {
    if ctx.sender() != ctx.database_identity() {
        return Err("scheduler only".into());
    }
    if let Some(mut row) = ctx.db.queued_work().id().find(job.work_id) {
        if row.status != "queued" {
            return Ok(());
        }
        row.attempts += 1;
        if row.input < 0 {
            row.status = "failed".into();
        } else {
            ctx.db.work_effect().insert(WorkEffect {
                id: row.id,
                value: row.input * 2,
            });
            row.status = "complete".into();
        }
        ctx.db.queued_work().id().update(row);
    }
    Ok(())
}

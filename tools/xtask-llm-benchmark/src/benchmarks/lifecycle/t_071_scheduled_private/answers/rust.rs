use spacetimedb::{reducer, table, ReducerContext, ScheduleAt, Table};
use std::time::Duration;

#[table(accessor = job_result, public)]
pub struct JobResult {
    #[primary_key]
    pub id: u64,
    pub status: String,
}

#[table(accessor = private_job, scheduled(run_private_job))]
pub struct PrivateJob {
    #[primary_key]
    #[auto_inc]
    pub scheduled_id: u64,
    pub scheduled_at: ScheduleAt,
    pub result_id: u64,
}

#[reducer]
pub fn enqueue_private(ctx: &ReducerContext, id: u64) {
    ctx.db.job_result().insert(JobResult {
        id,
        status: "queued".into(),
    });
    ctx.db.private_job().insert(PrivateJob {
        scheduled_id: 0,
        scheduled_at: ScheduleAt::Time(ctx.timestamp + Duration::from_millis(1)),
        result_id: id,
    });
}

#[reducer]
pub fn run_private_job(ctx: &ReducerContext, job: PrivateJob) {
    let mut result = ctx
        .db
        .job_result()
        .id()
        .find(job.result_id)
        .expect("job result missing");
    result.status = "complete".into();
    ctx.db.job_result().id().update(result);
}

use spacetimedb::{reducer, table, ReducerContext, ScheduleAt, Table};
use std::time::Duration;

#[table(accessor = work_item, public)]
pub struct WorkItem {
    #[primary_key]
    pub id: u64,
    #[index(btree)]
    pub group_id: u64,
}

#[table(accessor = delete_job, scheduled(run_delete_batch))]
pub struct DeleteJob {
    #[primary_key]
    #[auto_inc]
    pub scheduled_id: u64,
    pub scheduled_at: ScheduleAt,
    pub group_id: u64,
}

fn enqueue(ctx: &ReducerContext, group_id: u64) {
    ctx.db.delete_job().insert(DeleteJob {
        scheduled_id: 0,
        scheduled_at: ScheduleAt::Time(ctx.timestamp + Duration::from_millis(1)),
        group_id,
    });
}

#[reducer]
pub fn seed_group(ctx: &ReducerContext, group_id: u64, count: u32) {
    for offset in 0..count {
        ctx.db.work_item().insert(WorkItem { id: group_id * 100 + offset as u64, group_id });
    }
}

#[reducer]
pub fn request_delete(ctx: &ReducerContext, group_id: u64) { enqueue(ctx, group_id); }

#[reducer]
pub fn run_delete_batch(ctx: &ReducerContext, job: DeleteJob) {
    let rows: Vec<_> = ctx.db.work_item().group_id().filter(job.group_id).take(2).collect();
    for row in rows { ctx.db.work_item().id().delete(row.id); }
    if ctx.db.work_item().group_id().filter(job.group_id).next().is_some() { enqueue(ctx, job.group_id); }
}

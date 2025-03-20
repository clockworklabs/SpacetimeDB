from .. import Smoketest, random_string

class ClearDatabase(Smoketest):
    AUTOPUBLISH = False

    MODULE_CODE = """
use spacetimedb::{ReducerContext, Table, duration};

#[spacetimedb::table(name = counter, public)]
pub struct Counter {
    #[primary_key]
    id: u64,
    val: u64
}

#[spacetimedb::table(name = scheduled_counter, public, scheduled(inc, at = sched_at))]
pub struct ScheduledCounter {
    #[primary_key]
    #[auto_inc]
    scheduled_id: u64,
    sched_at: spacetimedb::ScheduleAt,
}

#[spacetimedb::reducer]
pub fn inc(ctx: &ReducerContext, arg: ScheduledCounter) {
    if let Some(mut counter) = ctx.db.counter().id().find(arg.scheduled_id) {
        counter.val += 1;
        ctx.db.counter().id().update(counter);
    } else {
        ctx.db.counter().insert(Counter {
            id: arg.scheduled_id,
            val: 1,
        });
    }
}

#[spacetimedb::reducer(init)]
pub fn init(ctx: &ReducerContext) {
    ctx.db.scheduled_counter().insert(ScheduledCounter {
        scheduled_id: 0,
        sched_at: duration!(100ms).into(),
    });
}
"""

    def test_publish_clear_database(self):
        """
        Test that publishing with the clear flag stops the old module.
        This relies on private control database internals.
        """

        name = random_string()
        self.publish_module(name, clear = False)
        self.publish_module(name, clear = True)
        self.spacetime("delete", name)

        deleted_replicas = self.query_control(f"select id from deleted_replica where database_identity = '0x{self.resolved_identity}'")
        # Both clear = True and delete should leave a deleted replica
        self.assertEqual(len(deleted_replicas), 2)
        state_filter = f'replica_id = {" OR replica_id = ".join(deleted_replicas)}'
        states = self.query_control(f"select lifecycle from replica_state where {state_filter}")
        # All replicas should have state 'Deleted'.
        all_deleted = all([x == "(Deleted = ())" for x in states])
        assert all_deleted

    def query_control(self, sql):
        out = self.spacetime("sql", "spacetime-control", sql)
        out = [line.strip() for line in out.splitlines()]
        out = out[2:] # Remove header
        return out

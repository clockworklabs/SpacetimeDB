from .. import Smoketest, random_string
import time

class DeleteDatabase(Smoketest):
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

    def test_delete_database(self):
        """
        Test that deleting a database stops the module.
        The module is considered stopped if its scheduled reducer stops
        producing update events.
        """

        name = random_string()
        self.publish_module(name, clear = False)
        sub = self.subscribe("select * from counter", n = 1000)
        time.sleep(2)
        self.spacetime("delete", name)

        updates = sub()
        # At a rate of 100ms, we shouldn't have more than 20 updates in 2secs.
        # But let's say 50, in case the delete gets delayed for some reason.
        assert len(updates) <= 50

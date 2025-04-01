from .. import Smoketest, random_string
import time


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

        # Initial publish
        replicas_1 = self.publish(name, clear = False)
        self.assertTrue(len(replicas_1) >= 1)

        # Publish with clear = True, destroying `replicas_1`
        replicas_2 = self.publish(name, clear = True)
        self.assertTrue(len(replicas_2) >= 1)
        self.assertNotEqual(replicas_1, replicas_2)

        # Delete the replicas created in the second publish
        self.spacetime("delete", name)

        # State updates don't happen instantly
        time.sleep(0.25)

        # Check that all replicas have state `Deleted`
        replicas = replicas_1 + replicas_2
        state_filter = f'replica_id = {" OR replica_id = ".join(replicas)}'
        states = self.query_control(f"select lifecycle from replica_state where {state_filter}")
        self.assertEqual(len(states), len(replicas))
        self.assertTrue(all([x == "(Deleted = ())" for x in states]))

    def publish(self, name, clear):
        self.publish_module(name, clear = clear)
        replicas = self.query_control(f"""
            select replica.id from replica
              join database on database.id = replica.database_id
             where database.database_identity = '0x{self.resolved_identity}'
        """)

        return replicas


    def query_control(self, sql):
        out = self.spacetime("sql", "spacetime-control", sql)
        out = [line.strip() for line in out.splitlines()]
        out = out[2:] # Remove header
        return out

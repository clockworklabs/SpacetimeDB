from .. import Smoketest
import time


class CancelReducer(Smoketest):

    MODULE_CODE = """
use spacetimedb::{duration, log, ReducerContext, Table};

#[spacetimedb::reducer(init)]
fn init(ctx: &ReducerContext) {
    let schedule = ctx.db.scheduled_reducer_args().insert(ScheduledReducerArgs {
        num: 1,
        scheduled_id: 0,
        scheduled_at: duration!(100ms).into(),
    });
    ctx.db.scheduled_reducer_args().scheduled_id().delete(&schedule.scheduled_id);

    let schedule = ctx.db.scheduled_reducer_args().insert(ScheduledReducerArgs {
         num: 2,
         scheduled_id: 0,
         scheduled_at: duration!(1000ms).into(),
     });
     do_cancel(ctx, schedule.scheduled_id);
}

#[spacetimedb::table(name = scheduled_reducer_args, public, scheduled(reducer))]
pub struct ScheduledReducerArgs {
    #[primary_key]
    #[auto_inc]
    scheduled_id: u64,
    scheduled_at: spacetimedb::ScheduleAt,
    num: i32,
}

#[spacetimedb::reducer]
fn do_cancel(ctx: &ReducerContext, schedule_id: u64) {
    ctx.db.scheduled_reducer_args().scheduled_id().delete(&schedule_id);
}

#[spacetimedb::reducer]
fn reducer(_ctx: &ReducerContext, args: ScheduledReducerArgs) {
    log::info!("the reducer ran: {}", args.num);
}
"""

    def test_cancel_reducer(self):
        """Ensure cancelling a reducer works"""

        time.sleep(2)
        logs = "\n".join(self.logs(5))
        self.assertNotIn("the reducer ran", logs)


TIMESTAMP_ZERO = {"__timestamp_micros_since_unix_epoch__": 0}


class SubscribeScheduledTable(Smoketest):
    MODULE_CODE = """
use spacetimedb::{log, duration, ReducerContext, Table, Timestamp};

#[spacetimedb::table(name = scheduled_table, public, scheduled(my_reducer, at = sched_at))]
pub struct ScheduledTable {
    #[primary_key]
    #[auto_inc]
    scheduled_id: u64,
    sched_at: spacetimedb::ScheduleAt,
    prev: Timestamp,
}

#[spacetimedb::reducer]
fn schedule_reducer(ctx: &ReducerContext) {
    ctx.db.scheduled_table().insert(ScheduledTable { prev: Timestamp::from_micros_since_unix_epoch(0), scheduled_id: 2, sched_at: Timestamp::from_micros_since_unix_epoch(0).into(), });
}

#[spacetimedb::reducer]
fn schedule_repeated_reducer(ctx: &ReducerContext) {
    ctx.db.scheduled_table().insert(ScheduledTable { prev: Timestamp::from_micros_since_unix_epoch(0), scheduled_id: 1, sched_at: duration!(100ms).into(), });
}

#[spacetimedb::reducer]
pub fn my_reducer(ctx: &ReducerContext, arg: ScheduledTable) {
    log::info!("Invoked: ts={:?}, delta={:?}", ctx.timestamp, ctx.timestamp.duration_since(arg.prev));
}
"""

    def test_scheduled_table_subscription(self):
        """This test deploys a module with a scheduled reducer and check if client receives subscription update for scheduled table entry and deletion of reducer once it ran"""
        # subscribe to empy scheduled_table
        sub = self.subscribe("SELECT * FROM scheduled_table", n=2)
        # call a reducer to schedule a reducer
        self.call("schedule_reducer")

        time.sleep(2)
        lines = sum(1 for line in self.logs(100) if "Invoked:" in line)
        # scheduled reducer should be ran by now
        self.assertEqual(lines, 1)

        row_entry = {
            "prev": TIMESTAMP_ZERO,
            "scheduled_id": 2,
            "sched_at": {"Time": TIMESTAMP_ZERO},
        }
        # subscription should have 2 updates, first for row insert in scheduled table and second for row deletion.
        self.assertEqual(
            sub(),
            [
                {"scheduled_table": {"deletes": [], "inserts": [row_entry]}},
                {"scheduled_table": {"deletes": [row_entry], "inserts": []}},
            ],
        )

    def test_scheduled_table_subscription_repeated_reducer(self):
        """This test deploys a module with a  repeated reducer and check if client receives subscription update for scheduled table entry and no delete entry"""
        # subscribe to emptry scheduled_table
        sub = self.subscribe("SELECT * FROM scheduled_table", n=2)
        # call a reducer to schedule a reducer
        self.call("schedule_repeated_reducer")

        time.sleep(2)
        lines = sum(1 for line in self.logs(100) if "Invoked:" in line)
        # repeated reducer should have run more than once.
        self.assertLess(2, lines)

        # scheduling repeated reducer again just to get 2nd subscription update.
        self.call("schedule_reducer")

        repeated_row_entry = {
            "prev": TIMESTAMP_ZERO,
            "scheduled_id": 1,
            "sched_at": {"Interval": {"__time_duration_micros__": 100000}},
        }
        row_entry = {
            "prev": TIMESTAMP_ZERO,
            "scheduled_id": 2,
            "sched_at": {"Time": TIMESTAMP_ZERO},
        }

        # subscription should have 2 updates and should not have any deletes
        self.assertEqual(
            sub(),
            [
                {"scheduled_table": {"deletes": [], "inserts": [repeated_row_entry]}},
                {"scheduled_table": {"deletes": [], "inserts": [row_entry]}},
            ],
        )


class VolatileNonatomicScheduleImmediate(Smoketest):
    BINDINGS_FEATURES = ["unstable"]
    MODULE_CODE = """
use spacetimedb::{ReducerContext, Table};

#[spacetimedb::table(name = my_table, public)]
pub struct MyTable {
    x: String,
}

#[spacetimedb::reducer]
fn do_schedule(_ctx: &ReducerContext) {
    spacetimedb::volatile_nonatomic_schedule_immediate!(do_insert("hello".to_owned()));
}

#[spacetimedb::reducer]
fn do_insert(ctx: &ReducerContext, x: String) {
    ctx.db.my_table().insert(MyTable { x });
}
"""

    def test_volatile_nonatomic_schedule_immediate(self):
        """Check that volatile_nonatomic_schedule_immediate works"""

        sub = self.subscribe("SELECT * FROM my_table", n=2)

        self.call("do_insert", "yay!")
        self.call("do_schedule")

        self.assertEqual(
            sub(),
            [
                {"my_table": {"deletes": [], "inserts": [{"x": "yay!"}]}},
                {"my_table": {"deletes": [], "inserts": [{"x": "hello"}]}},
            ],
        )

from .. import Smoketest
import time

class CancelReducer(Smoketest):

    MODULE_CODE = """
    use spacetimedb::{duration, println, spacetimedb, spacetimedb_lib::ScheduleAt, ReducerContext};

#[spacetimedb::reducer(init)]
fn init() {
    let schedule = ScheuledReducerArgs::insert(ScheuledReducerArgs {
        num: 1,
        scheduled_id: 0,
        scheduled_at: duration!(100ms).into(),
    });
    ScheuledReducerArgs::delete_by_scheduled_id(&schedule.unwrap().scheduled_id);

    let schedule = ScheuledReducerArgs::insert(ScheuledReducerArgs {
         num: 2,
         scheduled_id: 0,
         scheduled_at: duration!(1000ms).into(),
     });
     do_cancel(schedule.unwrap().scheduled_id);
}

#[spacetimedb::table(public, scheduled(reducer))]
pub struct ScheuledReducerArgs {
    num: i32,
}

#[spacetimedb::reducer]
fn do_cancel(schedule_id: u64) {
    ScheuledReducerArgs::delete_by_scheduled_id(&schedule_id);
}

#[spacetimedb::reducer]
fn reducer(_ctx: ReducerContext, args: ScheuledReducerArgs) {
    println!("the reducer ran: {}", args.num);
}
"""

    def test_cancel_reducer(self):
        """Ensure cancelling a reducer works"""

        time.sleep(2)
        logs = "\n".join(self.logs(5))
        self.assertNotIn("the reducer ran", logs)


class SubscribeScheduledTable(Smoketest):
    MODULE_CODE = """
use spacetimedb::{println, duration, spacetimedb, Timestamp, spacetimedb_lib::ScheduleAt, ReducerContext};


#[spacetimedb::table(public, scheduled(my_reducer))]
pub struct ScheduledTable {
    prev: Timestamp,
}

#[spacetimedb::reducer]
fn schedule_reducer() {
    let _ = ScheduledTable::insert(ScheduledTable { prev: Timestamp::from_micros_since_epoch(0), scheduled_id: 2, scheduled_at: Timestamp::from_micros_since_epoch(0).into(), });
}

#[spacetimedb::reducer]
fn schedule_repeated_reducer() {
    let _ = ScheduledTable::insert(ScheduledTable { prev: Timestamp::from_micros_since_epoch(0), scheduled_id: 1, scheduled_at: duration!(100ms).into(), });
}

#[spacetimedb::reducer]
pub fn my_reducer(_ctx: ReducerContext, arg: ScheduledTable) {
    println!("Invoked: ts={:?}, delta={:?}", Timestamp::now(), arg.prev.elapsed());
}
"""
    def test_scheduled_table_subscription(self):
        """This test deploys a module with a scheduled reducer and check if client receives subscription update for scheduled table entry and deletion of reducer once it ran"""
        # subscribe to empy ScheduledTable
        sub = self.subscribe("SELECT * FROM ScheduledTable", n=2)
        # call a reducer to schedule a reducer
        self.call("schedule_reducer")

        time.sleep(2)
        lines = sum(1 for line in self.logs(100) if "Invoked:" in line)
        # scheduled reducer should be ran by now
        self.assertEqual(lines, 1)

        row_entry = {'prev': 0, 'scheduled_id': 2, 'scheduled_at': {'Time': 0}}
        # subscription should have 2 updates, first for row insert in scheduled table and second for row deletion.
        self.assertEqual(sub(), [{'ScheduledTable': {'deletes': [], 'inserts': [row_entry]}}, {'ScheduledTable': {'deletes': [row_entry], 'inserts': []}}])



    def test_scheduled_table_subscription_repeated_reducer(self):
        """This test deploys a module with a  repeated reducer and check if client receives subscription update for scheduled table entry and no delete entry"""
        # subscribe to emptry ScheduledTable
        sub = self.subscribe("SELECT * FROM ScheduledTable", n=2)
        # call a reducer to schedule a reducer
        self.call("schedule_repeated_reducer")

        time.sleep(2)
        lines = sum(1 for line in self.logs(100) if "Invoked:" in line)
        # repeated reducer should have run more than once.
        self.assertLess(2, lines)

        # scheduling repeated reducer again just to get 2nd subscription update.
        self.call("schedule_reducer")

        repeated_row_entry = {'prev': 0, 'scheduled_id': 1, 'scheduled_at': {'Interval': 100000}}
        row_entry = {'prev': 0, 'scheduled_id': 2, 'scheduled_at': {'Time': 0}}

        # subscription should have 2 updates and should not have any deletes
        self.assertEqual(sub(), [{'ScheduledTable': {'deletes': [], 'inserts': [repeated_row_entry]}}, {'ScheduledTable': {'deletes': [], 'inserts': [row_entry]}}])


class VolatileNonatomicScheduleImmediate(Smoketest):
    BINDINGS_FEATURES = ["unstable_abi"]
    MODULE_CODE = """
use spacetimedb::spacetimedb;

#[spacetimedb::table(public)]
pub struct MyTable {
    x: String,
}

#[spacetimedb::reducer]
fn do_schedule() {
    spacetimedb::volatile_nonatomic_schedule_immediate!(do_insert("hello".to_owned()));
}

#[spacetimedb::reducer]
fn do_insert(x: String) {
    MyTable::insert(MyTable { x });
}
"""
    def test_volatile_nonatomic_schedule_immediate(self):
        """Check that volatile_nonatomic_schedule_immediate works"""

        sub = self.subscribe("SELECT * FROM MyTable", n=2)

        self.call("do_insert", "yay!")
        self.call("do_schedule")

        self.assertEqual(sub(), [{'MyTable': {'deletes': [], 'inserts': [{'x': 'yay!'}]}}, {'MyTable': {'deletes': [], 'inserts': [{'x': 'hello'}]}}])

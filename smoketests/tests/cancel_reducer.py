from .. import Smoketest
import time

class CancelReducer(Smoketest):
    
    MODULE_CODE = """
    use spacetimedb::{duration, println, spacetimedb, spacetimedb_lib::ScheduleAt, ReducerContext};

#[spacetimedb(init)]
fn init() {
    let schedule = ScheuledReducerArgs::insert(ScheuledReducerArgs {
        num: 1,
        scheduled_id: 0,
        scheduled_at: ScheduleAt::Interval(duration!(100ms).into()),
    });
    ScheuledReducerArgs::delete_by_scheduled_id(&schedule.unwrap().scheduled_id);

    let schedule = ScheuledReducerArgs::insert(ScheuledReducerArgs {
         num: 2,
         scheduled_id: 0,
         scheduled_at: ScheduleAt::Interval(duration!(1000ms).into()),
     });
     do_cancel(schedule.unwrap().scheduled_id);
}

#[spacetimedb(table(public), scheduled(reducer))]
pub struct ScheuledReducerArgs {
    num: i32,
}

#[spacetimedb(reducer)]
fn do_cancel(schedule_id: u64) {
    ScheuledReducerArgs::delete_by_scheduled_id(&schedule_id);
}

#[spacetimedb(reducer)]
fn reducer(_ctx: ReducerContext, args: ScheuledReducerArgs) {
    println!("the reducer ran: {}", args.num);
}
"""

    def test_cancel_reducer(self):
        """Ensure cancelling a reducer works"""

        time.sleep(2)
        logs = "\n".join(self.logs(5))
        self.assertNotIn("the reducer ran", logs)

from .. import Smoketest
import time

class CancelReducer(Smoketest):
    
    MODULE_CODE = """
use spacetimedb::{println, spacetimedb, ScheduleToken};

#[spacetimedb(init)]
fn init() {
    let token = spacetimedb::schedule!("100ms", reducer(1));
    token.cancel();
    let token = spacetimedb::schedule!("1000ms", reducer(2));
    spacetimedb::schedule!("500ms", do_cancel(token));
}

#[spacetimedb(reducer)]
fn do_cancel(token: ScheduleToken<reducer>) {
    token.cancel()
}

#[spacetimedb(reducer)]
fn reducer(num: i32) {
    println!("the reducer ran: {}", num)
}
"""

    def test_cancel_reducer(self):
        """Ensure cancelling a reducer works"""

        time.sleep(2)
        logs = "\n".join(self.logs(5))
        self.assertNotIn("the reducer ran", logs)

from .. import Smoketest

class Panic(Smoketest):
    MODULE_CODE = """
use spacetimedb::{log, ReducerContext};
use std::cell::RefCell;

thread_local! {
    static X: RefCell<u32> = RefCell::new(0);
}
#[spacetimedb::reducer]
fn first(_ctx: &ReducerContext) {
    X.with(|x| {
        let _x = x.borrow_mut();
        panic!()
    })
}
#[spacetimedb::reducer]
fn second(_ctx: &ReducerContext) {
    X.with(|x| *x.borrow_mut());
    log::info!("Test Passed");
}
"""

    def test_panic(self):
        """Tests to check if a SpacetimeDB module can handle a panic without corrupting"""

        with self.assertRaises(Exception):
            self.call("first")
        
        self.call("second")
        self.assertIn("Test Passed", self.logs(2))

class ReducerError(Smoketest):
    MODULE_CODE = """
use spacetimedb::ReducerContext;

#[spacetimedb::reducer]
fn fail(_ctx: &ReducerContext) -> Result<(), String> {
    Err("oopsie :(".into())
}
"""

    def test_reducer_error_message(self):
        """Tests to ensure an error message returned from a reducer gets printed to logs"""

        with self.assertRaises(Exception):
            self.call("fail")

        self.assertIn("oopsie :(", self.logs(2))

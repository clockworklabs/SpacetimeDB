from .. import Smoketest

class Panic(Smoketest):
    MODULE_CODE = """
use spacetimedb::{println, ReducerContext};
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
    println!("Test Passed");
}
"""

    def test_panic(self):
        """Tests to check if a SpacetimeDB module can handle a panic without corrupting"""

        with self.assertRaises(Exception):
            self.call("first")
        
        self.call("second")
        self.assertIn("Test Passed", self.logs(2))
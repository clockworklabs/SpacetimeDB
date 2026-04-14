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

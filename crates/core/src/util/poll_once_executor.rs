//! An `async` executor which polls a future only once, and cancels if it does not immediately return `Ready`.
//!
//! This is useful for running library code which is typed as `async`,
//! but which we know based on our specific invocation should never yield.
//! In our case, we configure Wasmtime in `async` mode in order to execute procedures,
//! but we maintain the invariant that instantiation and reducers will never yield
//! 

use std::{
    future::Future,
    pin::pin,
    sync::Arc,
    task::{Context, Poll, Wake, Waker},
};

struct TerribleWaker;

impl Wake for TerribleWaker {
    fn wake(self: Arc<Self>) {}
}

pub fn poll_once<Res, Fut: Future<Output = Res>>(mut fut: Fut) -> Option<Res> {
    let waker = Waker::from(Arc::new(TerribleWaker));
    let mut context = Context::from_waker(&waker);
    match <Fut as Future>::poll(pin!(&mut fut), &mut context) {
        Poll::Pending => None,
        Poll::Ready(res) => Some(res),
    }
}

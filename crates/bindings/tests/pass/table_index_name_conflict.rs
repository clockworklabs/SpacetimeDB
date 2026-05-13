// This file tests that it's possible to have a value item (`fn`, `const`, or `static`) named `index`
// without introducing a name conflict due to a binding introduced by the `#[table]` macro.
// Prior to a fix, the SATS derive macros (which were invoked by `table`) introduced some bindings
// which were not in the `__` reserved namespace and had common names,
// resulting in name collisions with user code.

use spacetimedb::http::{HandlerContext, Request, Response};

#[spacetimedb::http::handler]
fn index(_ctx: &mut HandlerContext, _req: Request) -> Response {
    Response::new(().into())
}

#[spacetimedb::table(accessor = things)]
struct Thing {
    #[index(btree)]
    value: u32,
}

fn main() {}

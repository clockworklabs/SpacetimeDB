#![deny(warnings)]

use spacetimedb::http::{Request, Response};

#[spacetimedb::http::handler]
fn lowercase_handler(_ctx: &mut spacetimedb::HandlerContext, _req: Request) -> Response {
    Response::new(().into())
}

fn main() {}

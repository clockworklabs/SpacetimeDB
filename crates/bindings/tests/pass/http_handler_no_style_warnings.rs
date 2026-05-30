#![deny(warnings)]

use spacetimedb::http::{HandlerContext, Request, Response};

#[spacetimedb::http::handler]
fn lowercase_handler(_ctx: &mut HandlerContext, _req: Request) -> Response {
    Response::new(().into())
}

fn main() {}

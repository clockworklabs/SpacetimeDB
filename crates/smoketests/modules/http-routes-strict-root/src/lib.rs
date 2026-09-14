use spacetimedb::http::{Body, HandlerContext, Request, Response, Router};

#[spacetimedb::http::handler]
fn empty_root(_ctx: &mut HandlerContext, _req: Request) -> Response {
    Response::new(Body::from_bytes("empty"))
}

#[spacetimedb::http::handler]
fn slash_root(_ctx: &mut HandlerContext, _req: Request) -> Response {
    Response::new(Body::from_bytes("slash"))
}

#[spacetimedb::http::handler]
fn foo(_ctx: &mut HandlerContext, _req: Request) -> Response {
    Response::new(Body::from_bytes("foo"))
}

#[spacetimedb::http::handler]
fn foo_slash(_ctx: &mut HandlerContext, _req: Request) -> Response {
    Response::new(Body::from_bytes("foo-slash"))
}

#[spacetimedb::http::router]
fn router() -> Router {
    Router::new()
        .get("", empty_root)
        .get("/", slash_root)
        .get("/foo", foo)
        .get("/foo/", foo_slash)
}

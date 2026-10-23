use spacetimedb::http::{Body, HandlerContext, Request, Response, Router};

#[spacetimedb::http::handler]
fn echo_uri(_ctx: &mut HandlerContext, req: Request) -> Response {
    Response::new(Body::from_bytes(req.uri().to_string()))
}

#[spacetimedb::http::router]
fn router() -> Router {
    Router::new().get("/echo-uri", echo_uri)
}

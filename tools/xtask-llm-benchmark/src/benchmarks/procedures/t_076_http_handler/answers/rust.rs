use spacetimedb::http::{handler, router, Body, HandlerContext, Request, Response, Router};

#[handler]
fn echo(_ctx: &mut HandlerContext, request: Request) -> Response {
    let body = request.into_body().into_string_lossy();
    Response::builder().status(201).header("content-type", "text/plain")
        .body(Body::from_bytes(format!("echo:{body}"))).unwrap()
}

#[router]
fn routes() -> Router { Router::new().post("/echo", echo) }

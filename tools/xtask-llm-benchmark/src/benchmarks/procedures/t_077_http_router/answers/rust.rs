use spacetimedb::http::{handler, router, Body, HandlerContext, Request, Response, Router};

#[handler]
fn list_items(_ctx: &mut HandlerContext, _request: Request) -> Response { Response::new(Body::from_bytes("list")) }

#[handler]
fn create_item(_ctx: &mut HandlerContext, request: Request) -> Response {
    let body = request.into_body().into_string_lossy();
    Response::builder().status(201).body(Body::from_bytes(format!("created:{body}"))).unwrap()
}

#[router]
fn routes() -> Router { Router::new().get("/items", list_items).post("/items", create_item) }

use spacetimedb::http::{Body, HandlerContext, Request, Response, Router};

#[spacetimedb::http::handler]
fn reverse_bytes(_ctx: &mut HandlerContext, req: Request) -> Response {
    let mut reversed = req.into_body().into_bytes().to_vec();
    reversed.reverse();
    Response::new(Body::from_bytes(reversed))
}

#[spacetimedb::http::handler]
fn reverse_words(_ctx: &mut HandlerContext, req: Request) -> Response {
    let body = match req.into_body().into_string() {
        Ok(body) => body,
        Err(_) => {
            return Response::builder()
                .status(400)
                .body(Body::from_bytes("request body must be valid UTF-8"))
                .expect("response builder should not fail");
        }
    };

    let reversed = body.split(' ').rev().collect::<Vec<_>>().join(" ");
    Response::new(Body::from_bytes(reversed))
}

#[spacetimedb::http::router]
fn router() -> Router {
    Router::new()
        .post("/reverse-bytes", reverse_bytes)
        .post("/reverse-words", reverse_words)
}

use spacetimedb::http::{Body, HandlerContext, Request, Response, Router};
use spacetimedb::{log, ReducerContext, Table, ViewContext};

#[spacetimedb::env]
pub struct Env {
    #[env(values("en", "fr"))]
    pub LANGUAGE: Option<String>,
}

#[spacetimedb::table(accessor = person)]
pub struct Person {
    name: String,
}

#[spacetimedb::reducer]
pub fn add(ctx: &ReducerContext, name: String) {
    ctx.db.person().insert(Person { name });
}

#[spacetimedb::reducer]
pub fn say_hello(ctx: &ReducerContext) {
    for person in ctx.db.person().iter() {
        log::info!("Hello, {}!", person.name);
    }
    log::info!("Hello, World!");
}

#[spacetimedb::view(accessor = nobody, public)]
pub fn nobody(_ctx: &ViewContext) -> Option<Person> {
    None
}

#[spacetimedb::http::handler]
fn health(_ctx: &mut HandlerContext, _req: Request) -> Response {
    Response::new(Body::from_bytes("ok"))
}

#[spacetimedb::http::router]
fn router() -> Router {
    Router::new().get("/health", health)
}

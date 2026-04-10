use spacetimedb_smoketests::Smoketest;

const MODULE_CODE: &str = r#"
use spacetimedb::http::{Body, Request, Response, Router};
use spacetimedb::HandlerContext;
use spacetimedb::Table;

#[spacetimedb::table(accessor = entries, public)]
pub struct Entry {
    id: u64,
    value: String,
}

#[spacetimedb::http::handler]
fn get_simple(_ctx: &mut HandlerContext, _req: Request) -> Response {
    Response::new(Body::from_bytes("ok"))
}

#[spacetimedb::http::handler]
fn post_insert(ctx: &mut HandlerContext, _req: Request) -> Response {
    ctx.with_tx(|tx| {
        let id = tx.db.entries().iter().count() as u64;
        tx.db.entries().insert(Entry {
            id,
            value: "posted".to_string(),
        });
    });
    Response::new(Body::from_bytes("inserted"))
}

#[spacetimedb::http::handler]
fn get_count(ctx: &mut HandlerContext, _req: Request) -> Response {
    let count = ctx.with_tx(|tx| tx.db.entries().iter().count());
    Response::new(Body::from_bytes(count.to_string()))
}

#[spacetimedb::http::handler]
fn any_handler(_ctx: &mut HandlerContext, _req: Request) -> Response {
    Response::new(Body::from_bytes("any"))
}

#[spacetimedb::http::handler]
fn header_echo(_ctx: &mut HandlerContext, req: Request) -> Response {
    let value = req
        .headers()
        .get("x-echo")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("");
    Response::new(Body::from_bytes(value.to_string()))
}

#[spacetimedb::http::handler]
fn set_response_header(_ctx: &mut HandlerContext, _req: Request) -> Response {
    Response::builder()
        .header("x-response", "set")
        .body(Body::from_bytes("header-set"))
        .expect("response builder should not fail")
}

#[spacetimedb::http::handler]
fn body_handler(_ctx: &mut HandlerContext, _req: Request) -> Response {
    Response::new(Body::from_bytes("non-empty"))
}

#[spacetimedb::http::handler]
fn teapot(_ctx: &mut HandlerContext, _req: Request) -> Response {
    Response::builder()
        .status(418)
        .body(Body::from_bytes("teapot"))
        .expect("response builder should not fail")
}

#[spacetimedb::http::router]
fn router() -> Router {
    Router::new()
        .get("/get", get_simple)
        .post("/post", post_insert)
        .get("/count", get_count)
        .any("/any", any_handler)
        .get("/header", header_echo)
        .get("/set-header", set_response_header)
        .get("/body", body_handler)
        .get("/teapot", teapot)
}
"#;

const EXAMPLE_MODULE_CODE: &str = r#"
use std::str::FromStr;

use spacetimedb::http::{Body, Request, Response, Router};
use spacetimedb::{HandlerContext, Table};

#[spacetimedb::table(accessor = data)]
struct Data {
    #[primary_key]
    #[auto_inc]
    id: u64,
    body: Vec<u8>,
}

#[spacetimedb::http::handler]
fn insert(ctx: &mut HandlerContext, request: Request) -> Response {
    let body: Vec<u8> = request.into_body().into_bytes().into();
    let id = ctx.with_tx(|tx| tx.db.data().insert(Data { id: 0, body: body.clone() }).id);
    Response::new(Body::from_bytes(format!("{id}")))
}

#[spacetimedb::http::handler]
fn retrieve(ctx: &mut HandlerContext, request: Request) -> Response {
    let id = request
        .uri()
        .query()
        .and_then(|query| query.strip_prefix("id="))
        .and_then(|id| u64::from_str(id).ok())
        .unwrap();
    let body = ctx.with_tx(|tx| tx.db.data().id().find(id).map(|data| data.body));
    if let Some(body) = body {
        Response::new(Body::from_bytes(body))
    } else {
        Response::builder().status(404).body(Body::empty()).unwrap()
    }
}

#[spacetimedb::http::router]
fn router() -> Router {
    Router::new().post("/insert", insert).get("/retrieve", retrieve)
}
"#;

const NO_SUCH_ROUTE_BODY: &str = "Database has not registered a handler for this route";

#[test]
fn http_routes_end_to_end() {
    let test = Smoketest::builder().module_code(MODULE_CODE).build();
    let identity = test.database_identity.as_ref().expect("database identity missing");

    let base = format!("{}/v1/database/{}/route", test.server_url, identity);
    let client = reqwest::blocking::Client::new();

    let resp = client.get(format!("{base}/get")).send().expect("get failed");
    assert!(resp.status().is_success());
    assert_eq!(resp.text().expect("get body"), "ok");

    let resp = client
        .post(format!("{base}/post"))
        .body("payload")
        .send()
        .expect("post failed");
    assert!(resp.status().is_success());

    let resp = client.get(format!("{base}/count")).send().expect("count failed");
    assert!(resp.status().is_success());
    assert_eq!(resp.text().expect("count body"), "1");

    let resp = client.put(format!("{base}/any")).send().expect("any failed");
    assert!(resp.status().is_success());
    assert_eq!(resp.text().expect("any body"), "any");

    let resp = client
        .get(format!("{base}/header"))
        .header("x-echo", "hello")
        .send()
        .expect("header echo failed");
    assert!(resp.status().is_success());
    assert_eq!(resp.text().expect("header body"), "hello");

    let resp = client
        .get(format!("{base}/set-header"))
        .send()
        .expect("set-header failed");
    assert!(resp.status().is_success());
    assert_eq!(
        resp.headers().get("x-response").and_then(|value| value.to_str().ok()),
        Some("set")
    );

    let resp = client.get(format!("{base}/body")).send().expect("body failed");
    assert!(resp.status().is_success());
    assert_eq!(resp.text().expect("body text"), "non-empty");

    let resp = client.get(format!("{base}/teapot")).send().expect("teapot failed");
    assert_eq!(resp.status().as_u16(), 418);

    let resp = client
        .get(format!("{base}/missing"))
        .send()
        .expect("missing route failed");
    assert_eq!(resp.status().as_u16(), 404);
    assert_eq!(resp.text().expect("missing route body"), NO_SUCH_ROUTE_BODY);

    let resp = client
        .get(format!(
            "{}/v1/database/{}/schema?version=10",
            test.server_url, identity
        ))
        .header("authorization", "Bearer not-a-jwt")
        .send()
        .expect("schema request failed");
    assert!(resp.status().is_client_error());

    let resp = client
        .get(format!("{base}/get"))
        .header("authorization", "Bearer not-a-jwt")
        .send()
        .expect("route request failed");
    assert!(resp.status().is_success());
}

#[test]
fn http_routes_pr_example_round_trip() {
    let test = Smoketest::builder().module_code(EXAMPLE_MODULE_CODE).build();
    let identity = test.database_identity.as_ref().expect("database identity missing");

    let base = format!("{}/v1/database/{}/route", test.server_url, identity);
    let client = reqwest::blocking::Client::new();
    let payload = b"hello from the PR example".to_vec();

    let resp = client
        .post(format!("{base}/insert"))
        .body(payload.clone())
        .send()
        .expect("insert failed");
    assert!(resp.status().is_success());
    let inserted_id = resp.text().expect("insert id body");

    let resp = client
        .get(format!("{base}/retrieve?id={inserted_id}"))
        .send()
        .expect("retrieve existing failed");
    assert!(resp.status().is_success());
    assert_eq!(
        resp.bytes().expect("retrieve existing body").as_ref(),
        payload.as_slice()
    );

    let resp = client
        .get(format!("{base}/retrieve?id=999999999"))
        .send()
        .expect("retrieve missing failed");
    assert_eq!(resp.status().as_u16(), 404);

    let resp = client
        .get(format!("{base}/retrieve?id=not-a-u64"))
        .send()
        .expect("retrieve invalid failed");
    assert!(resp.status().is_server_error());
}

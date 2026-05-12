use regex::Regex;
use spacetimedb_smoketests::{require_pnpm, workspace_root, Smoketest};
use std::{fs, path::Path};

const MODULE_CODE: &str = r#"
use spacetimedb::http::{Body, HandlerContext, Request, Response, Router};
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

use spacetimedb::http::{Body, HandlerContext, Request, Response, Router};
use spacetimedb::Table;

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

const STRICT_ROOT_ROUTING_MODULE_CODE: &str = r#"
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
"#;

const STRICT_NON_ROOT_ROUTING_MODULE_CODE: &str = r#"
use spacetimedb::http::{Body, HandlerContext, Request, Response, Router};

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
        .get("/foo", foo)
        .get("/foo/", foo_slash)
}
"#;

const FULL_URI_MODULE_CODE: &str = r#"
use spacetimedb::http::{Body, HandlerContext, Request, Response, Router};

#[spacetimedb::http::handler]
fn echo_uri(_ctx: &mut HandlerContext, req: Request) -> Response {
    Response::new(Body::from_bytes(req.uri().to_string()))
}

#[spacetimedb::http::router]
fn router() -> Router {
    Router::new().get("/echo-uri", echo_uri)
}
"#;

const HANDLE_REQUEST_BODY_MODULE_CODE: &str = r#"
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
"#;

const TS_MODULE_CODE: &str = r#"import { Router, SyncResponse, schema, table, t } from "spacetimedb/server";

const entry = table(
  { name: "entry", public: true },
  {
    id: t.u64().primaryKey(),
    value: t.string(),
  }
);

const spacetimedb = schema({ entry });
export default spacetimedb;

export const get_simple = spacetimedb.httpHandler((_ctx, _req) =>
  new SyncResponse("ok")
);

export const post_insert = spacetimedb.httpHandler((ctx, _req) => {
  ctx.withTx(tx => {
    const id = BigInt(tx.db.entry.count());
    tx.db.entry.insert({ id, value: "posted" });
  });
  return new SyncResponse("inserted");
});

export const get_count = spacetimedb.httpHandler((ctx, _req) => {
  const count = ctx.withTx(tx => tx.db.entry.count());
  return new SyncResponse(String(count));
});

export const any_handler = spacetimedb.httpHandler((_ctx, _req) =>
  new SyncResponse("any")
);

export const header_echo = spacetimedb.httpHandler((_ctx, req) =>
  new SyncResponse(req.headers.get("x-echo") ?? "")
);

export const set_response_header = spacetimedb.httpHandler((_ctx, _req) =>
  new SyncResponse("header-set", { headers: { "x-response": "set" } })
);

export const body_handler = spacetimedb.httpHandler((_ctx, _req) =>
  new SyncResponse("non-empty")
);

export const teapot = spacetimedb.httpHandler((_ctx, _req) =>
  new SyncResponse("teapot", { status: 418 })
);

export const router = spacetimedb.httpRouter(
  new Router()
    .get("/get", get_simple)
    .post("/post", post_insert)
    .get("/count", get_count)
    .any("/any", any_handler)
    .get("/header", header_echo)
    .get("/set-header", set_response_header)
    .get("/body", body_handler)
    .get("/teapot", teapot)
);
"#;

const TS_EXAMPLE_MODULE_CODE: &str = r#"import { Router, SyncResponse, schema, table, t } from "spacetimedb/server";

const data = table(
  { name: "data" },
  {
    id: t.u64().primaryKey().autoInc(),
    body: t.array(t.u8()),
  }
);

const spacetimedb = schema({ data });
export default spacetimedb;

export const insert = spacetimedb.httpHandler((ctx, req) => {
  const body = Array.from(req.bytes());
  const id = ctx.withTx(tx => tx.db.data.insert({ id: 0n, body }).id);
  return new SyncResponse(String(id));
});

export const retrieve = spacetimedb.httpHandler((ctx, req) => {
  const query = req.uri.split("?", 2)[1] ?? "";
  const idText = query.startsWith("id=") ? query.slice(3) : "";
  const id = BigInt(idText);
  const body = ctx.withTx(tx => tx.db.data.id.find(id)?.body);
  if (body != null) {
    return new SyncResponse(new Uint8Array(body));
  }
  return new SyncResponse(null, { status: 404 });
});

export const router = spacetimedb.httpRouter(
  new Router().post("/insert", insert).get("/retrieve", retrieve)
);
"#;

const TS_STRICT_ROOT_ROUTING_MODULE_CODE: &str = r#"import { Router, SyncResponse, schema } from "spacetimedb/server";

const spacetimedb = schema({});
export default spacetimedb;

export const empty_root = spacetimedb.httpHandler((_ctx, _req) =>
  new SyncResponse("empty")
);

export const slash_root = spacetimedb.httpHandler((_ctx, _req) =>
  new SyncResponse("slash")
);

export const foo = spacetimedb.httpHandler((_ctx, _req) =>
  new SyncResponse("foo")
);

export const foo_slash = spacetimedb.httpHandler((_ctx, _req) =>
  new SyncResponse("foo-slash")
);

export const router = spacetimedb.httpRouter(
  new Router()
    .get("", empty_root)
    .get("/", slash_root)
    .get("/foo", foo)
    .get("/foo/", foo_slash)
);
"#;

const TS_STRICT_NON_ROOT_ROUTING_MODULE_CODE: &str = r#"import { Router, SyncResponse, schema } from "spacetimedb/server";

const spacetimedb = schema({});
export default spacetimedb;

export const foo = spacetimedb.httpHandler((_ctx, _req) =>
  new SyncResponse("foo")
);

export const foo_slash = spacetimedb.httpHandler((_ctx, _req) =>
  new SyncResponse("foo-slash")
);

export const router = spacetimedb.httpRouter(
  new Router()
    .get("/foo", foo)
    .get("/foo/", foo_slash)
);
"#;

const TS_FULL_URI_MODULE_CODE: &str = r#"import { Router, SyncResponse, schema } from "spacetimedb/server";

const spacetimedb = schema({});
export default spacetimedb;

export const echo_uri = spacetimedb.httpHandler((_ctx, req) =>
  new SyncResponse(req.uri)
);

export const router = spacetimedb.httpRouter(
  new Router().get("/echo-uri", echo_uri)
);
"#;

const TS_HANDLE_REQUEST_BODY_MODULE_CODE: &str = r#"import { Router, SyncResponse, schema } from "spacetimedb/server";

const spacetimedb = schema({});
export default spacetimedb;

export const reverse_bytes = spacetimedb.httpHandler((_ctx, req) => {
  const reversed = req.bytes();
  reversed.reverse();
  return new SyncResponse(reversed);
});

export const reverse_words = spacetimedb.httpHandler((_ctx, req) => {
  let body;
  try {
    body = new TextDecoder("utf-8", { fatal: true }).decode(req.bytes());
  } catch {
    return new SyncResponse("request body must be valid UTF-8", { status: 400 });
  }

  const reversed = body.split(" ").reverse().join(" ");
  return new SyncResponse(reversed);
});

export const router = spacetimedb.httpRouter(
  new Router()
    .post("/reverse-bytes", reverse_bytes)
    .post("/reverse-words", reverse_words)
);
"#;

const NO_SUCH_ROUTE_BODY: &str = "Database has not registered a handler for this route";

fn extract_rust_code_blocks(doc_path: &Path) -> String {
    let doc = fs::read_to_string(doc_path).unwrap_or_else(|e| panic!("failed to read {}: {e}", doc_path.display()));
    let doc = doc.replace("\r\n", "\n");

    let re = Regex::new(r"```rust\n([\s\S]*?)\n```").expect("regex should compile");
    let blocks: Vec<_> = re
        .captures_iter(&doc)
        .map(|cap| cap.get(1).expect("capture group should exist").as_str().to_string())
        .collect();

    assert!(
        !blocks.is_empty(),
        "expected at least one rust code block in {}",
        doc_path.display()
    );

    blocks.join("\n\n")
}

fn extract_typescript_code_blocks(doc_path: &Path) -> String {
    let doc = fs::read_to_string(doc_path).unwrap_or_else(|e| panic!("failed to read {}: {e}", doc_path.display()));
    let doc = doc.replace("\r\n", "\n");

    let re = Regex::new(r"```(?:ts|typescript)\n([\s\S]*?)\n```").expect("regex should compile");
    let blocks: Vec<_> = re
        .captures_iter(&doc)
        .map(|cap| cap.get(1).expect("capture group should exist").as_str().to_string())
        .collect();

    assert!(
        !blocks.is_empty(),
        "expected at least one typescript code block in {}",
        doc_path.display()
    );

    blocks.join("\n\n")
}

fn rust_http_test(module_code: &str) -> (Smoketest, String) {
    let test = Smoketest::builder().module_code(module_code).build();
    let identity = test
        .database_identity
        .as_ref()
        .expect("database identity missing")
        .clone();
    (test, identity)
}

fn typescript_http_test(module_name: &str, db_name: &str, module_code: &str) -> (Smoketest, String) {
    require_pnpm!();
    let mut test = Smoketest::builder().autopublish(false).build();
    let identity = test
        .publish_typescript_module_source(module_name, db_name, module_code)
        .unwrap();
    (test, identity)
}

fn assert_http_routes_end_to_end(server_url: &str, identity: &str) {
    let base = format!("{server_url}/v1/database/{identity}/route");
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
        .get(format!("{server_url}/v1/database/{identity}/schema?version=10"))
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

fn assert_http_routes_pr_example_round_trip(server_url: &str, identity: &str) {
    let base = format!("{server_url}/v1/database/{identity}/route");
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

fn assert_http_routes_are_strict_for_non_root_paths(server_url: &str, identity: &str) {
    let base = format!("{server_url}/v1/database/{identity}/route");
    let client = reqwest::blocking::Client::new();

    let resp = client.get(format!("{base}/foo")).send().expect("foo failed");
    assert!(resp.status().is_success());
    assert_eq!(resp.text().expect("foo body"), "foo");

    let resp = client.get(format!("{base}/foo/")).send().expect("foo slash failed");
    assert!(resp.status().is_success());
    assert_eq!(resp.text().expect("foo slash body"), "foo-slash");

    let resp = client.get(format!("{base}//")).send().expect("double slash failed");
    assert_eq!(resp.status().as_u16(), 404);
    assert_eq!(resp.text().expect("double slash body"), NO_SUCH_ROUTE_BODY);

    let resp = client
        .get(format!("{base}//foo"))
        .send()
        .expect("double slash foo failed");
    assert_eq!(resp.status().as_u16(), 404);
    assert_eq!(resp.text().expect("double slash foo body"), NO_SUCH_ROUTE_BODY);
}

fn assert_http_routes_are_strict_for_root_paths(server_url: &str, identity: &str) {
    let base = format!("{server_url}/v1/database/{identity}/route");
    let client = reqwest::blocking::Client::new();

    let resp = client.get(base.clone()).send().expect("empty root failed");
    assert!(resp.status().is_success());
    assert_eq!(resp.text().expect("empty root body"), "empty");

    let resp = client.get(format!("{base}/")).send().expect("slash root failed");
    assert!(resp.status().is_success());
    assert_eq!(resp.text().expect("slash root body"), "slash");
}

fn assert_http_handler_observes_full_external_uri(server_url: &str, identity: &str) {
    let base = format!("{server_url}/v1/database/{identity}/route");
    let url = format!("{base}/echo-uri?alpha=beta");
    let client = reqwest::blocking::Client::new();

    let resp = client.get(&url).send().expect("echo-uri failed");
    assert!(resp.status().is_success());
    assert_eq!(resp.text().expect("echo-uri body"), url);
}

fn assert_handle_request_body(server_url: &str, identity: &str) {
    let base = format!("{server_url}/v1/database/{identity}/route");
    let client = reqwest::blocking::Client::new();

    let resp = client
        .post(format!("{base}/reverse-bytes"))
        .body(vec![0xFF, 0x00, 0xFE, 0x7F])
        .send()
        .expect("reverse-bytes invalid utf-8 failed");
    assert!(resp.status().is_success());
    assert_eq!(
        resp.bytes().expect("reverse-bytes invalid utf-8 body").as_ref(),
        [0x7F, 0xFE, 0x00, 0xFF]
    );

    let resp = client
        .post(format!("{base}/reverse-bytes"))
        .body("abcba")
        .send()
        .expect("reverse-bytes palindrome failed");
    assert!(resp.status().is_success());
    assert_eq!(resp.bytes().expect("reverse-bytes palindrome body").as_ref(), b"abcba");

    let resp = client
        .post(format!("{base}/reverse-bytes"))
        .body("stressed")
        .send()
        .expect("reverse-bytes non-palindrome failed");
    assert!(resp.status().is_success());
    assert_eq!(
        resp.bytes().expect("reverse-bytes non-palindrome body").as_ref(),
        b"desserts"
    );

    let resp = client
        .post(format!("{base}/reverse-words"))
        .body(vec![0x66, 0x6F, 0x80, 0x6F])
        .send()
        .expect("reverse-words invalid utf-8 failed");
    assert_eq!(resp.status().as_u16(), 400);
    assert_eq!(
        resp.text().expect("reverse-words invalid utf-8 body"),
        "request body must be valid UTF-8"
    );

    let resp = client
        .post(format!("{base}/reverse-words"))
        .body("step on no pets")
        .send()
        .expect("reverse-words palindrome failed");
    assert!(resp.status().is_success());
    assert_eq!(resp.text().expect("reverse-words palindrome body"), "pets no on step");

    let resp = client
        .post(format!("{base}/reverse-words"))
        .body("red green blue")
        .send()
        .expect("reverse-words non-palindrome failed");
    assert!(resp.status().is_success());
    assert_eq!(
        resp.text().expect("reverse-words non-palindrome body"),
        "blue green red"
    );
}

#[test]
fn http_routes_end_to_end() {
    let (test, identity) = rust_http_test(MODULE_CODE);
    assert_http_routes_end_to_end(&test.server_url, &identity);
}

#[test]
fn http_routes_pr_example_round_trip() {
    let (test, identity) = rust_http_test(EXAMPLE_MODULE_CODE);
    assert_http_routes_pr_example_round_trip(&test.server_url, &identity);
}

#[test]
fn http_routes_are_strict_for_non_root_paths() {
    let (test, identity) = rust_http_test(STRICT_NON_ROOT_ROUTING_MODULE_CODE);
    assert_http_routes_are_strict_for_non_root_paths(&test.server_url, &identity);
}

#[test]
fn http_routes_are_strict_for_root_paths() {
    let (test, identity) = rust_http_test(STRICT_ROOT_ROUTING_MODULE_CODE);
    assert_http_routes_are_strict_for_root_paths(&test.server_url, &identity);
}

#[test]
fn http_handler_observes_full_external_uri() {
    let (test, identity) = rust_http_test(FULL_URI_MODULE_CODE);
    assert_http_handler_observes_full_external_uri(&test.server_url, &identity);
}

#[test]
fn handle_request_body() {
    let (test, identity) = rust_http_test(HANDLE_REQUEST_BODY_MODULE_CODE);
    assert_handle_request_body(&test.server_url, &identity);
}

#[test]
fn typescript_http_routes_end_to_end() {
    let (test, identity) = typescript_http_test(
        "http-routes-typescript-basic",
        "http-routes-typescript-basic",
        TS_MODULE_CODE,
    );
    assert_http_routes_end_to_end(&test.server_url, &identity);
}

#[test]
fn typescript_http_routes_pr_example_round_trip() {
    let (test, identity) = typescript_http_test(
        "http-routes-typescript-example",
        "http-routes-typescript-example",
        TS_EXAMPLE_MODULE_CODE,
    );
    assert_http_routes_pr_example_round_trip(&test.server_url, &identity);
}

#[test]
fn typescript_http_routes_are_strict_for_non_root_paths() {
    let (test, identity) = typescript_http_test(
        "http-routes-typescript-strict-non-root",
        "http-routes-typescript-strict-non-root",
        TS_STRICT_NON_ROOT_ROUTING_MODULE_CODE,
    );
    assert_http_routes_are_strict_for_non_root_paths(&test.server_url, &identity);
}

#[test]
fn typescript_http_routes_are_strict_for_root_paths() {
    let (test, identity) = typescript_http_test(
        "http-routes-typescript-strict-root",
        "http-routes-typescript-strict-root",
        TS_STRICT_ROOT_ROUTING_MODULE_CODE,
    );
    assert_http_routes_are_strict_for_root_paths(&test.server_url, &identity);
}

#[test]
fn typescript_http_handler_observes_full_external_uri() {
    let (test, identity) = typescript_http_test(
        "http-routes-typescript-full-uri",
        "http-routes-typescript-full-uri",
        TS_FULL_URI_MODULE_CODE,
    );
    assert_http_handler_observes_full_external_uri(&test.server_url, &identity);
}

#[test]
fn typescript_handle_request_body() {
    let (test, identity) = typescript_http_test(
        "http-routes-typescript-request-body",
        "http-routes-typescript-request-body",
        TS_HANDLE_REQUEST_BODY_MODULE_CODE,
    );
    assert_handle_request_body(&test.server_url, &identity);
}

/// Validates the Rust example from `docs/docs/00200-core-concepts/00200-functions/00600-HTTP-handlers.md`.
#[test]
fn http_handlers_tutorial_say_hello_route_works() {
    let module_code = extract_rust_code_blocks(
        &workspace_root().join("docs/docs/00200-core-concepts/00200-functions/00600-HTTP-handlers.md"),
    );
    let test = Smoketest::builder().module_code(&module_code).build();
    let identity = test.database_identity.as_ref().expect("database identity missing");

    let url = format!("{}/v1/database/{}/route/say-hello", test.server_url, identity);
    let client = reqwest::blocking::Client::new();

    let resp = client.get(&url).send().expect("say-hello failed");
    assert!(resp.status().is_success());
    assert_eq!(resp.text().expect("say-hello body"), "Hello!");
}

/// Validates the TypeScript example from `docs/docs/00200-core-concepts/00200-functions/00600-HTTP-handlers.md`.
#[test]
fn typescript_http_handlers_tutorial_say_hello_route_works() {
    require_pnpm!();

    let module_code = extract_typescript_code_blocks(
        &workspace_root().join("docs/docs/00200-core-concepts/00200-functions/00600-HTTP-handlers.md"),
    );
    let mut test = Smoketest::builder().autopublish(false).build();
    let identity = test
        .publish_typescript_module_source(
            "http-handlers-docs-typescript",
            "http-handlers-docs-typescript",
            &module_code,
        )
        .unwrap();

    let url = format!("{}/v1/database/{identity}/route/say-hello", test.server_url);
    let client = reqwest::blocking::Client::new();

    let resp = client.get(&url).send().expect("say-hello failed");
    assert!(resp.status().is_success());
    assert_eq!(resp.text().expect("say-hello body"), "Hello!");
}

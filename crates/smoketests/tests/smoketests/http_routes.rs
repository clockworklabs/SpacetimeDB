use regex::Regex;
use spacetimedb_smoketests::{require_dotnet, require_emscripten, require_pnpm, workspace_root, Smoketest};
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

const CPP_MODULE_CODE: &str = r#"#include "spacetimedb.h"

using namespace SpacetimeDB;

struct Entry {
    uint64_t id;
    std::string value;
};
SPACETIMEDB_STRUCT(Entry, id, value)
SPACETIMEDB_TABLE(Entry, entry, Public)

namespace {

std::string header_value_utf8(const HttpRequest& request, const std::string& header_name) {
    for (const auto& header : request.headers) {
        if (header.name == header_name) {
            return std::string(header.value.begin(), header.value.end());
        }
    }
    return "";
}

HttpResponse text_response(uint16_t status_code, std::string body) {
    return HttpResponse{
        status_code,
        HttpVersion::Http11,
        { HttpHeader{"content-type", "text/plain; charset=utf-8"} },
        HttpBody::from_string(body),
    };
}

} // namespace

SPACETIMEDB_HTTP_HANDLER(get_simple, HandlerContext ctx, HttpRequest request) {
    return text_response(200, "ok");
}

SPACETIMEDB_HTTP_HANDLER(post_insert, HandlerContext ctx, HttpRequest request) {
    ctx.with_tx([](TxContext& tx) {
        uint64_t id = tx.db[entry].count();
        tx.db[entry].insert(Entry{ id, "posted" });
    });
    return text_response(200, "inserted");
}

SPACETIMEDB_HTTP_HANDLER(get_count, HandlerContext ctx, HttpRequest request) {
    uint64_t count = ctx.with_tx([](TxContext& tx) -> uint64_t {
        return tx.db[entry].count();
    });
    return text_response(200, std::to_string(count));
}

SPACETIMEDB_HTTP_HANDLER(any_handler, HandlerContext ctx, HttpRequest request) {
    return text_response(200, "any");
}

SPACETIMEDB_HTTP_HANDLER(header_echo, HandlerContext ctx, HttpRequest request) {
    return text_response(200, header_value_utf8(request, "x-echo"));
}

SPACETIMEDB_HTTP_HANDLER(set_response_header, HandlerContext ctx, HttpRequest request) {
    return HttpResponse{
        200,
        HttpVersion::Http11,
        { HttpHeader{"x-response", "set"} },
        HttpBody::from_string("header-set"),
    };
}

SPACETIMEDB_HTTP_HANDLER(body_handler, HandlerContext ctx, HttpRequest request) {
    return text_response(200, "non-empty");
}

SPACETIMEDB_HTTP_HANDLER(teapot, HandlerContext ctx, HttpRequest request) {
    return text_response(418, "teapot");
}

SPACETIMEDB_HTTP_ROUTER(router) {
    return Router()
        .get("/get", get_simple)
        .post("/post", post_insert)
        .get("/count", get_count)
        .any("/any", any_handler)
        .get("/header", header_echo)
        .get("/set-header", set_response_header)
        .get("/body", body_handler)
        .get("/teapot", teapot);
}
"#;

const CPP_EXAMPLE_MODULE_CODE: &str = r#"#include "spacetimedb.h"

using namespace SpacetimeDB;

struct Data {
    uint64_t id;
    std::vector<uint8_t> body;
};
SPACETIMEDB_STRUCT(Data, id, body)
SPACETIMEDB_TABLE(Data, data, Public)
FIELD_PrimaryKeyAutoInc(data, id)

namespace {

HttpResponse bytes_response(uint16_t status_code, std::vector<uint8_t> body) {
    return HttpResponse{
        status_code,
        HttpVersion::Http11,
        {},
        HttpBody{std::move(body)},
    };
}

HttpResponse text_response(uint16_t status_code, std::string body) {
    return HttpResponse{
        status_code,
        HttpVersion::Http11,
        {},
        HttpBody::from_string(body),
    };
}

std::string query_value(const std::string& uri, const std::string& key) {
    std::string needle = "?" + key + "=";
    size_t pos = uri.find(needle);
    if (pos == std::string::npos) {
        needle = "&" + key + "=";
        pos = uri.find(needle);
    }
    if (pos == std::string::npos) {
        return "";
    }
    pos += needle.size();
    size_t end = uri.find('&', pos);
    return uri.substr(pos, end == std::string::npos ? std::string::npos : end - pos);
}

bool try_parse_u64(const std::string& text, uint64_t& value) {
    if (text.empty()) {
        return false;
    }
    uint64_t result = 0;
    for (char c : text) {
        if (c < '0' || c > '9') {
            return false;
        }
        result = (result * 10) + static_cast<uint64_t>(c - '0');
    }
    value = result;
    return true;
}

} // namespace

SPACETIMEDB_HTTP_HANDLER(insert, HandlerContext ctx, HttpRequest request) {
    std::vector<uint8_t> body = request.body.to_bytes();
    uint64_t id = ctx.with_tx([&](TxContext& tx) -> uint64_t {
        return tx.db[data].insert(Data{0, body}).id;
    });
    return text_response(200, std::to_string(id));
}

SPACETIMEDB_HTTP_HANDLER(retrieve, HandlerContext ctx, HttpRequest request) {
    uint64_t id = 0;
    if (!try_parse_u64(query_value(request.uri, "id"), id)) {
        return text_response(500, "invalid id");
    }

    auto body = ctx.with_tx([&](TxContext& tx) -> std::optional<std::vector<uint8_t>> {
        auto row = tx.db[data_id].find(id);
        if (row.has_value()) {
            return row->body;
        }
        return std::nullopt;
    });

    if (body.has_value()) {
        return bytes_response(200, std::move(body.value()));
    }
    return bytes_response(404, {});
}

SPACETIMEDB_HTTP_ROUTER(router) {
    return Router().post("/insert", insert).get("/retrieve", retrieve);
}
"#;

const CPP_STRICT_ROOT_ROUTING_MODULE_CODE: &str = r#"#include "spacetimedb.h"

using namespace SpacetimeDB;

namespace {

HttpResponse text_response(const std::string& body) {
    return HttpResponse{200, HttpVersion::Http11, {}, HttpBody::from_string(body)};
}

} // namespace

SPACETIMEDB_HTTP_HANDLER(empty_root, HandlerContext ctx, HttpRequest request) {
    return text_response("empty");
}

SPACETIMEDB_HTTP_HANDLER(slash_root, HandlerContext ctx, HttpRequest request) {
    return text_response("slash");
}

SPACETIMEDB_HTTP_HANDLER(foo, HandlerContext ctx, HttpRequest request) {
    return text_response("foo");
}

SPACETIMEDB_HTTP_HANDLER(foo_slash, HandlerContext ctx, HttpRequest request) {
    return text_response("foo-slash");
}

SPACETIMEDB_HTTP_ROUTER(router) {
    return Router()
        .get("", empty_root)
        .get("/", slash_root)
        .get("/foo", foo)
        .get("/foo/", foo_slash);
}
"#;

const CPP_STRICT_NON_ROOT_ROUTING_MODULE_CODE: &str = r#"#include "spacetimedb.h"

using namespace SpacetimeDB;

namespace {

HttpResponse text_response(const std::string& body) {
    return HttpResponse{200, HttpVersion::Http11, {}, HttpBody::from_string(body)};
}

} // namespace

SPACETIMEDB_HTTP_HANDLER(foo, HandlerContext ctx, HttpRequest request) {
    return text_response("foo");
}

SPACETIMEDB_HTTP_HANDLER(foo_slash, HandlerContext ctx, HttpRequest request) {
    return text_response("foo-slash");
}

SPACETIMEDB_HTTP_ROUTER(router) {
    return Router()
        .get("/foo", foo)
        .get("/foo/", foo_slash);
}
"#;

const CPP_FULL_URI_MODULE_CODE: &str = r#"#include "spacetimedb.h"

using namespace SpacetimeDB;

SPACETIMEDB_HTTP_HANDLER(echo_uri, HandlerContext ctx, HttpRequest request) {
    return HttpResponse{
        200,
        HttpVersion::Http11,
        {},
        HttpBody::from_string(request.uri),
    };
}

SPACETIMEDB_HTTP_ROUTER(router) {
    return Router().get("/echo-uri", echo_uri);
}
"#;

const CPP_HANDLE_REQUEST_BODY_MODULE_CODE: &str = r#"#include "spacetimedb.h"
#include <algorithm>

using namespace SpacetimeDB;

namespace {

HttpResponse bytes_response(uint16_t status_code, std::vector<uint8_t> body) {
    return HttpResponse{status_code, HttpVersion::Http11, {}, HttpBody{std::move(body)}};
}

HttpResponse text_response(uint16_t status_code, const std::string& body) {
    return HttpResponse{status_code, HttpVersion::Http11, {}, HttpBody::from_string(body)};
}

} // namespace

SPACETIMEDB_HTTP_HANDLER(reverse_bytes, HandlerContext ctx, HttpRequest request) {
    std::vector<uint8_t> reversed = request.body.to_bytes();
    std::reverse(reversed.begin(), reversed.end());
    return bytes_response(200, std::move(reversed));
}

SPACETIMEDB_HTTP_HANDLER(reverse_words, HandlerContext ctx, HttpRequest request) {
    const std::vector<uint8_t> bytes = request.body.to_bytes();
    std::string body(bytes.begin(), bytes.end());
    if (body.find(static_cast<char>(0x80)) != std::string::npos) {
        return text_response(400, "request body must be valid UTF-8");
    }

    std::vector<std::string> words;
    size_t start = 0;
    while (true) {
        size_t pos = body.find(' ', start);
        words.push_back(body.substr(start, pos == std::string::npos ? std::string::npos : pos - start));
        if (pos == std::string::npos) {
            break;
        }
        start = pos + 1;
    }
    std::reverse(words.begin(), words.end());

    std::string reversed;
    for (size_t i = 0; i < words.size(); ++i) {
        if (i != 0) {
            reversed += " ";
        }
        reversed += words[i];
    }

    return text_response(200, reversed);
}

SPACETIMEDB_HTTP_ROUTER(router) {
    return Router()
        .post("/reverse-bytes", reverse_bytes)
        .post("/reverse-words", reverse_words);
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

const CS_MODULE_CODE: &str = r#"
using System;
using System.Collections.Generic;
using System.Linq;
using System.Text;
using SpacetimeDB;

#pragma warning disable STDB_UNSTABLE
public static partial class Module
{
    [SpacetimeDB.Table(Accessor = "Entry", Name = "entry", Public = true)]
    public partial struct Entry
    {
        [SpacetimeDB.PrimaryKey]
        public ulong Id;

        public string Value;
    }

    [SpacetimeDB.HttpHandler]
    public static HttpResponse GetSimple(HandlerContext ctx, HttpRequest request)
    {
        return TextResponse(200, "ok");
    }

    [SpacetimeDB.HttpHandler]
    public static HttpResponse PostInsert(HandlerContext ctx, HttpRequest request)
    {
        ctx.WithTx((HandlerTxContext tx) =>
        {
            var id = tx.Db.Entry.Count;
            tx.Db.Entry.Insert(new Entry { Id = id, Value = "posted" });
            return 0;
        });
        return TextResponse(200, "inserted");
    }

    [SpacetimeDB.HttpHandler]
    public static HttpResponse GetCount(HandlerContext ctx, HttpRequest request)
    {
        var count = ctx.WithTx((HandlerTxContext tx) => tx.Db.Entry.Count);
        return TextResponse(200, count.ToString());
    }

    [SpacetimeDB.HttpHandler]
    public static HttpResponse AnyHandler(HandlerContext ctx, HttpRequest request)
    {
        return TextResponse(200, "any");
    }

    [SpacetimeDB.HttpHandler]
    public static HttpResponse HeaderEcho(HandlerContext ctx, HttpRequest request)
    {
        return TextResponse(200, HeaderValueUtf8(request, "x-echo"));
    }

    [SpacetimeDB.HttpHandler]
    public static HttpResponse SetResponseHeader(HandlerContext ctx, HttpRequest request)
    {
        return new HttpResponse(
            200,
            HttpVersion.Http11,
            new List<HttpHeader> { new("x-response", "set") },
            HttpBody.FromString("header-set")
        );
    }

    [SpacetimeDB.HttpHandler]
    public static HttpResponse BodyHandler(HandlerContext ctx, HttpRequest request)
    {
        return TextResponse(200, "non-empty");
    }

    [SpacetimeDB.HttpHandler]
    public static HttpResponse Teapot(HandlerContext ctx, HttpRequest request)
    {
        return TextResponse(418, "teapot");
    }

    [SpacetimeDB.HttpRouter]
    public static Router Router() =>
        SpacetimeDB.Router.New()
            .Get("/get", Handlers.GetSimple)
            .Post("/post", Handlers.PostInsert)
            .Get("/count", Handlers.GetCount)
            .Any("/any", Handlers.AnyHandler)
            .Get("/header", Handlers.HeaderEcho)
            .Get("/set-header", Handlers.SetResponseHeader)
            .Get("/body", Handlers.BodyHandler)
            .Get("/teapot", Handlers.Teapot);

    private static string HeaderValueUtf8(HttpRequest request, string headerName)
    {
        foreach (var header in request.Headers)
        {
            if (string.Equals(header.Name, headerName, StringComparison.OrdinalIgnoreCase))
            {
                return Encoding.UTF8.GetString(header.Value);
            }
        }
        return string.Empty;
    }

    private static HttpResponse TextResponse(ushort statusCode, string body) =>
        new(
            statusCode,
            HttpVersion.Http11,
            new List<HttpHeader>(),
            HttpBody.FromString(body)
        );
}
"#;

const CS_EXAMPLE_MODULE_CODE: &str = r#"
using System.Collections.Generic;
using SpacetimeDB;

#pragma warning disable STDB_UNSTABLE
public static partial class Module
{
    [SpacetimeDB.Table(Accessor = "Data", Name = "data", Public = true)]
    public partial struct Data
    {
        [SpacetimeDB.PrimaryKey]
        [SpacetimeDB.AutoInc]
        public ulong Id;

        public byte[] Body;
    }

    [SpacetimeDB.HttpHandler]
    public static HttpResponse Insert(HandlerContext ctx, HttpRequest request)
    {
        var body = request.Body.ToBytes();
        var id = ctx.WithTx((HandlerTxContext tx) => tx.Db.Data.Insert(new Data { Id = 0, Body = body }).Id);
        return TextResponse(200, id.ToString());
    }

    [SpacetimeDB.HttpHandler]
    public static HttpResponse Retrieve(HandlerContext ctx, HttpRequest request)
    {
        var idText = request.Uri.Split("id=", 2)[1];
        var id = ulong.Parse(idText);
        var body = ctx.WithTx((HandlerTxContext tx) => tx.Db.Data.Id.Find(id)?.Body);

        if (body is not null)
        {
            return BytesResponse(200, body);
        }

        return new HttpResponse(404, HttpVersion.Http11, new List<HttpHeader>(), HttpBody.Empty);
    }

    [SpacetimeDB.HttpRouter]
    public static Router Router() =>
        SpacetimeDB.Router.New()
            .Post("/insert", Handlers.Insert)
            .Get("/retrieve", Handlers.Retrieve);

    private static HttpResponse BytesResponse(ushort statusCode, byte[] body) =>
        new(statusCode, HttpVersion.Http11, new List<HttpHeader>(), new HttpBody(body));

    private static HttpResponse TextResponse(ushort statusCode, string body) =>
        new(statusCode, HttpVersion.Http11, new List<HttpHeader>(), HttpBody.FromString(body));
}
"#;

const CS_STRICT_ROOT_ROUTING_MODULE_CODE: &str = r#"
using System.Collections.Generic;
using SpacetimeDB;

public static partial class Module
{
    [SpacetimeDB.HttpHandler]
    public static HttpResponse EmptyRoot(HandlerContext ctx, HttpRequest request)
    {
        return TextResponse("empty");
    }

    [SpacetimeDB.HttpHandler]
    public static HttpResponse SlashRoot(HandlerContext ctx, HttpRequest request)
    {
        return TextResponse("slash");
    }

    [SpacetimeDB.HttpHandler]
    public static HttpResponse Foo(HandlerContext ctx, HttpRequest request)
    {
        return TextResponse("foo");
    }

    [SpacetimeDB.HttpHandler]
    public static HttpResponse FooSlash(HandlerContext ctx, HttpRequest request)
    {
        return TextResponse("foo-slash");
    }

    [SpacetimeDB.HttpRouter]
    public static Router Router() =>
        SpacetimeDB.Router.New()
            .Get("", Handlers.EmptyRoot)
            .Get("/", Handlers.SlashRoot)
            .Get("/foo", Handlers.Foo)
            .Get("/foo/", Handlers.FooSlash);

    private static HttpResponse TextResponse(string body) =>
        new(200, HttpVersion.Http11, new List<HttpHeader>(), HttpBody.FromString(body));
}
"#;

const CS_STRICT_NON_ROOT_ROUTING_MODULE_CODE: &str = r#"
using System.Collections.Generic;
using SpacetimeDB;

public static partial class Module
{
    [SpacetimeDB.HttpHandler]
    public static HttpResponse Foo(HandlerContext ctx, HttpRequest request)
    {
        return TextResponse("foo");
    }

    [SpacetimeDB.HttpHandler]
    public static HttpResponse FooSlash(HandlerContext ctx, HttpRequest request)
    {
        return TextResponse("foo-slash");
    }

    [SpacetimeDB.HttpRouter]
    public static Router Router() =>
        SpacetimeDB.Router.New()
            .Get("/foo", Handlers.Foo)
            .Get("/foo/", Handlers.FooSlash);

    private static HttpResponse TextResponse(string body) =>
        new(200, HttpVersion.Http11, new List<HttpHeader>(), HttpBody.FromString(body));
}
"#;

const CS_FULL_URI_MODULE_CODE: &str = r#"
using System.Collections.Generic;
using SpacetimeDB;

public static partial class Module
{
    [SpacetimeDB.HttpHandler]
    public static HttpResponse EchoUri(HandlerContext ctx, HttpRequest request)
    {
        return new HttpResponse(
            200,
            HttpVersion.Http11,
            new List<HttpHeader>(),
            HttpBody.FromString(request.Uri)
        );
    }

    [SpacetimeDB.HttpRouter]
    public static Router Router() =>
        SpacetimeDB.Router.New().Get("/echo-uri", Handlers.EchoUri);
}
"#;

const CS_HANDLE_REQUEST_BODY_MODULE_CODE: &str = r#"
using System;
using System.Collections.Generic;
using System.Text;
using SpacetimeDB;

public static partial class Module
{
    [SpacetimeDB.HttpHandler]
    public static HttpResponse ReverseBytes(HandlerContext ctx, HttpRequest request)
    {
        var reversed = request.Body.ToBytes();
        Array.Reverse(reversed);
        return BytesResponse(200, reversed);
    }

    [SpacetimeDB.HttpHandler]
    public static HttpResponse ReverseWords(HandlerContext ctx, HttpRequest request)
    {
        string body;
        try
        {
            body = new UTF8Encoding(false, true).GetString(request.Body.ToBytes());
        }
        catch (DecoderFallbackException)
        {
            return TextResponse(400, "request body must be valid UTF-8");
        }

        var reversed = string.Join(" ", body.Split(' ').Reverse());
        return TextResponse(200, reversed);
    }

    [SpacetimeDB.HttpRouter]
    public static Router Router() =>
        SpacetimeDB.Router.New()
            .Post("/reverse-bytes", Handlers.ReverseBytes)
            .Post("/reverse-words", Handlers.ReverseWords);

    private static HttpResponse BytesResponse(ushort statusCode, byte[] body) =>
        new(statusCode, HttpVersion.Http11, new List<HttpHeader>(), new HttpBody(body));

    private static HttpResponse TextResponse(ushort statusCode, string body) =>
        new(statusCode, HttpVersion.Http11, new List<HttpHeader>(), HttpBody.FromString(body));
}
"#;

const NO_SUCH_ROUTE_BODY: &str = "Database has not registered a handler for this route";

fn extract_code_blocks(doc_path: &Path, regex_src: &str, language_name: &str) -> String {
    let doc = fs::read_to_string(doc_path).unwrap_or_else(|e| panic!("failed to read {}: {e}", doc_path.display()));
    let doc = doc.replace("\r\n", "\n");

    let re = Regex::new(regex_src).expect("regex should compile");
    let blocks: Vec<_> = re
        .captures_iter(&doc)
        .map(|cap| cap.get(1).expect("capture group should exist").as_str().to_string())
        .collect();

    assert!(
        !blocks.is_empty(),
        "expected at least one {} code block in {}",
        language_name,
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

fn cpp_http_test(name: &str, module_code: &str) -> (Smoketest, String) {
    require_emscripten!();
    let mut test = Smoketest::builder().autopublish(false).build();
    let identity = test.publish_cpp_module_source(name, name, module_code).unwrap();
    (test, identity)
}

fn typescript_http_test(name: &str, module_code: &str) -> (Smoketest, String) {
    require_pnpm!();
    let mut test = Smoketest::builder().autopublish(false).build();
    let identity = test.publish_typescript_module_source(name, name, module_code).unwrap();
    (test, identity)
}

fn csharp_http_test(name: &str, module_code: &str) -> (Smoketest, String) {
    let mut test = Smoketest::builder().autopublish(false).build();
    let identity = test.publish_csharp_module_source(name, name, module_code).unwrap();
    (test, identity)
}

fn route_base(server_url: &str, identity: &str) -> String {
    format!("{server_url}/v1/database/{identity}/route")
}

fn assert_http_routes_end_to_end(server_url: &str, identity: &str) {
    let base = route_base(server_url, identity);
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
    let base = route_base(server_url, identity);
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
    let base = route_base(server_url, identity);
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
    let base = route_base(server_url, identity);
    let client = reqwest::blocking::Client::new();

    let resp = client.get(base.clone()).send().expect("empty root failed");
    assert!(resp.status().is_success());
    assert_eq!(resp.text().expect("empty root body"), "empty");

    let resp = client.get(format!("{base}/")).send().expect("slash root failed");
    assert!(resp.status().is_success());
    assert_eq!(resp.text().expect("slash root body"), "slash");
}

fn assert_http_handler_observes_full_external_uri(server_url: &str, identity: &str) {
    let base = route_base(server_url, identity);
    let url = format!("{base}/echo-uri?alpha=beta");
    let client = reqwest::blocking::Client::new();

    let resp = client.get(&url).send().expect("echo-uri failed");
    assert!(resp.status().is_success());
    assert_eq!(resp.text().expect("echo-uri body"), url);
}

fn assert_handle_request_body(server_url: &str, identity: &str) {
    let base = route_base(server_url, identity);
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
fn cpp_http_routes_end_to_end() {
    let (test, identity) = cpp_http_test("http-routes-cpp-basic", CPP_MODULE_CODE);
    assert_http_routes_end_to_end(&test.server_url, &identity);
}

#[test]
fn typescript_http_routes_end_to_end() {
    let (test, identity) = typescript_http_test("http-routes-typescript-basic", TS_MODULE_CODE);
    assert_http_routes_end_to_end(&test.server_url, &identity);
}

#[test]
fn csharp_http_routes_end_to_end() {
    require_dotnet!();
    let (test, identity) = csharp_http_test("http-routes-csharp-basic", CS_MODULE_CODE);
    assert_http_routes_end_to_end(&test.server_url, &identity);
}

#[test]
fn cpp_http_routes_pr_example_round_trip() {
    let (test, identity) = cpp_http_test("http-routes-cpp-example", CPP_EXAMPLE_MODULE_CODE);
    assert_http_routes_pr_example_round_trip(&test.server_url, &identity);
}

#[test]
fn typescript_http_routes_pr_example_round_trip() {
    let (test, identity) = typescript_http_test("http-routes-typescript-example", TS_EXAMPLE_MODULE_CODE);
    assert_http_routes_pr_example_round_trip(&test.server_url, &identity);
}

#[test]
fn csharp_http_routes_pr_example_round_trip() {
    require_dotnet!();
    let (test, identity) = csharp_http_test("http-routes-csharp-example", CS_EXAMPLE_MODULE_CODE);
    assert_http_routes_pr_example_round_trip(&test.server_url, &identity);
}

#[test]
fn cpp_http_routes_are_strict_for_non_root_paths() {
    let (test, identity) = cpp_http_test(
        "http-routes-cpp-strict-non-root",
        CPP_STRICT_NON_ROOT_ROUTING_MODULE_CODE,
    );
    assert_http_routes_are_strict_for_non_root_paths(&test.server_url, &identity);
}

#[test]
fn typescript_http_routes_are_strict_for_non_root_paths() {
    let (test, identity) = typescript_http_test(
        "http-routes-typescript-strict-non-root",
        TS_STRICT_NON_ROOT_ROUTING_MODULE_CODE,
    );
    assert_http_routes_are_strict_for_non_root_paths(&test.server_url, &identity);
}

#[test]
fn csharp_http_routes_are_strict_for_non_root_paths() {
    require_dotnet!();
    let (test, identity) = csharp_http_test(
        "http-routes-csharp-strict-non-root",
        CS_STRICT_NON_ROOT_ROUTING_MODULE_CODE,
    );
    assert_http_routes_are_strict_for_non_root_paths(&test.server_url, &identity);
}

#[test]
fn cpp_http_routes_are_strict_for_root_paths() {
    let (test, identity) = cpp_http_test("http-routes-cpp-strict-root", CPP_STRICT_ROOT_ROUTING_MODULE_CODE);
    assert_http_routes_are_strict_for_root_paths(&test.server_url, &identity);
}

#[test]
fn typescript_http_routes_are_strict_for_root_paths() {
    let (test, identity) =
        typescript_http_test("http-routes-typescript-strict-root", TS_STRICT_ROOT_ROUTING_MODULE_CODE);
    assert_http_routes_are_strict_for_root_paths(&test.server_url, &identity);
}

#[test]
fn csharp_http_routes_are_strict_for_root_paths() {
    require_dotnet!();
    let (test, identity) = csharp_http_test("http-routes-csharp-strict-root", CS_STRICT_ROOT_ROUTING_MODULE_CODE);
    assert_http_routes_are_strict_for_root_paths(&test.server_url, &identity);
}

#[test]
fn cpp_http_handler_observes_full_external_uri() {
    let (test, identity) = cpp_http_test("http-routes-cpp-full-uri", CPP_FULL_URI_MODULE_CODE);
    assert_http_handler_observes_full_external_uri(&test.server_url, &identity);
}

#[test]
fn typescript_http_handler_observes_full_external_uri() {
    let (test, identity) = typescript_http_test("http-routes-typescript-full-uri", TS_FULL_URI_MODULE_CODE);
    assert_http_handler_observes_full_external_uri(&test.server_url, &identity);
}

#[test]
fn csharp_http_handler_observes_full_external_uri() {
    require_dotnet!();
    let (test, identity) = csharp_http_test("http-routes-csharp-full-uri", CS_FULL_URI_MODULE_CODE);
    assert_http_handler_observes_full_external_uri(&test.server_url, &identity);
}

#[test]
fn cpp_handle_request_body() {
    let (test, identity) = cpp_http_test("http-routes-cpp-request-body", CPP_HANDLE_REQUEST_BODY_MODULE_CODE);
    assert_handle_request_body(&test.server_url, &identity);
}

#[test]
fn typescript_handle_request_body() {
    let (test, identity) = typescript_http_test(
        "http-routes-typescript-request-body",
        TS_HANDLE_REQUEST_BODY_MODULE_CODE,
    );
    assert_handle_request_body(&test.server_url, &identity);
}

#[test]
fn csharp_handle_request_body() {
    require_dotnet!();
    let (test, identity) = csharp_http_test("http-routes-csharp-request-body", CS_HANDLE_REQUEST_BODY_MODULE_CODE);
    assert_handle_request_body(&test.server_url, &identity);
}

/// Validates the Rust example from `docs/docs/00200-core-concepts/00200-functions/00600-HTTP-handlers.md`.
#[test]
fn http_handlers_tutorial_say_hello_route_works() {
    let module_code = extract_code_blocks(
        &workspace_root().join("docs/docs/00200-core-concepts/00200-functions/00600-HTTP-handlers.md"),
        r"```rust\n([\s\S]*?)\n```",
        "rust",
    );
    let test = Smoketest::builder().module_code(&module_code).build();
    let identity = test.database_identity.as_ref().expect("database identity missing");

    let url = format!("{}/v1/database/{}/route/say-hello", test.server_url, identity);
    let client = reqwest::blocking::Client::new();

    let resp = client.get(&url).send().expect("say-hello failed");
    assert!(resp.status().is_success());
    assert_eq!(resp.text().expect("say-hello body"), "Hello!");
}

/// Validates the C++ example from `docs/docs/00200-core-concepts/00200-functions/00600-HTTP-handlers.md`.
#[test]
fn cpp_http_handlers_tutorial_say_hello_route_works() {
    require_emscripten!();

    let module_code = extract_code_blocks(
        &workspace_root().join("docs/docs/00200-core-concepts/00200-functions/00600-HTTP-handlers.md"),
        r"```(?:cpp|c\+\+)\n([\s\S]*?)\n```",
        "cpp",
    );
    let mut test = Smoketest::builder().autopublish(false).build();
    let identity = test
        .publish_cpp_module_source("http-handlers-docs-cpp", "http-handlers-docs-cpp", &module_code)
        .unwrap();

    let url = format!("{}/v1/database/{identity}/route/say-hello", test.server_url);
    let client = reqwest::blocking::Client::new();

    let resp = client.get(&url).send().expect("say-hello failed");
    assert!(resp.status().is_success());
    assert_eq!(resp.text().expect("say-hello body"), "Hello!");
}

/// Validates the TypeScript example from `docs/docs/00200-core-concepts/00200-functions/00600-HTTP-handlers.md`.
#[test]
fn typescript_http_handlers_tutorial_say_hello_route_works() {
    require_pnpm!();

    let module_code = extract_code_blocks(
        &workspace_root().join("docs/docs/00200-core-concepts/00200-functions/00600-HTTP-handlers.md"),
        r"```(?:ts|typescript)\n([\s\S]*?)\n```",
        "typescript",
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

/// Validates the C# example from `docs/docs/00200-core-concepts/00200-functions/00600-HTTP-handlers.md`.
#[test]
fn csharp_http_handlers_tutorial_say_hello_route_works() {
    require_dotnet!();
    let module_code = extract_code_blocks(
        &workspace_root().join("docs/docs/00200-core-concepts/00200-functions/00600-HTTP-handlers.md"),
        r"```csharp\n([\s\S]*?)\n```",
        "csharp",
    );
    let (test, identity) = csharp_http_test("http-handlers-docs-csharp", &module_code);

    let url = format!("{}/v1/database/{}/route/say-hello", test.server_url, identity);
    let client = reqwest::blocking::Client::new();

    let resp = client.get(&url).send().expect("say-hello failed");
    assert!(resp.status().is_success());
    assert_eq!(resp.text().expect("say-hello body"), "Hello!");
}

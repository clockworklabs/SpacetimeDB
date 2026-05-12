---
title: HTTP Handlers
slug: /functions/http-handlers
---

import Tabs from '@theme/Tabs'
import TabItem from '@theme/TabItem'

HTTP handlers allow a SpacetimeDB database to expose an HTTP API.
External clients can make HTTP requests to routes nested under [`/v1/database/:name_or_address/route`](../../00300-resources/00200-reference/00200-http-api/00300-database.md#any-v1databasename_or_identityroutepath); these requests are resolved to routes defined by the database and then passed to the corresponding HTTP handler.

:::warning
***HTTP handlers are currently in beta, and their API may change in upcoming SpacetimeDB releases.***
:::

## Defining HTTP Handlers

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

Define an HTTP handler with `spacetimedb.httpHandler`.

The function must accept exactly two arguments:

1. A `HandlerContext`.
2. A `Request`.

The function must return a `SyncResponse`.

```typescript
import { schema, SyncResponse } from "spacetimedb/server";

const spacetimedb = schema({});
export default spacetimedb;

export const say_hello = spacetimedb.httpHandler((_ctx, _req) => {
    return new SyncResponse("Hello!");
});
```

</TabItem>
<TabItem value="rust" label="Rust">

Because HTTP handlers are unstable, Rust modules that define them must opt in to the `unstable` feature in their `Cargo.toml`:

```toml
[dependencies]
spacetimedb = { version = "2.*", features = ["unstable"] }
```

Define an HTTP handler by annotating a function with `#[spacetimedb::http::handler]`.

The function must accept exactly two arguments:

1. A  `&mut spacetimedb::http::HandlerContext`.
2. A `spacetimedb::http::Request`.

The function must return a `spacetimedb::http::Response`.

```rust
use spacetimedb::http::{Body, handler, HandlerContext, Request, Response};

#[handler]
fn say_hello(_ctx: &mut HandlerContext, _req: Request) -> Response {
    Response::new(Body::from_bytes("Hello!"))
}
```

</TabItem>
<TabItem value="cpp" label="C++">

Because HTTP handlers are unstable, C++ modules that define them must enable `SPACETIMEDB_UNSTABLE_FEATURES` when compiling.

Define an HTTP handler with `SPACETIMEDB_HTTP_HANDLER`.

The function must accept exactly two arguments:

1. A `SpacetimeDB::HandlerContext`.
2. A `SpacetimeDB::HttpRequest`.

The function must return a `SpacetimeDB::HttpResponse`.

```cpp
#include "spacetimedb.h"

using namespace SpacetimeDB;

SPACETIMEDB_HTTP_HANDLER(say_hello, HandlerContext ctx, HttpRequest request) {
    (void)ctx;
    (void)request;
    return HttpResponse{
        200,
        HttpVersion::Http11,
        { HttpHeader{"content-type", "text/plain; charset=utf-8"} },
        HttpBody::from_string("Hello!"),
    };
}
```

</TabItem>
</Tabs>

## Registering Handlers to Routes

Once you've [defined an HTTP handler](#defining-http-handlers), you must register it to a route in order to make it reachable for requests.

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

All routes exposed by your module are declared in a `Router`. Register the `Router` for your database by passing it to `spacetimedb.httpRouter`.

```typescript
import { Router } from "spacetimedb/server";

export const router = spacetimedb.httpRouter(
    new Router()
        .get("/say-hello", say_hello)
);
```

Add routes within a router with the `get`, `head`, `options`, `put`, `delete`, `post`, `patch` and `any` methods, which register an HTTP handler for that HTTP method at a given path.

Nest routers with `router.nest(prefix, subRouter)`, which causes `subRouter` to handle routing for all paths that start with `prefix`.

Combine routers with `router.merge(otherRouter)`, which combines both routers.

</TabItem>
<TabItem value="rust" label="Rust">

All routes exposed by your module are declared in a `spacetimedb::http::Router`. Register the `Router` for your database by returning it from a function annotated with `#[spacetimedb::http::router]`.

```rust
use spacetimedb::http::{router, Router};

#[router]
fn router() -> Router {
    Router::new()
        .get("/say-hello", say_hello)
}
```

Add routes within a router with the `get`, `head`, `options`, `put`, `delete`, `post`, `patch` and `any` methods, which register an HTTP handler for that HTTP method at a given path.

Nest routers with `router.nest(prefix, sub_router)`, which causes `sub_router` to handle routing for all paths that start with `prefix`.

Combine routers with `router.merge(other_router)`, which combines both routers.

</TabItem>
<TabItem value="cpp" label="C++">

All routes exposed by your module are declared in a `SpacetimeDB::Router`. Register the `Router` for your database by returning it from a function defined with `SPACETIMEDB_HTTP_ROUTER`.

```cpp
SPACETIMEDB_HTTP_ROUTER(router) {
    return Router()
        .get("/say-hello", say_hello);
}
```

Add routes within a router with the `get`, `head`, `options`, `put`, `delete_`, `post`, `patch` and `any` methods, which register an HTTP handler for that HTTP method at a given path.

Nest routers with `router.nest(prefix, sub_router)`, which causes `sub_router` to handle routing for all paths that start with `prefix`.

Combine routers with `router.merge(other_router)`, which combines both routers.

</TabItem>
</Tabs>

### Strict Routing

SpacetimeDB uses strict routing, meaning that a request must match a path exactly in order to be routed to that handler. Trailing slashes are significant.

## Sending Requests

Routes defined by a SpacetimeDB database are exposed under the prefix `/v1/database/:name/route`. To access the `say-hello` route above, send a request to `$SPACETIMEDB_URI/v1/database/$DATABASE/route/say-hello`, where `$SPACETIMEDB_URI` is the SpacetimeDB host (usually `https://maincloud.spacetimedb.com`), and `$DATABASE` is the name of the database.

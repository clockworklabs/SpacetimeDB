---
Title: Procedures
slug: /procedures
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';

A **procedure** is a function exported by a [database](/#database), similar to a [reducer](/#reducer).
Connected [clients](/#client-side-sdks) can call procedures.
Procedures can perform additional operations not possible in reducers, including making HTTP requests to external services.
However, procedures don't automatically run in database transactions,
and must manually open and commit a transaction in order to read from or modify the database state.
For this reason, prefer defining reducers rather than procedures unless you need to use one of the special procedure operators.

Procedures are currently in beta, and their API may change in upcoming SpacetimeDB releases.

# Defining Procedures

<Tabs groupId="server-language" queryString>
<TabItem value="rust" label="Rust">

Because procedures are unstable, Rust modules that define them must opt in to the `unstable` feature in their `Cargo.toml`:

```toml
[dependencies]
spacetimedb = { version = "1.x", features = ["unstable"] }
```

Define a procedure by annotating a function with `#[spacetimedb::procedure]`.

This function's first argument must be of type `&mut spacetimedb::ProcedureContext`.
By convention, this argument is named `ctx`.

A procedure may accept any number of additional arguments.
Each argument must be of a type that implements `spacetimedb::SpacetimeType`.
When defining a `struct` or `enum`, annotate it with `#[derive(spacetimedb::SpacetimeType)]`
to make it usable as a procedure argument.
These argument values will not be broadcast to clients other than the caller.

A procedure may return a value of any type that implements `spacetimedb::SpacetimeType`.
This return value will be sent to the caller, but will not be broadcast to any other clients.

```rust
#[spacetimedb::procedure]
fn add_two_numbers(ctx: &mut spacetimedb::ProcedureContext, lhs: u32, rhs: u32) -> u64 {
    lhs as u64 + rhs as u64
}
```

</TabItem>
<TabItem value="csharp"label="C#">

Support for procedures in C# modules is coming soon!

</TabItem>
<TabItem value="typescript" label="TypeScript">

TODO(noa)

</TabItem>
</Tabs>

## Accessing the database

<Tabs groupId="server-language" queryString>
<TabItem value="rust" label="Rust">

Unlike reducers, procedures don't automatically run in database transactions.
This means there's no `ctx.db` field to access the database.
Instead, procedure code must manage transactions explicitly with `ProcedureContext::with_tx`.

```rust
#[spacetimedb::table(name = my_table)]
struct MyTable {
    a: u32,
    b: String,
}

#[spacetimedb::procedure]
fn insert_a_value(ctx: &mut ProcedureContext, a: u32, b: String) {
    ctx.with_tx(|ctx| {
        ctx.my_table().insert(MyTable { a, b });
    });
}
```

`ProcedureContext::with_tx` takes a function of type `Fn(&TxContext) -> T`.
Within that function, the `&TxContext` can be used to access the database
[in all the same ways as a `ReducerContext`](https://docs.rs/spacetimedb/latest/spacetimedb/struct.ReducerContext.html).
When the function returns, the transaction will be committed,
and its changes to the database state will become permanent and be broadcast to clients.
If the function panics, the transaction will be rolled back, and its changes will be discarded.
However, for transactions that may fail,
[prefer calling `try_with_tx` and returning a `Result`](#fallible-database-operations) rather than panicking.

:::warning
The function passed to `ProcedureContext::with_tx` may be invoked multiple times,
possibly seeing a different version of the database state each time.

If invoked more than once with reference to the same database state,
it must perform the same operations and return the same result each time.

If invoked more than once with reference to different database states,
values observed during prior runs must not influence the behavior of the function or the calling procedure.

Avoid capturing mutable state within functions passed to `with_tx`.
:::

</TabItem>
<TabItem value="typescript" label="TypeScript">

TODO(noa)

</TabItem>
</Tabs>

### Fallible database operations

<Tabs groupId="server-language" queryString>
<TabItem value="rust" label="Rust">

For fallible database operations, instead use `ProcedureContext::try_with_tx`:

```rust
#[spacetimedb::procedure]
fn maybe_insert_a_value(ctx: &mut ProcedureContext, a: u32, b: String) {
    ctx.try_with_tx(|ctx| {
        if a < 10 {
            return Err("a is less than 10!");
        }
        ctx.my_table().insert(MyTable { a, b });
        Ok(())
    });
}
```

`ProcedureContext::try_with_tx` takes a function of type `Fn(&TxContext) -> Result<T, E>`.
If the function returns `Ok`, the transaction will be committed,
and its changes to the database state will become permanent and be broadcast to clients.
If that function returns `Err`, the transaction will be rolled back, and its changes will be discarded.

</TabItem>
<TabItem value="typescript" label="TypeScript">

TODO(noa)

</TabItem>
</Tabs>

### Reading values out of the database

<Tabs groupId="server-language" queryString>
<TabItem value="rust" label="Rust">

Functions passed to
[`ProcedureContext::with_tx`](#accessing-the-database) and [`ProcedureContext::try_with_tx`](#fallible-database-operations)
may return a value, and that value will be returned to the calling procedure.

Transaction return values are never saved or broadcast to clients, and are used only by the calling procedure.

```rust
#[spacetimedb::table(name = player)]
struct Player {
    id: spacetimedb::Identity,
    level: u32,
}

#[spacetimedb::procedure]
fn find_highest_level_player(ctx: &mut ProcedureContext) {
    let highest_level_player = ctx.with_tx(|ctx| {
        ctx.db.player().iter().max_by_key(|player| player.level)
    });
    match highest_level_player {
        Some(player) => log::info!("Congratulations to {}", player.id),
        None => log::warn!("No players..."),
    }
}
```

</TabItem>
<TabItem value="typescript" label="TypeScript">

TODO(noa)

</TabItem>
</Tabs>

## HTTP Requests

<Tabs groupId="server-language" queryString>
<TabItem value="rust" label="Rust">

Procedures can make HTTP requests to external services using methods contained in `ctx.http`.

`ctx.http.get` performs simple `GET` requests with no headers:

```rust
#[spacetimedb::procedure]
fn get_request(ctx: &mut ProcedureContext) {
    match ctx.http.get("https://example.invalid") {
        Ok(response) => log::info!(
            "Got response with status {} and body {}",
            response.status(),
            response.body().into_string_lossy(),
        ),
        Err(error) => log::error!("Request failed: {error:?}"),
    }
}
```

`ctx.http.send` sends any [`http::Request`](https://docs.rs/http/latest/http/request/struct.Request.html)
whose body can be converted to `spacetimedb::http::Body`.
`http::Request` is re-exported as `spacetimedb::http::Request`.

```rust
#[spacetimedb::procedure]
fn post_request(ctx: &mut spacetimedb::ProcedureContext) {
    let request = spacetimedb::http::Request::builder()
        .uri("https://example.invalid/upload")
        .method("POST")
        .header("Content-Type", "text/plain")
        .body("This is the body of the HTTP request")
        .expect("Building `Request` object failed");
    match ctx.http.send(request) {
        Ok(response) => log::info!(
            "Got response with status {} and body {}",
            response.status(),
            response.body().into_string_lossy(),
        ),
        Err(error) => log::error!("Request failed: {error:?}"),
    }
}
```

Each of these methods returns a [`http::Response`](https://docs.rs/http/latest/http/response/struct.Response.html#method.body)
containing a `spacetimedb::http::Body`. `http::Response` is re-exported as `spacetimedb::http::Response`.

Set a timeout for a `ctx.http.send` request by including a `spacetimedb::http::Timeout` as an [`extension`](https://docs.rs/http/latest/http/request/struct.Builder.html#method.extension):

```rust
#[spacetimedb::procedure]
fn get_request_with_short_timeout(ctx: &mut spacetimedb::ProcedureContext) {
    let request = spacetimedb::http::Request::builder()
        .uri("https://example.invalid")
        .method("GET")
        .extension(spacetimedb::http::Timeout(std::time::Duration::from_millis(10)))
        .expect("Building `Request` object failed");
    ctx.http.send(request).expect("HTTP request failed");
}
```

Procedures can't send requests at the same time as holding open a [transaction](#accessing-the-database).

</TabItem>
<TabItem value="typescript" label="TypeScript">

TODO(noa)

</TabItem>
</Tabs>

# Calling procedures

<Tabs groupId="client-language" queryString>
<TabItem value="rust" label="Rust">

Clients can invoke procedures using methods on `ctx.procedures`:

```rust
ctx.procedures.insert_a_value(12, "Foo".to_string());
```

</TabItem>
<TabItem value="csharp" label="C#">

Clients can invoke procedures using methods on `ctx.Procedures`:

```csharp
ctx.Procedures.InsertAValue(12, "Foo");
```

</TabItem>
<TabItem value="typescript" label="TypeScript">

Clients can invoke procedures using methods on `ctx.procedures`:

```typescript
ctx.procedures.insertAValue({ a: 12, b: "Foo" });
```

</TabItem>
<TabItem value="unreal" label="Unreal">

Clients can invoke procedures using methods on `Ctx->Procedures`:

```cpp
Ctx->Procedures->InsertAValue(12, "Foo");
```

</TabItem>
</Tabs>

## Observing return values

<Tabs groupId="client-language" queryString>
<TabItem value="rust" label="Rust">

A client can also invoke a procedure while registering a callback to run when it completes.
That callback will have access to the return value of the procedure,
or an error if the procedure fails.

```rust
ctx.procedures.add_two_numbers_then(1, 2, |ctx, result| {
    let sum = result.expect("Procedure failed");
    println!("1 + 2 = {sum}");
});
```

</TabItem>
<TabItem value="csharp" label="C#">

A client can also invoke a procedure while registering a callback to run when it completes.
That callback will have access to the return value of the procedure,
or an error if the procedure fails.

```csharp
ctx.Procedures.AddTwoNumbers(12, "Foo", (ctx, result) =>
{
    if (result.IsSuccess)
    {
        Log.Info($"1 + 2 = {result.Value!}");
    }
    else
    {
        throw result.Error!;
    }
});
```

</TabItem>
<TabItem value="typescript" label="TypeScript">

When a client invokes a procedure, it gets a [`Promise`](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/Promise) which resolves to the return value of the procedure.

```typescript
ctx.procedures.addTwoNumbers({ lhs: 1, rhs: 2 }).then(
    sum => console.log(`1 + 2 = {sum}`)
);
```

</TabItem>
<TabItem value="unreal" label="Unreal">

A client can also invoke a procedure while registering a callback to run when it completes.
That callback will have access to the return value of the procedure,
or an error if the procedure fails.

TODO

</TabItem>
</Tabs>

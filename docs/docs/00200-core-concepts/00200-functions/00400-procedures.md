---
title: Procedures
slug: /functions/procedures
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';


A **procedure** is a function exported by a [database](/databases), similar to a [reducer](/functions/reducers).
Connected [clients](/sdks) can call procedures.
Procedures can perform additional operations not possible in reducers, including making HTTP requests to external services.
However, procedures don't automatically run in database transactions,
and must manually open and commit a transaction in order to read from or modify the database state.
For this reason, prefer defining reducers rather than procedures unless you need to use one of the special procedure operators.

:::warning
***Procedures are currently in beta, and their API may change in upcoming SpacetimeDB releases.***
:::

## Defining Procedures

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

Define a procedure with `spacetimedb.procedure`:

```typescript
spacetimedb.procedure(
    "add_two_numbers",
    { lhs: t.u32(), rhs: t.u32() },
    t.u64(),
    (ctx, { lhs, rhs }) => BigInt(lhs) + BigInt(rhs),
);
```

The `spacetimedb.procedure` function takes:
* the procedure name,
* (optional) an object representing its parameter types,
* its return type,
* and the procedure function itself.

The function will receive a `ProcedureContext` and an object of its arguments, and it must return
a value corresponding to its return type. This return value will be sent to the caller, but will
not be broadcast to any other clients.

</TabItem>
<TabItem value="csharp"label="C#">

Support for procedures in C# modules is coming soon!

</TabItem>
<TabItem value="rust" label="Rust">

Because procedures are unstable, Rust modules that define them must opt in to the `unstable` feature in their `Cargo.toml`:

```toml
[dependencies]
spacetimedb = { version = "1.*", features = ["unstable"] }
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
</Tabs>

### Accessing the database

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

Unlike reducers, procedures don't automatically run in database transactions.
This means there's no `ctx.db` field to access the database.
Instead, procedure code must manage transactions explicitly with `ProcedureCtx.withTx`.

```typescript
const MyTable = table(
    { name: "my_table" },
    {
        a: t.u32(),
        b: t.string(),
    },
)

const spacetimedb = schema(MyTable);

#[spacetimedb::procedure]
spacetimedb.procedure("insert_a_value", { a: t.u32(), b: t.u32() }, t.unit(), (ctx, { a, b }) => {
    ctx.withTx(ctx => {
        ctx.myTable.insert({ a, b });
    });
    return {};
})
```

`ProcedureCtx.withTx` takes a function of `(ctx: TransactionCtx) => T`.
Within that function, the `TransactionCtx` can be used to access the database
[in all the same ways as a `ReducerCtx`](/functions/reducers/reducer-context)
When the function returns, the transaction will be committed,
and its changes to the database state will become permanent and be broadcast to clients.
If the function throws an error, the transaction will be rolled back, and its changes will be discarded.

:::warning
The function passed to `ProcedureCtx.withTx` may be invoked multiple times,
possibly seeing a different version of the database state each time.

If invoked more than once with reference to the same database state,
it must perform the same operations and return the same result each time.

If invoked more than once with reference to different database states,
values observed during prior runs must not influence the behavior of the function or the calling procedure.

Avoid capturing mutable state within functions passed to `withTx`.
:::

</TabItem>
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
</Tabs>

#### Fallible database operations

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

For fallible database operations, you can throw an error inside the transaction function:

```typescript
spacetimedb.procedure("maybe_insert_a_value", { a: t.u32(), b: t.string() }, t.unit(), (ctx, { a, b }) => {
    ctx.withTx(ctx => {
        if (a < 10) {
            throw new SenderError("a is less than 10!");
        }
        ctx.myTable.insert({ a, b });
    });
})
```

</TabItem>
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
</Tabs>

#### Reading values out of the database

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

Functions passed to
[`ProcedureCtx.withTx`](#accessing-the-database)
may return a value, and that value will be returned to the calling procedure.

Transaction return values are never saved or broadcast to clients, and are used only by the calling procedure.

```typescript
const Player = table(
    { name: "player" },
    {
        id: t.identity(),
        level: t.u32(),
    },
);

const spacetimedb = schema(Player);

spacetimedb.procedure("find_highest_level_player", t.unit(), ctx => {
    let highestLevelPlayer = ctx.withTx(ctx =>
        Iterator.from(ctx.db.player).reduce(
            (a, b) => a == null || b.level > a.level ? b : a,
            null
        )
    );
    if (highestLevelPlayer != null) {
        console.log("Congratulations to ", highestLevelPlayer.id);
    } else {
        console.warn("No players...");
    }
    return {};
});
```

</TabItem>
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
</Tabs>

## HTTP Requests

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

Procedures can make HTTP requests to external services using methods contained in `ctx.http`.

`ctx.http.fetch` is similar to the browser `fetch()` API, but is synchronous.

It can perform simple `GET` requests:

```typescript
#[spacetimedb::procedure]
spacetimedb.procedure("get_request", t.unit(), ctx => {
    try {
        const response = ctx.http.fetch("https://example.invalid");
        const body = response.text();
        console.log(`Got response with status ${response.status} and body ${body}`);
    } catch (e) {
        console.error("Request failed: ", e);
    }
    return {};
});
```

It can also accept an options object to specify a body, headers, HTTP method, and timeout:

```typescript
spacetimedb.procedure("post_request", t.unit(), ctx => {
    try {
        const response = ctx.http.fetch("https://example.invalid/upload", {
            method: "POST",
            headers: { "Content-Type": "text/plain" },
            body: "This is the body of the HTTP request",
        });
        const body = response.text();
        console.log(`Got response with status ${response.status} and body {body}`);
    } catch (e) {
        console.error("Request failed: ", e);
    }
    return {};
});

spacetimedb.procedure("get_request_with_short_timeout", t.unit(), ctx => {
    try {
        const response = ctx.http.fetch("https://example.invalid", {
            method: "GET",
            timeout: TimeDuration.fromMillis(10),
        });
        const body = response.text();
        console.log(`Got response with status ${response.status} and body {body}`);
    } catch (e) {
        console.error("Request failed: ", e);
    }
    return {};
});
```

Procedures can't send requests at the same time as holding open a [transaction](#accessing-the-database).

</TabItem>
<TabItem value="rust" label="Rust">

Procedures can make HTTP requests to external services using methods contained in `ctx.http`.

`ctx.http.get` performs simple `GET` requests with no headers:

```rust
#[spacetimedb::procedure]
fn get_request(ctx: &mut ProcedureContext) {
    match ctx.http.get("https://example.invalid") {
        Ok(response) => {
            let (response, body) = response.into_parts();
            log::info!(
                "Got response with status {} and body {}",
                response.status,
                body.into_string_lossy(),
            )
        },
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
        Ok(response) => {
            let (response, body) = response.into_parts();
            log::info!(
                "Got response with status {} and body {}",
                response.status,
                body.into_string_lossy(),
            )
        }
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
        // Set a timeout of 10 ms.
        .extension(spacetimedb::http::Timeout(std::time::Duration::from_millis(10).into()))
        // Empty body for a `GET` request.
        .body(())
        .expect("Building `Request` object failed");
    ctx.http.send(request).expect("HTTP request failed");
}
```

Procedures can't send requests at the same time as holding open a [transaction](#accessing-the-database).

</TabItem>
</Tabs>

## Calling procedures

<Tabs groupId="client-language" queryString>
<TabItem value="typescript" label="TypeScript">

Clients can invoke procedures using methods on `ctx.procedures`:

```typescript
ctx.procedures.insertAValue({ a: 12, b: "Foo" });
```

</TabItem>
<TabItem value="csharp" label="C#">

Clients can invoke procedures using methods on `ctx.Procedures`:

```csharp
ctx.Procedures.InsertAValue(12, "Foo");
```

</TabItem>
<TabItem value="rust" label="Rust">

Clients can invoke procedures using methods on `ctx.procedures`:

```rust
ctx.procedures.insert_a_value(12, "Foo".to_string());
```

</TabItem>
<TabItem value="cpp" label="Unreal C++">

Clients can invoke procedures using methods on `ctx.Procedures`:

```cpp
Context.Procedures->InsertAValue(12, TEXT("Foo"), {});
```

</TabItem>
<TabItem value="blueprint" label="Unreal Blueprint">

Clients can invoke procedures using methods on `ctx.Procedures`:

![Calling Procedures](/images/unreal/procedures/ue-blueprint-calling-procedure.png)

</TabItem>
</Tabs>

### Observing return values

<Tabs groupId="client-language" queryString>
<TabItem value="typescript" label="TypeScript">

When a client invokes a procedure, it gets a [`Promise`](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/Promise) which resolves to the return value of the procedure.

```typescript
ctx.procedures.addTwoNumbers({ lhs: 1, rhs: 2 }).then(
    sum => console.log(`1 + 2 = ${sum}`)
);
```

</TabItem>
<TabItem value="csharp" label="C#">

A client can also invoke a procedure while registering a callback to run when it completes.
That callback will have access to the return value of the procedure,
or an error if the procedure fails.

```csharp
ctx.Procedures.AddTwoNumbers(1, 2, (ctx, result) =>
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
<TabItem value="cpp" label="Unreal C++">

A client can also invoke a procedure while registering a callback to run when it completes.
That callback will have access to the return value of the procedure,
or an error if the procedure fails.

```cpp
{
    ...    
    FOnAddTwoNumbersComplete ReturnCallback;
    BIND_DELEGATE_SAFE(ReturnCallback, this, AGameManager, OnAddTwoNumbersComplete);
    Context.Procedures->AddTwoNumbers(1, 2, ReturnCallback);
}

void AGameManager::OnAddTwoNumbersComplete(const FProcedureEventContext& Context, int32 Result, bool bSuccess)
{
    if (bSuccess)
    {
        UE_LOG(LogTemp, Log, TEXT("1 + 2 = %d"), Result);
    }
    else
    {
        if (Context.Event.Status.IsInternalError())
        {
            UE_LOG(LogTemp, Error, TEXT("Error: %s"), *Context.Event.Status.GetAsInternalError());
            return;
        }
        UE_LOG(LogTemp, Error, TEXT("Out of energy!"));
    }
}
```

</TabItem>
<TabItem value="blueprint" label="Unreal Blueprint">

A client can also invoke a procedure while registering a callback to run when it completes.
That callback will have access to the return value of the procedure,
or an error if the procedure fails.

![Procedure Callbacks](/images/unreal/procedures/ue-blueprint-procedure-callback.png)

</TabItem>
</Tabs>

## Example: Calling an External AI API

A common use case for procedures is integrating with external APIs like OpenAI's ChatGPT. Here's a complete example showing how to build an AI-powered chat feature.

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
import { schema, t, table, SenderError } from 'spacetimedb/server';

const AiMessage = table(
  { name: 'ai_message', public: true },
  {
    user: t.identity(),
    prompt: t.string(),
    response: t.string(),
    createdAt: t.timestamp(),
  }
);

const spacetimedb = schema(AiMessage);

spacetimedb.procedure(
  'ask_ai',
  { prompt: t.string(), apiKey: t.string() },
  t.string(),
  (ctx, { prompt, apiKey }) => {
    // Make the HTTP request to OpenAI
    const response = ctx.http.fetch('https://api.openai.com/v1/chat/completions', {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        'Authorization': `Bearer ${apiKey}`,
      },
      body: JSON.stringify({
        model: 'gpt-4',
        messages: [{ role: 'user', content: prompt }],
      }),
    });

    if (response.status !== 200) {
      throw new SenderError(`API returned status ${response.status}`);
    }

    const data = response.json();
    const aiResponse = data.choices?.[0]?.message?.content;

    if (!aiResponse) {
      throw new SenderError('Failed to parse AI response');
    }

    // Store the conversation in the database
    ctx.withTx(txCtx => {
      txCtx.db.aiMessage.insert({
        user: txCtx.sender,
        prompt,
        response: aiResponse,
        createdAt: txCtx.timestamp,
      });
    });

    return aiResponse;
  }
);
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
use spacetimedb::{table, procedure, ProcedureContext, Identity, Timestamp};

#[table(name = ai_message, public)]
pub struct AiMessage {
    user: Identity,
    prompt: String,
    response: String,
    created_at: Timestamp,
}

#[procedure]
pub fn ask_ai(ctx: &mut ProcedureContext, prompt: String, api_key: String) -> Result<String, String> {
    // Build the request to OpenAI's API
    let request_body = format!(
        r#"{{"model": "gpt-4", "messages": [{{"role": "user", "content": "{}"}}]}}"#,
        prompt.replace('"', "\\\"")
    );

    let request = spacetimedb::http::Request::builder()
        .uri("https://api.openai.com/v1/chat/completions")
        .method("POST")
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", api_key))
        .body(request_body)
        .map_err(|e| format!("Failed to build request: {e}"))?;

    // Make the HTTP request
    let response = ctx.http.send(request)
        .map_err(|e| format!("HTTP request failed: {e:?}"))?;

    let (parts, body) = response.into_parts();

    if parts.status != 200 {
        return Err(format!("API returned status {}", parts.status));
    }

    let body_str = body.into_string_lossy();

    // Parse the response (simplified - in production use serde_json)
    let ai_response = extract_content(&body_str)
        .ok_or("Failed to parse AI response")?;

    // Store the conversation in the database
    ctx.with_tx(|tx_ctx| {
        tx_ctx.db.ai_message().insert(AiMessage {
            user: tx_ctx.sender,
            prompt: prompt.clone(),
            response: ai_response.clone(),
            created_at: tx_ctx.timestamp,
        });
    });

    Ok(ai_response)
}

fn extract_content(json: &str) -> Option<String> {
    // Simple extraction - in production, use proper JSON parsing
    let content_start = json.find("\"content\":")? + 11;
    let content_end = json[content_start..].find('"')? + content_start;
    Some(json[content_start..content_end].to_string())
}
```

</TabItem>
</Tabs>

### Calling from a client

<Tabs groupId="client-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
// Call the procedure and wait for the AI response
const response = await ctx.procedures.askAi({
  prompt: "What is SpacetimeDB?",
  apiKey: process.env.OPENAI_API_KEY,
});

console.log("AI says:", response);
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
ctx.procedures.ask_ai_then(
    "What is SpacetimeDB?".to_string(),
    api_key,
    |_ctx, result| {
        match result {
            Ok(response) => println!("AI says: {}", response),
            Err(e) => eprintln!("Error: {:?}", e),
        }
    },
);
```

</TabItem>
</Tabs>

:::warning
**Security note:** Never hardcode API keys in your client code. Consider storing them securely on the server side or using environment variables during development.
:::

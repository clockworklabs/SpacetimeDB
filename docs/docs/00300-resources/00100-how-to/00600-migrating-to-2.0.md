---
title: Migrating from 1.0 to 2.0
slug: /how-to/migrating-to-2-0
---

# Migrating from SpacetimeDB 1.0 to 2.0

This guide covers the breaking changes between SpacetimeDB 1.0 and 2.0 and how to update your code.

## Overview of Changes

SpacetimeDB 2.0 introduces a new WebSocket protocol (v2) and SDK with several breaking changes aimed at simplifying the programming model and improving security:

1. **Reducer callbacks removed** -- replaced by event tables and per-call `_then()` callbacks
2. **`light_mode` removed** -- no longer necessary since reducer events are no longer broadcast
3. **`CallReducerFlags` removed** -- `NoSuccessNotify` and `set_reducer_flags()` are gone
4. **Event tables introduced** -- a new table type for publishing transient events to subscribers

## Reducer Callbacks

### What changed

In 1.0, you could register global callbacks on reducers that would fire whenever *any* client called that reducer and you were subscribed to affected rows:

<Tabs groupId="client-language" queryString>
<TabItem value="typescript" label="TypeScript">

TODO

</TabItem>
<TabItem value="csharp" label="C#">

TODO

</TabItem>
<TabItem value="rust" label="Rust">

```rust
// 1.0 -- REMOVED in 2.0
conn.reducers.on_deal_damage(|ctx, target, amount| {
    println!("Someone called deal_damage with args: ({target}, {amount})");
});
```

</TabItem>
</Tabs>

In 2.0, global reducer callbacks no longer exist. The server does not broadcast reducer argument data to other clients. Instead, you have two options:

### Option A: Per-call result callbacks (`_then()`)

If you only need to know the result of a reducer *you* called, use the `_then()` variant:

<Tabs groupId="client-language" queryString>
<TabItem value="typescript" label="TypeScript">

TODO

</TabItem>
<TabItem value="csharp" label="C#">

TODO

</TabItem>
<TabItem value="rust" label="Rust">

```rust
// 2.0 -- per-call callback
ctx.reducers.deal_damage_then(target, amount, |ctx, result| {
    match result {
        Ok(Ok(())) => println!("Reducer succeeded"),
        Ok(Err(err)) => println!("Reducer failed: {err}"),
        Err(internal) => println!("Internal error: {internal:?}"),
    }
}).unwrap();
```

</TabItem>
</Tabs>


The fire-and-forget form still works:

```rust
// 2.0 -- fire and forget (unchanged)
ctx.reducers.deal_damage(target, amount).unwrap();
```

### Option B: Event tables (recommended for most use cases)

If you need *other* clients to observe that something happened (the primary use case for 1.0 reducer callbacks), create an event table and insert into it from your reducer.

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

TODO

</TabItem>
<TabItem value="csharp" label="C#">

TODO

</TabItem>
<TabItem value="rust" label="Rust">

**Server (module) -- before:**
```rust
// 1.0 server -- reducer args were automatically broadcast
#[reducer]
fn deal_damage(ctx: &ReducerContext, target: Identity, amount: u32) {
    // update game state...
}
```

**Server (module) -- after:**
```rust
// 2.0 server -- explicitly publish events via an event table
#[spacetimedb::table(accessor = damage_event, public, event)]
pub struct DamageEvent {
    pub target: Identity,
    pub amount: u32,
}

#[reducer]
fn deal_damage(ctx: &ReducerContext, target: Identity, amount: u32) {
    // update game state...

    ctx.db.damage_event().insert(DamageEvent { target, amount });
}
```

</TabItem>
</Tabs>

<Tabs groupId="client-language" queryString>
<TabItem value="typescript" label="TypeScript">

TODO

</TabItem>
<TabItem value="csharp" label="C#">

TODO

</TabItem>
<TabItem value="rust" label="Rust">

**Client -- before:**
```rust
// 1.0 client -- global reducer callback
conn.reducers.on_deal_damage(|ctx, target, amount| {
    play_damage_animation(target, amount);
});
```

**Client -- after:**
```rust
// 2.0 client -- event table callback
conn.db.damage_event().on_insert(|ctx, event| {
    play_damage_animation(event.target, event.amount);
});
```

</TabItem>
</Tabs>


### Why event tables are better

- **Security**: You control exactly what data is published. In 1.0, reducer arguments were broadcast to any subscriber of affected rows, which could accidentally leak sensitive data.
- **Flexibility**: Multiple reducers can insert the same event type. In 1.0, events were tied 1:1 to a specific reducer.
- **Transactional**: Events are only published if the transaction commits. In 1.0, workarounds using scheduled reducers were not transactional.
- **Row-level security**: RLS rules apply to event tables, so you can control which clients see which events.
- **Queryable**: Event tables can be subscribed to with query builders (or SQL), and can be filtered per-client.

### Event table details

- Event tables are always empty outside of a transaction. They don't accumulate rows.
- On the client, `count()` always returns 0 and `iter()` is always empty.
- Only `on_insert` callbacks are generated (no `on_delete` or `on_update`).
- The `event` keyword in `#[table(..., event)]` marks the table as transient.
- Event tables must be subscribed to explicitly (they are excluded from `subscribeToAllTables` / `SubscribeToAllTables` / `subscribe_to_all_tables`).

## Event Type Changes

### What changed

<Tabs groupId="client-language" queryString>
<TabItem value="typescript" label="TypeScript">

TODO

</TabItem>
<TabItem value="csharp" label="C#">

TODO

</TabItem>
<TabItem value="rust" label="Rust">

In 1.0, table callbacks received `Event::Reducer` with full reducer information when a reducer caused a table change. Non-callers could also receive `Event::UnknownTransaction`.

In 2.0, the event model is simplified:

- **The caller** sees `Event::Reducer` with `ReducerEvent { timestamp, status, reducer }` in response to their own reducer calls.
- **Other clients** see `Event::Transaction` (no reducer details).
- `Event::UnknownTransaction` is removed.

```rust
// 2.0 -- checking who caused a table change
conn.db.my_table().on_insert(|ctx, row| {
    match &ctx.event {
        Event::Reducer(reducer_event) => {
            // This client called the reducer that caused this insert.
            println!("Our reducer: {:?}", reducer_event.reducer);
        }
        Event::Transaction => {
            // Another client's action caused this insert.
        }
        _ => {}
    }
});
```

</TabItem>
</Tabs>

If you need metadata about reducers invoked by other clients, update your reducer code to emit an event using an event table.

## Subscription API

The subscription API is largely unchanged:

<Tabs groupId="client-language" queryString>
<TabItem value="typescript" label="TypeScript">

TODO

</TabItem>
<TabItem value="csharp" label="C#">

TODO

</TabItem>
<TabItem value="rust" label="Rust">

```rust
// 2.0 -- same as 1.0
ctx.subscription_builder()
    .on_applied(|ctx| { /* ... */ })
    .on_error(|ctx, error| { /* ... */ })
    .add_query(|q| q.from.my_table())
    .subscribe();
```

</TabItem>
</Tabs>

Note that subscribing to event tables requires an explicit query:

<Tabs groupId="client-language" queryString>
<TabItem value="typescript" label="TypeScript">

TODO

</TabItem>
<TabItem value="csharp" label="C#">

TODO

</TabItem>
<TabItem value="rust" label="Rust">

```rust
// Event tables are excluded from subscribe_to_all_tables(), so subscribe explicitly:
ctx.subscription_builder()
    .on_applied(|ctx| { /* ... */ })
    .add_query(|q| q.from.damage_event())
    .subscribe();
```

</TabItem>
</Tabs>

## Table Name Canonicalization

### What changed

SpacetimeDB 2.0 no longer equates the canonical name of your tables and indexes with the accessor method you use in module or client code. The canonical name is largely an internal detail, but you may encounter it when making SQL queries, or in the migration plans printed by `spacetime publish`.

### Updating source code: Change `name` to `accessor` in table definitions

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

TODO

</TabItem>
<TabItem value="csharp" label="C#">

TODO

</TabItem>
<TabItem value="rust" label="Rust">

The `name` argument to table definitions is now used to overwrite the canonical name, and is optional. The `accessor` argument controls the method names you write in your module and client source code.

By default, the canonical name is derived from the accessor by converting it to snake case.

To migrate a 1.0 table definition to 2.0, replace `name =` with `accessor =` in the table and index definitions:

```rust
// 1.0 -- NO LONGER VALID in 2.0
#[spacetimedb::table(
    name = my_table,
    public,
    index(
        name = position,
        btree(columns = [x, y]),
    )
)]
struct MyTable {
    #[primary_key]
    #[auto_inc]
    id: u32,
    x: u32,
    y: u32,
}

// 2.0
#[spacetimedb::table(
    accessor= my_table,
    public,
    index(
        accessor = position,
        btree(columns = [x, y]),
    )
)]
struct MyTable {
    #[primary_key]
    #[auto_inc]
    id: u32,
    x: u32,
    y: u32,
}
```

</TabItem>
</Tabs>

### Auto-migrating existing databases

The new default behavior for canonicalizing names may not be compatible with existing 1.0 databases, as it may change the casing of table names, which would require a manual migration.

#### Option 1: Disable case conversion

To avoid this manual migration, configure the case conversion policy in your module to not convert, which will result in the same table names as a 1.0 module:

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

export const moduleSettings: ModuleSettings = {
  caseConversionPolicy: CaseConversionPolicy.None,
};

</TabItem>
<TabItem value="csharp" label="C#">

TODO

</TabItem>
<TabItem value="rust" label="Rust">

```rust
use spacetimedb::CaseConversionPolicy;

#[spacetimedb::settings]
const CASE_CONVERSION_POLICY: CaseConversionPolicy = CaseConversionPolicy::None;
```

</TabItem>
</Tabs>

#### Option 2: overwrite the name of individual tables

Alternatively, manually specify the correct canonical name of each table:

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

</TabItem>
<TabItem value="csharp" label="C#">


</TabItem>
<TabItem value="rust" label="Rust">
```rust
#[spacetimedb::table(
    accessor = my_table,
    name = "MyTable",
    public,
    index(
        accessor = position,
        btree(columns = [x, y]),
    )
)]
struct MyTable {
    #[primary_key]
    #[auto_inc]
    id: u32,
    x: u32,
    y: u32,
}
```

</TabItem>
</Tabs>

## Clients connect with database name

<Tabs groupId="client-language" queryString>
<TabItem value="typescript" label="TypeScript">

When constructing a `DbConnection` to a remote database, you now use `withDatabaseName` to provide the database name, rather than `withModuleName`. This is a more accurate terminology.

TODO: code snippet


</TabItem>
<TabItem value="csharp" label="C#">

When constructing a `DbConnection` to a remote database, you now use `WithDatabaseName` to provide the database name, rather than `WithModuleName`. This is a more accurate terminology.

TODO: code snippet

</TabItem>
<TabItem value="rust" label="Rust">

When constructing a `DbConnection` to a remote database, you now use `with_database_name` to provide the database name, rather than `with_module_name`. This is a more accurate terminology.

```rust
// 1.0 -- NO LONGER CORRECT
let conn = DbConnection::builder()
    .with_uri("https://maincloud.spacetimedb.com")
    .with_module_name("my-database")
    // other options...
    .build();

// 2.0
let conn = DbConnection::builder()
    .with_uri("https://maincloud.spacetimedb.com")
    .with_database_name("my-database")
    // other options...
    .build()
```

</TabItem>
</Tabs>

## `sender` Is Now A Method, Not A Field

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

This change does not apply to TypeScript, where properties are indistinguishable from fields.

</TabItem>
<TabItem value="csharp" label="C#">

This change does not apply to C#, where properties are indistinguishable from fields.

</TabItem>
<TabItem value="rust" label="Rust">

In Rust modules, the sender of a request is no longer exposed via a field `ctx.sender` on `ReducerContext`, `ProcedureContext`, `ViewContext` or `AnonymousViewContext`. Instead, each of these types has a method `ctx.sender()` which returns the sender's identity.

```rust
// 1.0 -- NO LONGER CORRECT
#[spacetimedb::reducer]
fn my_reducer(ctx: &ReducerContext) {
    let sender_identity = ctx.sender;
    // Do stuff with `sender_identity`...
}

// 2.0
#[spacetimedb::reducer]
fn my_reducer(ctx: &ReducerContext) {
    let sender_identity = ctx.sender();
    // Do stuff with `sender_identity`...
}
```
</TabItem>
</Tabs>

## Only Primary Keys Have Update Methods

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">


</TabItem>
<TabItem value="csharp" label="C#">



</TabItem>
<TabItem value="rust" label="Rust">

In 2.0 modules, only `#[primary_key]` indexes expose an `update` method, whereas previously, `#[unique]` indexes also provided that method. The previous behavior led to confusion, as only updates which preserved the value in the primary key column resulted in `on_update` callbacks being invoked on the client.

### Updates which preserve the primary key - update with the primary key index

```rust
#[spacetimedb::table(accessor = user)]
struct User {
    #[primary_key]
    identity: Identity,

    #[unique]
    name: String,

    apples_owned: u32,
}

// 1.0 -- REMOVED in 2.0
#[spacetimedb::reducer]
fn add_apple(ctx: &ReducerContext, name: String) {
    let user = ctx.db.user().name().find(&name).unwrap();
    ctx.db.user().name().update(User {
        apples_owned: user.apples_owned + 1,
        ..user
    });
}

// 2.0
#[spacetimedb::reducer]
fn add_apple(ctx: &ReducerContext, name: String) {
    let user = ctx.db.user().name().find(&name).unwrap();
    ctx.db.user().identity().update(User {
        apples_owned: user.apples_owned + 1,
        ..user
    });
}
```

### Updates which change the primary key - explicitly delete and insert

```rust
#[spacetimedb::table(accessor = user)]
struct User {
    #[primary_key]
    identity: Identity,

    #[unique]
    name: String,

    apples_owned: u32,
}

// 1.0 -- REMOVED in 2.0
#[spacetimedb::reducer]
fn change_user_identity(ctx: &ReducerContext, name: String, identity: Identity) {
    let user = ctx.db.user().name().find(&name).unwrap();
    ctx.db.user().name().update(User {
        identity,
        ..user
    });
}

// 2.0
fn change_user_identity(ctx: &ReducerContext, name: String, identity: Identity) {
    let user = ctx.db.user().name().find(&name).unwrap();
    ctx.db.user().delete(user.clone());
    ctx.db.user().insert(User {
        identity,
        ..user
    });
}
```

</TabItem>
</Tabs>

## Scheduled Functions Are Now Private

Scheduled reducers and procedures are now private by default, meaning that only the database owner and team collaborators can bypass the schedule table to invoke them manually.

### Remove authorization logic from scheduled functions

Because scheduled reducers and procedures are now private, it's no longer necessary to explicitly check that the sender is the database itself.

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

</TabItem>
<TabItem value="csharp" label="C#">

</TabItem>
<TabItem value="rust" label="Rust">

```rust
#[spacetimedb::table(accessor = my_timer, scheduled(run_my_timer))]
struct MyTimer {
    #[primary_key]
    #[auto_inc]
    scheduled_id: u64,
    scheduled_at: spacetimedb::ScheduleAt,
}

// 1.0 - SUPERFLUOUS
#[spacetimedb::reducer]
fn run_my_timer(ctx: &ReducerContext, timer: MyTimer) -> Result<(), String> {
    if ctx.sender() != ctx.identity() {
        return Err("`run_my_timer` should only be invoked by the database!".to_string());
    }
    // Do stuff...
    Ok(())
}

// 2.0
#[spacetimedb::reducer]
fn run_my_timer(ctx: &ReducerContext, timer: MyTimer) {
    // Do stuff...
}
```

</TabItem>
</Tabs>

### Define wrappers for functions that are both scheduled and invoked by clients

In the rare event that you have a reducer or procedure which is intended to be invoked by both clients and a schedule table, define a new public reducer or procedure which wraps the scheduled function.

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">


</TabItem>
<TabItem value="csharp" label="C#">

</TabItem>
<TabItem value="rust" label="Rust">

```rust
#[spacetimedb::table(accessor = my_timer, scheduled(run_my_timer_private))]
struct MyTimer {
    #[primary_key]
    #[auto_inc]
    scheduled_id: u64,
    scheduled_at: spacetimedb::ScheduleAt,
}

#[spacetimedb::reducer]
fn run_my_timer_private(ctx: &ReducerContext, timer: MyTimer) {
    // Do stuff...
}

#[spacetimedb::reducer]
fn run_my_timer(ctx: &ReducerContext, timer: MyTimer) {
    run_my_timer_private(ctx, timer)
}
```

</TabItem>
</Tabs>

## Private Items Are Not Code-Generated By Default

Starting in SpacetimeDB 2.0, `spacetime generate` will not generate bindings for private tables or functions by default. These bindings were confusing, as only clients authenticated as the database owner or a collaborator could access them, with most clients seeing an error when trying to subscribe to a private table or invoke a private function.

### Pass `--include-private` to `spacetime generate`

For clients which rely on generated bindings to private tables or functions, pass the `--include-private` flag to the `spacetime generate` CLI command.

## Light Mode

### What changed

In 1.0, `light_mode` prevented the server from sending reducer event data to a client (unless that client was the caller):

<Tabs groupId="client-language" queryString>
<TabItem value="typescript" label="TypeScript">

TODO

</TabItem>
<TabItem value="csharp" label="C#">

TODO

</TabItem>
<TabItem value="rust" label="Rust">

```rust
// 1.0 -- REMOVED in 2.0
DbConnection::builder()
    .with_light_mode(true)
    // ...
```

</TabItem>
</Tabs>

In 2.0, the server never broadcasts reducer argument data to any client, so `light_mode` is no longer necessary. Simply remove the call:

<Tabs groupId="client-language" queryString>
<TabItem value="typescript" label="TypeScript">

TODO

</TabItem>
<TabItem value="csharp" label="C#">

TODO

</TabItem>
<TabItem value="rust" label="Rust">

```rust
// 2.0
DbConnection::builder()
    .with_uri(uri)
    .with_module_name(name)
    // no with_light_mode needed
    .build()
```

</TabItem>
</Tabs>

## CallReducerFlags

### What changed

In 1.0, you could suppress success notifications for individual reducer calls:

<Tabs groupId="client-language" queryString>
<TabItem value="typescript" label="TypeScript">

TODO

</TabItem>
<TabItem value="csharp" label="C#">

TODO

</TabItem>
<TabItem value="rust" label="Rust">

```rust
// 1.0 -- REMOVED in 2.0
ctx.set_reducer_flags(CallReducerFlags::NoSuccessNotify);
ctx.reducers.my_reducer(args).unwrap();
```

</TabItem>
</Tabs>


In 2.0, the success notification is lightweight (just `request_id` and `timestamp`, no reducer args or full event data), so there is no need to suppress it. Remove any `set_reducer_flags` calls and `CallReducerFlags` imports.

## Quick Migration Checklist

- [ ] Remove all `ctx.reducers.on_<reducer>()` calls
  - Replace with `_then()` callbacks for your own reducer calls
  - Replace with event tables + `on_insert` for cross-client notifications
- [ ] Update `Event::UnknownTransaction` matches to `Event::Transaction`
- [ ] For each reducer whose args you were observing from other clients:
  1. Create an `#[table(..., event)]` on the server
  2. Insert into it from the reducer
  3. Subscribe to it on the client
  4. Use `on_insert` instead of the old reducer callback
- [ ] Replace `name =` with `accessor =` in table and index definitions
- [ ] Set your module's case conversion policy to `None`
- [ ] Change `with_module_name` to `with_database_name`
- [ ] Change `ctx.sender` to `ctx.sender()`
  - Only necessary in Rust modules.
- [ ] Remove `update` calls on non-primary key unique indexes
  - When leaving the primary key value unchanged, update using the primary key index
  - When altering the primary key value, delete and insert
- [ ] Remove superfluous auth logic from scheduled functions which are not called by clients
- [ ] Define wrappers around scheduled functions which are called by clients
- [ ] Use `spacetime generate --include-private` if you rely on bindings for private tables or functions
- [ ] Remove `with_light_mode()` from `DbConnectionBuilder`
- [ ] Remove `set_reducer_flags()` calls and `CallReducerFlags` imports
- [ ] Remove `unstable::CallReducerFlags` from imports

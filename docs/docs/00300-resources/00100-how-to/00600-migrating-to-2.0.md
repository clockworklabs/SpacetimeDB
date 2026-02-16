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

```rust
// 1.0 -- REMOVED in 2.0
conn.reducers.on_insert_one_u_8(|ctx, arg| {
    println!("Someone called insert_one_u_8 with arg: {}", arg);
});
```

In 2.0, global reducer callbacks no longer exist. The server does not broadcast reducer argument data to other clients. Instead, you have two options:

### Option A: Per-call result callbacks (`_then()`)

If you only need to know the result of a reducer *you* called, use the `_then()` variant:

```rust
// 2.0 -- per-call callback
ctx.reducers.insert_one_u_8_then(42, |ctx, result| {
    match result {
        Ok(Ok(())) => println!("Reducer succeeded"),
        Ok(Err(err)) => println!("Reducer failed: {err}"),
        Err(internal) => println!("Internal error: {internal:?}"),
    }
}).unwrap();
```

The fire-and-forget form still works:

```rust
// 2.0 -- fire and forget (unchanged)
ctx.reducers.insert_one_u_8(42).unwrap();
```

### Option B: Event tables (recommended for most use cases)

If you need *other* clients to observe that something happened (the primary use case for 1.0 reducer callbacks), create an event table and insert into it from your reducer.

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
#[spacetimedb::table(name = damage_event, public, event)]
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

### Why event tables are better

- **Security**: You control exactly what data is published. In 1.0, reducer arguments were broadcast to any subscriber of affected rows, which could accidentally leak sensitive data.
- **Flexibility**: Multiple reducers can insert the same event type. In 1.0, events were tied 1:1 to a specific reducer.
- **Transactional**: Events are only published if the transaction commits. In 1.0, workarounds using scheduled reducers were not transactional.
- **Row-level security**: RLS rules apply to event tables, so you can control which clients see which events.
- **Queryable**: Event tables are subscribed to with standard SQL, and can be filtered per-client.

### Event table details

- Event tables are always empty outside of a transaction. They don't accumulate rows.
- On the client, `count()` always returns 0 and `iter()` is always empty.
- Only `on_insert` callbacks are generated (no `on_delete` or `on_update`).
- The `event` keyword in `#[table(..., event)]` marks the table as transient.
- Event tables must be subscribed to explicitly (they are excluded from `SELECT * FROM *`).

## Light Mode

### What changed

In 1.0, `light_mode` prevented the server from sending reducer event data to a client (unless that client was the caller):

```rust
// 1.0 -- REMOVED in 2.0
DbConnection::builder()
    .with_light_mode(true)
    // ...
```

In 2.0, the server never broadcasts reducer argument data to any client, so `light_mode` is no longer necessary. Simply remove the call:

```rust
// 2.0
DbConnection::builder()
    .with_uri(uri)
    .with_module_name(name)
    // no with_light_mode needed
    .build()
```

## CallReducerFlags

### What changed

In 1.0, you could suppress success notifications for individual reducer calls:

```rust
// 1.0 -- REMOVED in 2.0
ctx.set_reducer_flags(CallReducerFlags::NoSuccessNotify);
ctx.reducers.my_reducer(args).unwrap();
```

In 2.0, the success notification is lightweight (just `request_id` and `timestamp`, no reducer args or full event data), so there is no need to suppress it. Remove any `set_reducer_flags` calls and `CallReducerFlags` imports.

## Event Type Changes

### What changed

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

## Subscription API

The subscription API is largely unchanged:

```rust
// 2.0 -- same as 1.0
ctx.subscription_builder()
    .on_applied(|ctx| { /* ... */ })
    .on_error(|ctx, error| { /* ... */ })
    .subscribe(["SELECT * FROM my_table"]);
```

Note that subscribing to event tables requires an explicit query:

```rust
// Event tables are excluded from SELECT * FROM *, so subscribe explicitly:
ctx.subscription_builder()
    .on_applied(|ctx| { /* ... */ })
    .subscribe(["SELECT * FROM damage_event"]);
```

## Quick Migration Checklist

- [ ] Remove all `ctx.reducers.on_<reducer>()` calls
  - Replace with `_then()` callbacks for your own reducer calls
  - Replace with event tables + `on_insert` for cross-client notifications
- [ ] Remove `with_light_mode()` from `DbConnectionBuilder`
- [ ] Remove `set_reducer_flags()` calls and `CallReducerFlags` imports
- [ ] Remove `unstable::CallReducerFlags` from imports
- [ ] Update `Event::UnknownTransaction` matches to `Event::Transaction`
- [ ] For each reducer whose args you were observing from other clients:
  1. Create an `#[table(..., event)]` on the server
  2. Insert into it from the reducer
  3. Subscribe to it on the client
  4. Use `on_insert` instead of the old reducer callback

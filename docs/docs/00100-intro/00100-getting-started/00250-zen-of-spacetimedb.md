---
title: Zen of SpacetimeDB
slug: /intro/zen
---

# The Zen of SpacetimeDB

SpacetimeDB is built on a simple philosophy: **let your worries melt away**. These principles guide how SpacetimeDB works and how you should think about building applications with it.

## Everything is a Table

Your entire application state lives in tables. Users, messages, game entities, sessions—all tables. There's no separate cache layer, no Redis, no in-memory state that needs to be synchronized with a database. The database *is* your state.

This simplifies your mental model dramatically. When you need to store something, you define a table. When you need to query something, you query a table. When you need to update something, you update a table.

```
Traditional stack:        SpacetimeDB:
┌─────────────────┐       ┌─────────────────┐
│   Application   │       │                 │
├─────────────────┤       │                 │
│      Cache      │  →    │     Tables      │
├─────────────────┤       │                 │
│    Database     │       │                 │
└─────────────────┘       └─────────────────┘
```

## Everything is Persistent

Stop worrying about data loss. SpacetimeDB holds all your data in memory for blazing-fast access, but automatically persists everything to disk. You get the speed of in-memory computing with the durability of a traditional database.

Write your code as if memory were infinite and permanent. Insert rows freely. Query without fear. SpacetimeDB handles the persistence—you handle the logic.

## Everything is Real-Time

Think of your client as a **replica** of your server. When you subscribe to data, SpacetimeDB mirrors that data to your client and keeps it synchronized automatically. You don't poll. You don't fetch. You subscribe, and the data flows.

```typescript
// Subscribe once
const [messages] = useTable(tables.message);

// messages updates automatically when the server state changes
// No polling. No refetching. Just reactive data.
```

This isn't just convenient—it changes how you think about client-server communication. Stop thinking in terms of requests and responses. Think in terms of **synchronized state**.

## Everything is Transactional

Every reducer runs in a transaction. If something goes wrong, just throw an error (or return `Err`). All your changes roll back automatically. No partial updates. No corrupted state. No cleanup code.

```rust
#[spacetimedb::reducer]
fn transfer_funds(ctx: &ReducerContext, from: u64, to: u64, amount: u64) -> Result<(), String> {
    let sender = ctx.db.account().id().find(from).ok_or("Sender not found")?;
    if sender.balance < amount {
        return Err("Insufficient funds".to_string()); // Everything rolls back
    }
    // ... rest of transfer
    Ok(())
}
```

This means you can write your business logic boldly. Try things. If they fail, the database remains consistent. SpacetimeDB has your back.

## Everything is Programmable

SpacetimeDB doesn't limit you to declarative rules or configuration files. Your module is real code—Rust, C#, or TypeScript—running inside the database. You have the full power of a programming language at your disposal.

Need custom authorization logic? Write a function. Need to validate complex business rules? Write a function. Need to transform data before storing it? Write a function.

Even access control is programmable. While SpacetimeDB provides sensible defaults (public vs. private tables), you can implement any access pattern you can express in code.

---

## The Result

When you embrace these principles, building real-time applications becomes remarkably simple:

- **No backend servers to deploy** — your logic runs in the database
- **No caching layer to manage** — the database is already in memory
- **No sync code to write** — subscriptions handle it automatically
- **No rollback logic to maintain** — transactions handle it automatically
- **No limitations on your logic** — it's just code

This is the Zen of SpacetimeDB: a simpler way to build, so you can focus on what matters—your application.

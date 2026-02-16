# Query Builder Syntax: Inconsistencies & Standardization Plan

## Overview

This document catalogs inconsistencies across the query builder syntax in SpacetimeDB and proposes a standardization plan. The scope covers:

- **Rust server-side** (views/module query builder) — `crates/query-builder/`
- **TypeScript server-side** (views/module query builder) — `crates/bindings-typescript/src/lib/query.ts`
- **TypeScript client-side** (subscription query builder) — same file, used via codegen'd `query` export
- **TypeScript `useTable` hook** (React) — `crates/bindings-typescript/src/lib/filter.ts`
- **Proposals 0030 (Views)** and **0031 (Client Query Builder)**

---

## Inconsistencies

### 1. Client entry point: `query.tableName` vs `ctx.from.tableName`

On the TypeScript server (views), tables are accessed via `ctx.from.person`. On the TypeScript client, they're accessed via a standalone codegen'd export called `query`:

```ts
// Server-side view
ctx.from.person.where(row => row.id.eq(5)).build()

// Client-side subscription (current)
import { query } from './module_bindings';
conn.subscriptionBuilder().subscribe(query.player.build());
```

Proposal 0031 intended client-side access to also go through `ctx.from`:

```ts
conn.subscriptionBuilder()
    .addQuery(ctx => ctx.from.users.build())
    .subscribe();
```

**The `query` export should not exist.** The `tables` export (which already exists as table definitions) should carry query builder methods directly, and the subscription API should support a callback form with `ctx.from` for cross-language consistency.

### 2. `useTable` uses a completely different filter system

The `useTable` hook uses `filter.ts` — a separate, simpler system with string-based column names:

```tsx
// useTable (string-based, different API)
useTable(tableDef, where(eq('online', true)))

// Server-side view (typed, property-based)
ctx.from.myTable.where(row => row.online.eq(true))
```

`filter.ts` defines `Expr<Column>` with `eq(key, value)`, `and(...)`, `or(...)` where keys are strings. The query builder uses typed property accessors via `ColumnExpression`. These are completely separate codepaths with different capabilities (`filter.ts` only supports equality).

### 3. `.build()` is required but unnecessary

Every query must end with `.build()`:

```ts
// TypeScript
query.player.where(row => row.online.eq(true)).build()

// Rust
ctx.from.user().r#where(|c| c.online.eq(true)).build()
```

In **TypeScript**, `.build()` is a no-op cast — `FromBuilder` and `SemijoinImpl` already carry the `QueryBrand` symbol and implement `toSql()`. The `subscribe()` method already accepts `RowTypedQuery` and checks via `isRowTypedQuery()`. The intermediate builder types already satisfy the query interface.

In **Rust**, `.build()` materializes the SQL string. But this could be deferred to when `.sql()` is actually called, with builder types implementing `Into<Query<T>>`.

### 4. TypeScript query builder is missing `ne()` (not-equal)

Rust has six comparison operators: `eq`, `ne`, `gt`, `lt`, `gte`, `lte`. TypeScript's `ColumnExpression` has all of these except `ne`.

### 5. Rust query builder is missing `not()` / `NOT`

TypeScript has `not(expr)` as a standalone function. Rust's `BoolExpr` only has `And` and `Or` variants — no `Not`.

### 6. Boolean combinators use different styles

Rust uses method chaining:
```rust
c.age.gt(20).and(c.age.lt(30))
```

TypeScript uses standalone functions:
```ts
and(row.name.eq('Alice'), row.age.eq(30))
```

These should be standardized on methods for consistency:
```ts
row.name.eq('Alice').and(row.age.eq(30))
```

### 7. Standalone `from()` wrapper is redundant

TypeScript exports a `from()` function that wraps a `TableRef` in a `FromBuilder`:

```ts
from(qb.person).where(row => row.name.eq('Alice')).build()
```

But `TableRefImpl` already implements `From<TableDef>`, so you can call `.where()` directly:

```ts
qb.person.where(row => row.name.eq('Alice')).build()
```

The `from()` function is redundant and should be deprecated.

### 8. Subscription API: `addQuery` chaining vs direct `subscribe`

Proposal 0031 (Rust) uses `addQuery` chaining:
```rust
ctx.subscription_builder()
    .add_query(|ctx| ctx.from.users().build())
    .add_query(|ctx| ctx.from.players().build())
    .subscribe();
```

This is needed in Rust for per-query type inference and type-state transitions. In TypeScript, arrays are idiomatic and `addQuery` is unnecessary ceremony:

```ts
// Preferred for TypeScript — pass array to subscribe
conn.subscriptionBuilder().subscribe(ctx => [
    ctx.from.user.where(r => r.online.eq(true)),
    ctx.from.player,
]);
```

---

## Standardization Plan

### 1. Remove `.build()` requirement

Users should not need to call `.build()` at the end of every query.

**TypeScript:** The builder types already carry the `QueryBrand` and implement `toSql()`. Update the types so `From<TableDef>` and `SemijoinBuilder<TableDef>` are assignable to `Query<TableDef>`. Keep `.build()` as a deprecated no-op for backwards compatibility.

**Rust:** Implementation approach TBD (e.g. `Into<Query<T>>`, a custom trait, or macro-level changes). The `#[view]` macro should accept builder types directly, not just `Query<T>`.

**After:**
```ts
// TypeScript
conn.subscriptionBuilder().subscribe(tables.user.where(r => r.online.eq(true)));

// Rust
ctx.from.user().r#where(|c| c.online.eq(true))  // returned directly from view
```

### 2. Eliminate `query` export, use `tables` with query builder methods

The codegen'd `query` export should be removed. The existing `tables` export should carry query builder capabilities (`.where()`, `.leftSemijoin()`, `.rightSemijoin()`, etc.) directly on each table.

### 3. Subscription API: callback form with `ctx.from`

The `subscribe()` method should accept a callback that receives a query context, matching cross-language consistency with Rust and C#:

```ts
// Callback form (canonical, cross-language consistent)
conn.subscriptionBuilder().subscribe(ctx => ctx.from.user.where(r => r.online.eq(true)));

// Array form
conn.subscriptionBuilder().subscribe(ctx => [
    ctx.from.user.where(r => r.online.eq(true)),
    ctx.from.player,
]);

// Direct expression form (also accepted, convenient shorthand)
conn.subscriptionBuilder().subscribe(tables.user.where(r => r.online.eq(true)));
```

The `ctx` mirrors whatever the Rust query context carries (currently `from`, potentially identity/connection info for parameterized views in the future).

No `addQuery()` chaining in TypeScript — pass single queries or arrays directly to `subscribe()`.

### 4. Unify `useTable` with the query builder (React only)

Replace the string-based `filter.ts` system with the typed query builder:

**Before:**
```tsx
useTable(tableDef, where(eq('online', true)))
```

**After:**
```tsx
useTable(tables.user.where(row => row.online.eq(true)))
// or without a filter:
useTable(tables.user)
// with callbacks:
useTable(tables.user.where(row => row.online.eq(true)), {
    onInsert: (row) => console.log('Inserted:', row),
})
```

This deprecates `filter.ts` (`eq`, `and`, `or`, `where` from that module) in favor of the query builder's typed expressions. Client-side evaluation for `useTable` will need to work with `BooleanExpr` instead of `Expr`.

### 5. Add missing `ne()` to TypeScript

Add `ne()` to `ColumnExpression` in `query.ts`, following the exact same pattern as `eq`, `lt`, `gt`, etc.

### 6. Add missing `not()` to Rust

Add a `Not(Box<BoolExpr<T>>)` variant to `BoolExpr<T>` in `expr.rs`, a `.not()` method on `BoolExpr`, and handle it in `format_expr`.

### 7. Standardize boolean combinators on method chaining

TypeScript should support method-style `and`/`or` on boolean expressions to match Rust:

```ts
// Target (method style, consistent with Rust)
row.age.gt(20).and(row.age.lt(30))

// Still supported (standalone functions)
and(row.age.gt(20), row.age.lt(30))
```

This means `BooleanExpr` in TypeScript needs `.and()` and `.or()` methods. The standalone `and()`/`or()` functions can remain as convenience.

### 8. Deprecate standalone `from()` in TypeScript

Mark `from()` as deprecated. All docs and examples should use the table ref directly:

```ts
// Before
from(tables.person).where(row => row.name.eq('Alice'))

// After
tables.person.where(row => row.name.eq('Alice'))
```

---

## Target Syntax (All Languages)

### Rust Server (Views)

```rust
#[spacetimedb::view(accessor = online_users, public)]
fn online_users(ctx: &ViewContext) -> Query<User> {
    ctx.from.user().r#where(|c| c.online.eq(true))
}

#[spacetimedb::view(accessor = player_mods, public)]
fn player_mods(ctx: &AnonymousViewContext) -> Query<PlayerState> {
    ctx.from
        .player_state()
        .left_semijoin(ctx.from.moderator(), |p, m| p.entity_id.eq(m.entity_id))
}
```

### Rust Client (Subscriptions)

```rust
ctx.subscription_builder()
    .add_query(|ctx| ctx.from.user().r#where(|c| c.online.eq(true)))
    .add_query(|ctx| ctx.from.player())
    .subscribe();
```

### TypeScript Server (Views)

```ts
spacetime.anonymousView({ name: 'onlineUsers', public: true }, arrayRetValue, ctx => {
    return ctx.from.user.where(row => row.online.eq(true));
});
```

### TypeScript Client (Subscriptions)

```ts
// Callback form (cross-language consistent)
conn.subscriptionBuilder().subscribe(ctx => [
    ctx.from.user.where(r => r.online.eq(true)),
    ctx.from.player,
]);

// Direct form (convenient shorthand)
conn.subscriptionBuilder().subscribe(tables.user.where(r => r.online.eq(true)));
```

### TypeScript React (`useTable`)

```tsx
const [users, isReady] = useTable(tables.user.where(row => row.online.eq(true)));
const [allPlayers, isReady] = useTable(tables.player);
const [users, isReady] = useTable(tables.user.where(row => row.online.eq(true)), {
    onInsert: (row) => console.log('New user:', row),
});
```

---

## File Reference

| Component | Path |
|-----------|------|
| Rust query builder core | `crates/query-builder/src/{lib,table,join,expr}.rs` |
| Rust `#[table]` macro codegen | `crates/bindings-macro/src/table.rs` |
| Rust client SDK codegen | `crates/codegen/src/rust.rs` |
| Rust view context | `crates/bindings/src/lib.rs` |
| Rust views smoketest | `crates/smoketests/modules/views-query/src/lib.rs` |
| TS query builder | `crates/bindings-typescript/src/lib/query.ts` |
| TS filter (to deprecate) | `crates/bindings-typescript/src/lib/filter.ts` |
| TS React `useTable` | `crates/bindings-typescript/src/react/useTable.ts` |
| TS subscription builder | `crates/bindings-typescript/src/sdk/subscription_builder_impl.ts` |
| TS codegen | `crates/codegen/src/typescript.rs` |
| TS view type tests | `crates/bindings-typescript/src/server/view.test-d.ts` |
| TS query builder tests | `crates/bindings-typescript/tests/query.test.ts` |
| TS client query tests | `crates/bindings-typescript/tests/client_query.test.ts` |

---
title: TypeScript Reference
slug: /modules/typescript
toc_max_heading_level: 6
---

# SpacetimeDB TypeScript Module Library

[SpacetimeDB](https://spacetimedb.com/) lets you write server-side applications (called **modules**) that run inside a relational database. Modules define **tables** (your data) and **reducers** (your server endpoints). Clients connect directly to the database to read public data via SQL subscriptions and queries, and they invoke reducers to mutate state.

```text
    Client Application                          SpacetimeDB
┌───────────────────────┐                ┌───────────────────────┐
│                       │                │                       │
│  ┌─────────────────┐  │    SQL Query   │  ┌─────────────────┐  │
│  │ Subscribed Data │<─────────────────────│    Database     │  │
│  └─────────────────┘  │                │  └─────────────────┘  │
│           │           │                │           ^           │
│           │           │                │           │           │
│           v           │                │           v           │
│  +─────────────────┐  │ call_reducer() │  ┌─────────────────┐  │
│  │   Client Code   │─────────────────────>│   Module Code   │  │
│  └─────────────────┘  │                │  └─────────────────┘  │
│                       │                │                       │
└───────────────────────┘                └───────────────────────┘
```

TypeScript modules are built with the TypeScript Module Library from [`spacetimedb/server`](https://www.npmjs.com/package/spacetimedb). You define your schema and reducers in TypeScript, and then build and deploy with the [`spacetime` CLI](https://spacetimedb.com/install) using the `spacetime publish` command. Under the hood, SpacetimeDB uses [Rolldown](https://rolldown.rs/) to bundle your application into a single JavaScript artifact before uploading it to the SpacetimeDB host.

:::note
SpacetimeDB also provides a TypeScript **client** SDK at `spacetimedb/sdk`, as well as integrations for frameworks like `spacetimedb/react`. This guide focuses exclusively on the **server-side module** library.
:::

If you’re new to TypeScript, see the [TypeScript Handbook](https://www.typescriptlang.org/docs/handbook/intro.html). For a guided introduction to modules, see the [TypeScript Module Quickstart](/modules/typescript/quickstart).

## Overview

SpacetimeDB modules interact with the outside world via two mechanisms: **tables** and **reducers**.

- [Tables](#tables) store data; public tables are queryable and subscribable by clients.
- [Reducers](#reducers) are functions that can read and write tables and are callable over the network.

Reducers are atomic and deterministic, there’s no direct filesystem or network access (e.g., `fs`, `fetch`). They execute inside the database with ACID guarantees.

A minimal module looks like this:

```ts
import { schema, table, t, type RowObj } from 'spacetimedb/server';

// Define a table that is publicly readable by clients
const players = table(
  { name: 'players', public: true },
  {
    id: t.u32().primaryKey().autoInc(),
    name: t.string(),
  }
);

// Compose a schema from one or more tables
const spacetimedb = schema(players);

// Define a reducer that inserts a row
spacetimedb.reducer('add_player', { name: t.string() }, (ctx, { name }) => {
  ctx.db.players.insert({ id: 0, name });
});
```

Reducers don’t return data directly; instead, clients read from tables or subscribe for live updates.

Tables and reducers can use any types built with `t.*` (e.g., `t.string()`, `t.i32()`) or composite types defined with `t.object`, `t.enum`, `t.array`, or `t.option`.

## Setup

1. **[Install the CLI](https://spacetimedb.com/install)**

2. **Initialize a TypeScript module project**

   ```bash
   spacetime init --lang typescript my-project
   cd my-project
   ```

   This creates a scaffold with a sample module entrypoint and `package.json`.

3. **Develop**
   - Add tables with `table(...)` and reducers with `spacetimedb.reducer(...)` in your source.

4. **Build and publish**

   ```bash
   spacetime login
   spacetime publish <MY_DATABASE_NAME>
   # Example: spacetime publish silly_demo_app
   ```

Publishing bundles your code into a JavaScript bundle, and creates a database and installs your bundle in that database. The CLI outputs the database’s **name** and **Identity** (a hex string). Save this identity for administration tasks like `spacetime logs <DATABASE_NAME>`.

:::warning
IMPORTANT! In order to build and publish your module, you must have a `src/index.ts` file in your project. If you have multiple files that define reducers, you must import them from that file. e.g.

```ts
import './schema';
import './my_reducers';
import './my_other_reducers';
```

This ensures that those files are included in the bundle.
:::

Re-publishing updates your module in place with [automatic migrations](#automatic-migrations) where possible:

```bash
spacetime publish <MY_DATABASE_NAME>
```

where `<MY_DATABASE_NAME>` is the name of your existing database.

You can also generate client bindings for your schema with `spacetime generate`. See the [client SDK documentation](https://spacetimedb.com/docs/sdks/typescript#generate-module-bindings) for more information.

# How it works

SpacetimeDB transpiles and bundles your code into a JavaScript bundle that conform to its host ABI (application binary interface). The **host** loads your module, applies schema migrations, initializes lifecycle reducers, and serves client calls. During module updates, active connections and subscriptions remain intact, allowing you to hotswap your server code without affecting or disconnecting any clients.

## Publishing Flow

When you run `spacetime publish <DATABASE_NAME>`, the following happens:

- The host locates or creates the target database.
- The new schema is compared against the current version; if compatible, an [automatic migration](#automatic-migrations) runs.
- The host atomically swaps in the new module, invoking lifecycle reducers such as `Init`.
- The module becomes live, serving new reducer calls.

## Tables

All data in SpacetimeDB is stored in the form of **tables**. SpacetimeDB tables are hosted in memory, in the same process as your code, for extremely low latency and high throughput access to your data. SpacetimeDB also automatically persists all data in tables to disk behind the scenes.

In TypeScript you can declare a new table with the `table` function.

```ts
import { table, t } from 'spacetimedb/server';

const people = table(
  { name: 'people', public: true },
  {
    id: t.u32().primaryKey().autoInc(),
    name: t.string().index('btree'),
    email: t.string().unique(),
  }
);
```

The first argument to the `table` function is where you can define options for the table, and the second argument is an object which defines the type of each column in the table.

You can set the following options on tables:

| **Property** | **Type**                 | **Description**                                                                   | **Default** |
| ------------ | ------------------------ | --------------------------------------------------------------------------------- | ----------- |
| `name`       | `string`                 | The name of the table.                                                            | -           |
| `public`     | `boolean`                | Whether the table is publicly accessible.                                         | `false`     |
| `indexes`    | `IndexOpts<keyof Row>[]` | Declarative multi-column indexes for the table.                                   | -           |
| `scheduled`  | `string`                 | The name of the reducer to be executed based on the scheduled rows in this table. | -           |

:::note
All tables are **private** by default, meaning that they are visible only to the module owner. You can explicitly make them public to all users by setting `public: true` in the options.
:::

### `IndexOpts<AllowedCol>`

Defines configuration for a table index.  
Each index must specify an algorithm and one or more columns.

| **Property** | **Type**                | **Description**                                                   |
| ------------ | ----------------------- | ----------------------------------------------------------------- |
| `name`       | `string` _(optional)_   | A custom name for the index.                                      |
| `algorithm`  | `'btree'` \| `'direct'` | The indexing algorithm used.                                      |
| `columns`    | `readonly AllowedCol[]` | _(Required for `btree`)_ Columns included in the B-Tree index.    |
| `column`     | `AllowedCol`            | _(Required for `direct`)_ Column used for direct lookup indexing. |

Each table generates a database accessor at `ctx.db.<table_name>` with methods like:

| Operation    | Example                                           |
| ------------ | ------------------------------------------------- |
| Insert row   | `ctx.db.people.insert({ id: 0, name, email })`    |
| Delete row   | `ctx.db.people.delete({ id, name, email })`       |
| Iterate rows | `for (const row of ctx.db.people.iter()) { ... }` |
| Count rows   | `ctx.db.people.count`                             |

:::tip
**Performance:** Prefer using indexes or unique accessors for targeted lookups instead of full iterations.
:::

### Public and Private Tables

- **Private tables**: Visible only to reducers and the database owner (e.g., via CLI debugging). Clients cannot access them.
- **Public tables**: Exposed for client read access. Writes still occur only through reducers.

# Types

Types for tables are constructed with SpacetimeDB's `TypeBuilder` API which is exported as `t` from `spacetimedb/server`. This type is very similar to other type validation libraries like [Zod](https://github.com/colinhacks/zod). These types tell SpacetimeDB what the schema of your database should be. They also allow you to provide very specific datatypes like unsigned 8-bit integers for maximum performance.

```ts
import { t } from 'spacetimedb/server';
```

`t` provides a collection of factory functions for creating SpacetimeDB algebraic types used in table definitions. Each function returns a corresponding _builder_ (e.g., `BoolBuilder`, `StringBuilder`, `F64Builder`) that implements `TypeBuilder`, enabling type-safe schema construction.

- Primitive types map to native TypeScript: `bool` → `boolean`, `string` → `string`, `number`/`f32`/`f64` → `number`, and large integers to `bigint`.
- Complex types (`object`, `row`, `array`, `enum`) support nested/structured schemas.
- The `scheduleAt` function creates a special column type used for scheduling reducers.

### Primitives

| Factory      | Returns         | TS Representation | Description                     |
| ------------ | --------------- | ----------------- | ------------------------------- |
| `t.bool()`   | `BoolBuilder`   | `boolean`         | Boolean column type.            |
| `t.string()` | `StringBuilder` | `string`          | UTF-8 string type.              |
| `t.number()` | `F64Builder`    | `number`          | Alias for `f64` (64-bit float). |
| `t.f32()`    | `F32Builder`    | `number`          | 32-bit float.                   |
| `t.f64()`    | `F64Builder`    | `number`          | 64-bit float.                   |
| `t.i8()`     | `I8Builder`     | `number`          | Signed 8-bit integer.           |
| `t.u8()`     | `U8Builder`     | `number`          | Unsigned 8-bit integer.         |
| `t.i16()`    | `I16Builder`    | `number`          | Signed 16-bit integer.          |
| `t.u16()`    | `U16Builder`    | `number`          | Unsigned 16-bit integer.        |
| `t.i32()`    | `I32Builder`    | `number`          | Signed 32-bit integer.          |
| `t.u32()`    | `U32Builder`    | `number`          | Unsigned 32-bit integer.        |
| `t.i64()`    | `I64Builder`    | `bigint`          | Signed 64-bit integer.          |
| `t.u64()`    | `U64Builder`    | `bigint`          | Unsigned 64-bit integer.        |
| `t.i128()`   | `I128Builder`   | `bigint`          | Signed 128-bit integer.         |
| `t.u128()`   | `U128Builder`   | `bigint`          | Unsigned 128-bit integer.       |
| `t.i256()`   | `I256Builder`   | `bigint`          | Signed 256-bit integer.         |
| `t.u256()`   | `U256Builder`   | `bigint`          | Unsigned 256-bit integer.       |

### Structured Types

| Factory                  | Returns                               | TypeScript Representation                                                     | Description / Usage                                                                                            |
| ------------------------ | ------------------------------------- | ----------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------- |
| `t.object(name, obj)`    | `ProductBuilder<Obj>`                 | `{ [K in keyof Obj]: T<Obj[K]> }`                                             | Product/object type (fields are `TypeBuilder`s). Used for nested or structured data types.                     |
| `t.row(obj)`             | `RowBuilder<Obj>`                     | `{ [K in keyof Obj]: T<Obj[K]> }`                                             | Row type for table schemas. Same TS shape as `object`, but allows keys which can have column metadata on them. |
| `t.enum(name, variants)` | `SumBuilder<Obj> \| SimpleSumBuilder` | Union of tagged objects: `{ tag: 'variant' } \| { tag: 'variant', value: T }` | Sum/enum type. If all variants are empty (unit), yields a simple string-like enum; otherwise a tagged union.   |
| `t.array(element)`       | `ArrayBuilder<Element>`               | `T<Element>[]`                                                                | Array of the given element type.                                                                               |
| `t.unit()`               | `UnitBuilder`                         | `{}` (in some cases `undefined`, as in the case of the simplified enum above) | Zero-field product type (unit). Used for empty payloads or tag-only enum variants.                             |

:::note
`t.object` and `t.enum` require a `name` parameter which defines the name of this type in SpacetimeDB. This parameter is not strictly required by TypeScript but it allows SpacetimeDB to code generate those types in other languages that require names for types.
:::

### Special / Scheduling

| Factory            | Returns                        | TypeScript Representation | Description                                                                                                                          |
| ------------------ | ------------------------------ | ------------------------- | ------------------------------------------------------------------------------------------------------------------------------------ |
| `t.scheduleAt()`   | `ColumnBuilder<ScheduleAt, …>` | `ScheduleAt`              | Special column type for scheduling reducer execution. Automatically sets `isScheduleAt: true` in metadata.                           |
| `t.option(value)`  | `OptionBuilder<Value>`         | `Value \| undefined`      | Optional value type (equivalent to an enum with `some` / `none`). In TypeScript, represented as the inner value type or `undefined`. |
| `t.identity()`     | `IdentityBuilder`              | `Identity`                | Unique identity type. Used for identifying entities within SpacetimeDB.                                                              |
| `t.connectionId()` | `ConnectionIdBuilder`          | `ConnectionId`            | Represents a client connection identifier.                                                                                           |
| `t.timestamp()`    | `TimestampBuilder`             | `Timestamp`               | Represents an absolute point in time (microseconds since Unix epoch).                                                                |
| `t.timeDuration()` | `TimeDurationBuilder`          | `TimeDuration`            | Represents a relative duration in microseconds.                                                                                      |

Use `t` to define advanced types for rows or arguments:

```ts
const simpleEnum = t.enum('SimpleEnum', {
  Zero: t.unit(),
  One: t.unit(),
  Two: t.unit(),
});

const everyPrimitive = t.object('EveryPrimitiveStruct', {
  a: t.u8(),
  b: t.u16(),
  c: t.u32(),
  d: t.u64(),
  e: t.u128(),
  f: t.u256(),
  g: t.i8(),
  h: t.i16(),
  i: t.i32(),
  j: t.i64(),
  k: t.i128(),
  l: t.i256(),
  m: t.bool(),
  n: t.f32(),
  o: t.f64(),
  p: t.string(),
  q: t.identity(),
  r: t.connectionId(),
  s: t.timestamp(),
  t: t.timeDuration(),
});

const container = t.object('Container', {
  maybe: t.option(t.i32()),
  list: t.array(t.string()),
  enums: t.array(simpleEnum),
});
```

Row types are reusable:

```ts
const a = table({ name: 'a' }, { n: t.u8() });
const b = table({ name: 'b' }, { a: a.rowType, text: t.string() });
```

### Column Attributes

You can convert a plain type into a column by adding one or more column attributes to that type. This will convert the `TypeBuilder` into a `ColumnBuilder` which stores metadata about the column attributes. `ColumnBuilder` types must be either passed to the `table` function directly, or as a field of a type constructed with `t.row()`:

```ts
import { t } from 'spacetimedb/server';

const peopleRowType = t.row({
  id: t.u32().primaryKey().autoInc(),
  name: t.string().index('btree'),
  email: t.string().unique(),
});

const people = table({ name: 'people', public: true }, peopleRowType);
```

### Unique and Primary Key Columns

Columns can be marked `.unique()` or `.primaryKey()`. Only one primary key is allowed, but multiple unique columns are supported. The primary key column represents the identity of the row. Changes to a row that don't affect the primary key are considered to be updates, while changes to the primary key are considered to be a new row (i.e. a delete followed by an insert).

The unique and primary key column attributes guarantee that only a single row can exist with a given value for the column and generate accessors at `ctx.db.<table>.<column>`:

- `find(key)` - returns a row or `null`
- `update(row)` - replaces the existing row with the same primary key and returns the updated row
- `delete(key)` - removes the row, returns a boolean

Example:

```ts
const users = table(
  { name: 'users', public: true },
  {
    id: t.u32().primaryKey(),
    username: t.string().unique(),
    dogCount: t.u64(),
  }
);

const spacetimedb = schema(users);

spacetimedb.reducer('give_dogs', { id: t.u32(), n: t.u32() }, (ctx, { id, n }) => {
    const row = ctx.db.users.id.find(id);
    if (!row) {
      throw new SenderError('User not found');
    }
    row.dogCount += n;
    ctx.db.users.id.update(row);
});

spacetimedb.reducer('ban_username', { username: t.string() }, (ctx, { username }) => {
    ctx.db.users.username.delete(username);
});
```

:::note
Updates require a unique or primary key column. The base table view has no direct `update` method.
:::

### Auto-increment Columns

Use `.autoInc()` for automatically increasing integer identifiers. Inserting a row with a zero-valued field causes the database to assign a new unique value.

```ts
const posts = table(
  { name: 'posts', public: true },
  {
    id: t.u64().primaryKey().autoInc(),
    title: t.string(),
  }
);

const spacetimedb = schema(posts);

spacetimedb.reducer('add_post', { title: t.string() }, (ctx, { title }) => {
  const inserted = ctx.db.posts.insert({ id: 0, title });
  // inserted.id now contains the assigned auto-incremented value
});
```

## Indexes

You can define indexes either directly on a column or on a table for efficient data access and filtering:

- Single-column: `.index('btree')` on a column.
- Multi-column: use `indexes` in the table options.

```ts
const scores = table(
  {
    name: 'scores',
    public: true,
    indexes: [
      {
        name: 'byPlayerAndLevel',
        algorithm: 'btree',
        columns: ['player_id', 'level'],
      },
    ],
  },
  {
    player_id: t.u32(),
    level: t.u32(),
    points: t.i64(),
  }
);
```

Access indexes at `ctx.db.<table>.<index>` with:

- `filter(bound)` - iterate rows by prefix or range
- `delete(bound)` - remove rows matching the bound

Example:

```ts
for (const row of ctx.db.scores.byPlayerAndLevel.filter(123)) {
  // rows with player_id = 123
}
for (const row of ctx.db.scores.byPlayerAndLevel.filter([123, [1, 10]])) {
  // player_id = 123, 1 <= level <= 10
}
```

Indexable key types include integers, booleans, strings, `identity`, `connectionId`, and simple enums defined with `t.enum`.

## Reducers

Reducers are declared with `spacetimedb.reducer(name, argTypes, handler)`, where `spacetimedb` is the value returned from the `schema` function.

:::note
By convention in our examples we use the name `spacetimedb` for this value, but you can call it whatever you like. `s` is a shorter alternative if you prefer. This value provides access to the database and also context for the TypeScript type system to ensure your
:::

- The handler signature is `(ctx, args)`.
- Arguments are validated against the types defined in the `argTypes`.
- Reducers modify tables and do not return any values.

```ts
spacetimedb.reducer('give_item', { player_id: t.u64(), item_id: t.u64() }, (ctx, { player_id, item_id }) => {
    // modify tables
});
```

### Reducer Errors

Reducers execute in an "atomic" transactional context, meaning either all of the changes from the function are applied or none of them. If your reducer encounters an error during execution, all of the changes you've applied during that call will be rolled back as if the reducer had never been called at all.

In SpacetimeDB there are two classes of errors that you reducer might encounter:

1. Sender errors, which are caused by the caller of the reducer (called the "sender")
2. Programmer errors, which are errors caused by incorrect logic in your module code.

#### Sender Errors

There are two ways you can return a sender error from a reducer:

1. By throwing a `SenderError` via `throw new SenderError("message")` where `message` is the error string
2. By returning a value of type `{ tag: 'err', value: string }` where `value` is the error string

For example:

```ts
spacetimedb.reducer('give_item', { player_id: t.u64(), item_id: t.u64() }, (ctx, { player_id, item_id }) => {
    if (!ctx.db.owner.id.find(ctx.sender)) {
      throw new SenderError('Reducer may only be invoked by module owner');
    }
    // ...
});
// or
spacetimedb.reducer('give_item', { player_id: t.u64(), item_id: t.u64() }, (ctx, { player_id, item_id }) => {
    if (!ctx.db.owner.id.find(ctx.sender)) {
      return {
        tag: 'err',
        value: 'Reducer may only be invoked by module owner',
      };
    }
    // ...
});
```

#### Programmer Errors

SpacetimeDB considers all uncaught errors thrown by your reducer which are not of the type `SenderError` to be programmer errors or "panics". These errors will be shown to you in your project's dashboard, or you can configure alerting so that you find out when these errors occur.

Just as with `SenderError` if an error is uncaught, all changes made during the transaction will be rolled back.

### ReducerContext

Within a reducer, the context (`ctx`) provides:

- `ctx.db` - access to tables and indexes
- `ctx.sender` - caller `Identity`
- `ctx.connectionId` - caller connection ID, or `undefined`
- `ctx.timestamp` - invocation `Timestamp`

Examples:

```ts
spacetimedb.reducer('insert_caller_identity', ctx => {
  ctx.db.users.insert({ identity: ctx.sender, name: 'me' });
});

spacetimedb.reducer('record_call_time', ctx => {
  ctx.db.calls.insert({ t: ctx.timestamp });
});
```

# Scheduled Reducers

Define recurring or delayed operations with **scheduled tables** containing a `scheduleAt` column.

```ts
const ScheduledJobs = table(
  { name: 'scheduled_jobs', scheduled: 'send_message', public: true },
  {
    scheduled_id: t.u64().primaryKey().autoInc(),
    scheduled_at: t.scheduleAt(),
    text: t.string(),
  }
);

const spacetimedb = schema(ScheduledJobs);

spacetimedb.reducer('send_message', { arg: ScheduledJobs.rowType }, (_ctx, { arg }) => {
    // Called automatically by scheduler with job row data
});
```

Insert rows to schedule jobs; delete to cancel. Scheduling is transactional-failed reducers prevent scheduling persistence.

**Restricting manual calls:**

```ts
spacetimedb.reducer('send_message', { arg: ScheduledJobs.rowType }, (ctx, { arg }) => {
    if (!ctx.db.owner.id.find(ctx.sender)) {
      throw new SenderError(
        'Reducer may only be invoked by the database owner'
      );
    }
    // ...
});
```

## Automatic Migrations

Re-publishing attempts schema migrations automatically. Safe operations:

- ✅ Add tables or indexes
- ✅ Toggle auto-increment
- ✅ Make private tables public

Potentially breaking:

- ⚠️ Modify or remove reducers
- ⚠️ Make public tables private
- ⚠️ Remove primary keys or indexes used in client queries
- ⚠️ Add columns (these can break old clients)

Forbidden without manual migration:

- ❌ Remove tables
- ❌ Change column definitions or order
- ❌ Alter scheduling status
- ❌ Add new constraints that invalidate existing data

:::warning
The following deletes all data stored in the database.
:::

To fully reset your database and clear all data, run:

```bash
spacetime publish --clear-database <DATABASE_NAME>
# or
spacetime publish -c <DATABASE_NAME>
```

## Logging & Diagnostics

SpacetimeDB provides a lightweight, high-performance logging system modeled after the standard JavaScript `console` API. You can use familiar logging calls like `console.log()`, `console.error()`, or `console.debug()`, and they will automatically be routed through SpacetimeDB’s internal `sys.console_log` system.

Logs are visible only to the database owner and can be viewed via the CLI:

```bash
spacetime logs <DATABASE_NAME>
```

Client applications cannot access logs, they are private to your database instance.

### Console API

SpacetimeDB implements a `console` object compatible with the standard `Console` interface, but adapted for a WASM/SpacetimeDB environment. Use the following methods exactly as you would in the browser or Node.js:

```ts
console.log('Hello SpacetimeDB!');
console.info('Connected to database');
console.warn('Cache is nearly full');
console.error('Failed to fetch entity');
console.debug('Reducer input:', data);
console.trace('Reducer execution trace');
```

### Assertions

`console.assert(condition, ...data)` logs an error if the condition is falsy:

```ts
console.assert(userId !== undefined, 'Missing user ID!');
```

If the assertion fails, the message is logged at **error level**.

### Tables and Object Logging

`console.table()` logs structured or tabular data for inspection.  
Properties are ignored, only the `tabularData` object is formatted as a string.

```ts
console.table({ x: 10, y: 20 });
```

### Timers

SpacetimeDB’s console supports named timers via `console.time()`, `console.timeLog()`, and `console.timeEnd()`.

| Method                            | Description                                                             |
| --------------------------------- | ----------------------------------------------------------------------- |
| `console.time(label)`             | Starts a new timer. Warns if a timer with the same label exists.        |
| `console.timeLog(label, ...data)` | Logs intermediate timing info (does **not** stop the timer).            |
| `console.timeEnd(label)`          | Ends a timer and logs the total elapsed time. Warns if no timer exists. |

Example:

```ts
console.time('load');
loadWorldData();
console.timeLog('load', 'Halfway through loading');
finalizeLoad();
console.timeEnd('load'); // Logs elapsed time
```

### Additional Console Methods

The following methods are present for API completeness but are currently **no-ops**:

- `console.clear()`
- `console.dir()`
- `console.dirxml()`
- `console.count()`
- `console.countReset()`
- `console.group()`
- `console.groupCollapsed()`
- `console.groupEnd()`
- `console.timeStamp()`
- `console.profile()`
- `console.profileEnd()`

## Cheatsheet

This section summarizes the most common patterns for declaring tables, reducers, and indexes in TypeScript modules.
Each example assumes:

```ts
import { schema, table, t } from 'spacetimedb/server';
```

---

### Tables

```ts
const products = table(
  { name: 'products', public: true },
  {
    id: t.u32().primaryKey().autoInc(),
    sku: t.string().unique(),
    name: t.string().index('btree'),
  }
);
```

- `.primaryKey()` defines a primary key column (only one per table).
- `.autoInc()` assigns increasing integer IDs automatically when you insert with zero.
- `.unique()` defines a unique constraint (non-primary).
- `.index('btree')` adds a searchable index to speed up lookups and range filters.

---

### Reducers

```ts
const spacetimedb = schema(products);

// Insert a new product
spacetimedb.reducer('insert_product', products.rowType, (ctx, product) => {
  ctx.db.products.insert(product);
});

// Update by SKU (unique key)
spacetimedb.reducer('update_product_by_sku', products.rowType, (ctx, product) => {
    ctx.db.products.sku.update(product);
});

// Delete by SKU
spacetimedb.reducer('delete_product_by_sku', { sku: t.string() }, (ctx, { sku }) => {
    ctx.db.products.sku.delete(sku);
});
```

Reducers mutate tables via `ctx.db.<table>`.
Reducers are transactional and automatically roll back if they throw an exception.

---

### Indexes

```ts
for (const row of ctx.db.products.name.filter(['A', ['M', 'Z']])) {
  // All products whose names start with a letter between "A" and "Z"
}

const deletedCount = ctx.db.products.name.delete(['G']);
```

Indexes may be filtered by a prefix or a bounded range.
They are generated automatically from `.index('btree')` annotations or declared explicitly in table options.

---

### Scheduled Reducers

```ts
const Reminders = table(
  { name: 'reminders', scheduled: 'send_reminder' },
  {
    scheduled_id: t.u64().primaryKey().autoInc(),
    scheduled_at: t.scheduleAt(),
    message: t.string(),
  }
);

spacetimedb.reducer('send_reminder', { arg: Reminders.rowType }, (_ctx, { arg }) => {
    // Invoked automatically by the scheduler
    // arg.message, arg.scheduled_at, arg.scheduled_id
});
```

Insert rows into a scheduled table to queue work; delete them to cancel.
Reducers may guard against manual invocation by checking `ctx.sender`.

---

### Common Context Properties

| Property           | Description                                                |
| ------------------ | ---------------------------------------------------------- |
| `ctx.db`           | Handle to all tables and indexes in the current database.  |
| `ctx.sender`       | The `Identity` of the reducer caller.                      |
| `ctx.connectionId` | The `ConnectionId` of the reducer caller (or `undefined`). |
| `ctx.timestamp`    | A `Timestamp` for when the reducer was invoked.            |

---

This cheatsheet provides concise operational examples.
For detailed behavior and lifecycle semantics, see the sections on [Tables](#tables), [Reducers](#reducers), and [Indexes](#indexes) above.

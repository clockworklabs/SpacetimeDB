---
name: spacetimedb-typescript-core
description: Core SpacetimeDB TypeScript server and client SDK syntax for building an application without framework or architecture guidance.
license: Apache-2.0
metadata:
  author: clockworklabs
  version: "1.0"
  language: typescript
---

# SpacetimeDB TypeScript Core API

## Server module

Define tables with `table()`, bind them with `schema()`, and export the schema
as the module default. Export reducers from the same module or its entry file.

```typescript
import { schema, table, t } from 'spacetimedb/server';

const record = table(
  { name: 'record', public: true },
  {
    id: t.u64().primaryKey().autoInc(),
    label: t.string().index('btree'),
    value: t.u32(),
  },
);

const spacetimedb = schema({ record });
export default spacetimedb;

export const createRecord = spacetimedb.reducer(
  { label: t.string(), value: t.u32() },
  (ctx, { label, value }) => {
    ctx.db.record.insert({ id: 0n, label, value });
  },
);
```

Table names must be snake_case. The keys passed to `schema({ ... })` are the
server-side `ctx.db` accessor names. A split module must re-export the schema
as the default export from its entry file.

## Types and table access

Common builders are `t.string()`, `t.bool()`, `t.u32()`, `t.i32()`,
`t.u64()`, `t.i64()`, `t.identity()`, `t.timestamp()`, and
`t.option(inner)`. The 64-bit integer builders use TypeScript `bigint` values.
Use `0n` for an auto-increment `u64` or `i64` field during insertion.

Column modifiers include `.primaryKey()`, `.autoInc()`, `.unique()`, and
`.index('btree')`.

```typescript
const row = ctx.db.record.id.find(id);                  // row | null
const inserted = ctx.db.record.insert(values);          // inserted row
if (row) ctx.db.record.id.update({ ...row, value: 2 }); // update by primary key
ctx.db.record.id.delete(id);                            // delete by primary key
const matching = [...ctx.db.record.label.filter(label)];
const all = [...ctx.db.record.iter()];
```

`iter()` and `filter()` return iterators. Spread them before using array
methods. Insert through the table accessor, not through an index accessor.

## Generated client bindings

Generated bindings convert snake_case table, reducer, and field names to
camelCase. A server reducer named `createRecord` is called as `createRecord` in a
TypeScript client.

Create a connection with the generated `DbConnection`:

```typescript
import { DbConnection, tables } from './module_bindings';

const connection = DbConnection.builder()
  .withUri(serverUri)
  .withDatabaseName(moduleName)
  .onConnect(ctx => {
    ctx.subscriptionBuilder()
      .onApplied(() => console.log('ready'))
      .subscribe([tables.record]);
  })
  .build();
```

Call reducers with an object argument:

```typescript
await connection.reducers.createRecord({ label: 'Example', value: 1 });
```

The generated database accessors support row callbacks:

```typescript
connection.db.record.onInsert((_ctx, row) => console.log(row.label));
connection.db.record.onUpdate((_ctx, oldRow, newRow) => console.log(oldRow, newRow));
connection.db.record.onDelete((_ctx, row) => console.log(row.id));
```

---
title: Column Types and Constraints
slug: /tables/columns
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';


Columns define the structure of your tables. Each column has a type and can have constraints that enforce data integrity.

## Column Types

SpacetimeDB supports a variety of column types optimized for performance.

### Primitive Types

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

| Type | Returns | TypeScript Type | Description |
|------|---------|----------------|-------------|
| `t.bool()` | `BoolBuilder` | `boolean` | Boolean value |
| `t.string()` | `StringBuilder` | `string` | UTF-8 string |
| `t.f32()` | `F32Builder` | `number` | 32-bit floating point |
| `t.f64()` | `F64Builder` | `number` | 64-bit floating point |
| `t.i8()` | `I8Builder` | `number` | Signed 8-bit integer |
| `t.u8()` | `U8Builder` | `number` | Unsigned 8-bit integer |
| `t.i16()` | `I16Builder` | `number` | Signed 16-bit integer |
| `t.u16()` | `U16Builder` | `number` | Unsigned 16-bit integer |
| `t.i32()` | `I32Builder` | `number` | Signed 32-bit integer |
| `t.u32()` | `U32Builder` | `number` | Unsigned 32-bit integer |
| `t.i64()` | `I64Builder` | `bigint` | Signed 64-bit integer |
| `t.u64()` | `U64Builder` | `bigint` | Unsigned 64-bit integer |
| `t.i128()` | `I128Builder` | `bigint` | Signed 128-bit integer |
| `t.u128()` | `U128Builder` | `bigint` | Unsigned 128-bit integer |
| `t.i256()` | `I256Builder` | `bigint` | Signed 256-bit integer |
| `t.u256()` | `U256Builder` | `bigint` | Unsigned 256-bit integer |

</TabItem>
<TabItem value="csharp" label="C#">

| Type | Description |
|------|-------------|
| `bool` | Boolean value |
| `string` | UTF-8 string |
| `float`, `double` | Floating point numbers |
| `sbyte`, `short`, `int`, `long` | Signed integers (8-bit to 64-bit) |
| `byte`, `ushort`, `uint`, `ulong` | Unsigned integers (8-bit to 64-bit) |
| `SpacetimeDB.I128`, `SpacetimeDB.I256` | Signed 128-bit and 256-bit integers |
| `SpacetimeDB.U128`, `SpacetimeDB.U256` | Unsigned 128-bit and 256-bit integers |

</TabItem>
<TabItem value="rust" label="Rust">

| Type | Description |
|------|-------------|
| `bool` | Boolean value |
| `String` | UTF-8 string |
| `f32`, `f64` | Floating point numbers |
| `i8` through `i128` | Signed integers |
| `u8` through `u128` | Unsigned integers |

</TabItem>
</Tabs>

### Special Types

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

**Structured Types**

| Type | Returns | TypeScript Type | Description |
|------|---------|----------------|-------------|
| `t.object(name, obj)` | `ProductBuilder<Obj>` | `{ [K in keyof Obj]: T<Obj[K]> }` | Product/object type for nested or structured data |
| `t.row(obj)` | `RowBuilder<Obj>` | `{ [K in keyof Obj]: T<Obj[K]> }` | Row type for table schemas (allows column metadata) |
| `t.enum(name, variants)` | `SumBuilder<Obj>` or `SimpleSumBuilder` | `{ tag: 'variant' } \| { tag: 'variant', value: T }` | Sum/enum type (tagged union or simple enum) |
| `t.array(element)` | `ArrayBuilder<Element>` | `T<Element>[]` | Array of the given element type |
| `t.unit()` | `UnitBuilder` | `{}` or `undefined` | Zero-field product type (unit) |
| `t.option(value)` | `OptionBuilder<Value>` | `Value \| undefined` | Optional value type |

**Special Types**

| Type | Returns | TypeScript Type | Description |
|------|---------|----------------|-------------|
| `t.identity()` | `IdentityBuilder` | `Identity` | Unique identity for authentication |
| `t.connectionId()` | `ConnectionIdBuilder` | `ConnectionId` | Client connection identifier |
| `t.timestamp()` | `TimestampBuilder` | `Timestamp` | Absolute point in time (microseconds since Unix epoch) |
| `t.timeDuration()` | `TimeDurationBuilder` | `TimeDuration` | Relative duration in microseconds |
| `t.scheduleAt()` | `ColumnBuilder<ScheduleAt, …>` | `ScheduleAt` | Special column type for scheduling reducer execution |

</TabItem>
<TabItem value="csharp" label="C#">

**Structured Types**

| Type | Description |
|------|-------------|
| `TaggedEnum<Variants>` | Tagged union/enum type for sum types |
| `T?` | Nullable/optional value |
| `List<T>` | List of elements |

**Special Types**

| Type | Description |
|------|-------------|
| `Identity` | Unique identity for authentication |
| `ConnectionId` | Client connection identifier |
| `Timestamp` | Absolute point in time (microseconds since Unix epoch) |
| `TimeDuration` | Relative duration in microseconds |
| `ScheduleAt` | When a scheduled reducer should execute (either at a specific time or at repeating intervals) |

</TabItem>
<TabItem value="rust" label="Rust">

**Structured Types**

| Type | Description |
|------|-------------|
| `enum` with `#[derive(SpacetimeType)]` | Sum type/tagged union |
| `Option<T>` | Optional value |
| `Vec<T>` | Vector of elements |

**Special Types**

| Type | Description |
|------|-------------|
| `Identity` | Unique identity for authentication |
| `ConnectionId` | Client connection identifier |
| `Timestamp` | Absolute point in time (microseconds since Unix epoch) |
| `Duration` | Relative duration |
| `ScheduleAt` | When a scheduled reducer should execute (either `Time(Timestamp)` or `Interval(Duration)`) |

</TabItem>
</Tabs>

## Column Constraints

### Unique Columns

Mark columns as unique to ensure only one row can exist with a given value.

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
const user = table(
  { name: 'user', public: true },
  {
    id: t.u32().primaryKey(),
    email: t.string().unique(),
    username: t.string().unique(),
  }
);
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
[SpacetimeDB.Table(Name = "user", Public = true)]
public partial struct User
{
    [SpacetimeDB.PrimaryKey]
    public uint Id;

    [SpacetimeDB.Unique]
    public string Email;

    [SpacetimeDB.Unique]
    public string Username;
}
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
#[spacetimedb::table(name = user, public)]
pub struct User {
    #[primary_key]
    id: u32,
    #[unique]
    email: String,
    #[unique]
    username: String,
}
```

</TabItem>
</Tabs>

### Primary Key

A table can have one primary key column. The primary key represents the identity of the row. Changes that don't affect the primary key are updates; changes to the primary key are treated as delete + insert.

Only one column can be marked as a primary key, but multiple columns can be marked unique.

### Auto-Increment Columns

Use auto-increment for automatically increasing integer identifiers. Inserting a row with a zero value causes the database to assign a new unique value.

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
const post = table(
  { name: 'post', public: true },
  {
    id: t.u64().primaryKey().autoInc(),
    title: t.string(),
  }
);

const spacetimedb = schema(post);

spacetimedb.reducer('add_post', { title: t.string() }, (ctx, { title }) => {
  const inserted = ctx.db.post.insert({ id: 0, title });
  // inserted.id now contains the assigned auto-incremented value
});
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
[SpacetimeDB.Table(Name = "post", Public = true)]
public partial struct Post
{
    [SpacetimeDB.PrimaryKey]
    [SpacetimeDB.AutoInc]
    public ulong Id;
    public string Title;
}

[SpacetimeDB.Reducer]
public static void AddPost(ReducerContext ctx, string title)
{
    var inserted = ctx.Db.post.Insert(new Post { Id = 0, Title = title });
    // inserted.Id now contains the assigned auto-incremented value
}
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
#[spacetimedb::table(name = post, public)]
pub struct Post {
    #[primary_key]
    #[auto_inc]
    id: u64,
    title: String,
}

#[spacetimedb::reducer]
fn add_post(ctx: &ReducerContext, title: String) -> Result<(), String> {
    let inserted = ctx.db.post().insert(Post { id: 0, title })?;
    // inserted.id now contains the assigned auto-incremented value
    Ok(())
}
```

</TabItem>
</Tabs>

### Default Values

Default values allow you to add new columns to existing tables during [automatic migrations](/databases/automatic-migrations). When you republish a module with a new column that has a default value, existing rows are automatically populated with that default.

:::note
New columns with default values must be added at the **end** of the table definition. Adding columns in the middle of a table is not supported.
:::

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
const player = table(
  { name: 'player', public: true },
  {
    id: t.u64().primaryKey().autoInc(),
    name: t.string(),
    // New columns added with defaults
    score: t.u32().default(0),
    isActive: t.bool().default(true),
    bio: t.string().default(''),
  }
);
```

The `.default(value)` method can be chained on any column type builder. The value must match the column's type.

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
[SpacetimeDB.Table(Name = "player", Public = true)]
public partial struct Player
{
    [SpacetimeDB.PrimaryKey]
    [SpacetimeDB.AutoInc]
    public ulong Id;

    public string Name;

    // New columns added with defaults
    [SpacetimeDB.Default(0u)]
    public uint Score;

    [SpacetimeDB.Default(true)]
    public bool IsActive;

    [SpacetimeDB.Default("")]
    public string Bio;
}
```

The `[SpacetimeDB.Default(value)]` attribute specifies the default value. The value is serialized to match the column's type.

</TabItem>
<TabItem value="rust" label="Rust">

```rust
#[spacetimedb::table(name = player, public)]
pub struct Player {
    #[primary_key]
    #[auto_inc]
    id: u64,
    name: String,
    // New columns added with defaults
    #[default(0)]
    score: u32,
    #[default(true)]
    is_active: bool,
}
```

The `#[default(value)]` attribute specifies the default value. The expression must be const-evaluable (usable in a `const` context).

:::note Rust Limitation
Default values in Rust must be const-evaluable. This means you **cannot** use `String` defaults like `#[default("".to_string())]` because `.to_string()` is not a const fn. Only primitive types, enums, and other const-constructible types can have defaults.
:::

</TabItem>
</Tabs>

#### Constraints

Default values **cannot** be combined with:
- `#[primary_key]` / `[PrimaryKey]` / `.primaryKey()`
- `#[unique]` / `[Unique]` / `.unique()`
- `#[auto_inc]` / `[AutoInc]` / `.autoInc()`

This restriction exists because these constraints require the database to manage the column values, which conflicts with providing a static default.

:::warning TypeScript: Unintuitive Error Message

In TypeScript, this constraint is enforced at compile time. If you violate it, you'll see an **unintuitive error message**:

```
Expected 3 arguments, but got 2.
```

**This error means one of your columns has an invalid combination of `.default()` with `.primaryKey()`, `.unique()`, or `.autoInc()`.**

For example, this code will produce the error:

```typescript
// ERROR: default() + primaryKey() is not allowed
const badTable = table(
  { name: 'bad' },
  { id: t.u64().default(0n).primaryKey() }  // <- Causes "Expected 3 arguments"
);
```

**How to fix:** Remove either `.default()` or the constraint (`.primaryKey()`/`.unique()`/`.autoInc()`).

:::

#### Use Cases

- **Schema evolution**: Add new features to your application without losing existing data
- **Optional fields**: Provide sensible defaults for fields that may not have been tracked historically
- **Feature flags**: Add boolean columns with `default(false)` to enable new functionality gradually

---
title: Column Types
slug: /tables/column-types
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';


Columns define the structure of your tables. SpacetimeDB supports primitive types, composite types for complex data, and special types for database-specific functionality.

## Representing Collections

When modeling data that contains multiple items, you have two choices: store the collection as a column (using `Vec`, `List`, or `Array`) or store each item as a row in a separate table. This decision affects how you query, update, and subscribe to that data.

**Use a collection column when:**
- The items form an atomic unit that you always read and write together
- Order is semantically important and frequently accessed by position
- The collection is small and bounded (e.g., a fixed-size inventory)
- The items are values without independent identity

**Use a separate table when:**
- Items have independent identity and lifecycle
- You need to query, filter, or index individual items
- The collection can grow unbounded
- Clients should receive updates for individual item changes, not the entire collection
- You want to enforce referential integrity between items and other data

Consider a game inventory with ordered pockets. A `Vec<Item>` preserves pocket order naturally, but if you need to query "all items owned by player X" across multiple players, a separate `inventory_item` table with a `pocket_index` column allows that query efficiently. The right choice depends on your dominant access patterns.

## Binary Data and Files

SpacetimeDB includes optimizations for storing binary data as `Vec<u8>` (Rust), `List<byte>` (C#), or `t.array(t.u8())` (TypeScript). You can store files, images, serialized data, or other binary blobs directly in table columns.

This approach works well when:
- The binary data is associated with a specific row (e.g., a user's avatar image)
- You want the data to participate in transactions and subscriptions
- The data size is reasonable (up to several megabytes per row)

For very large files or data that changes independently of other row fields, consider external storage with a reference stored in the table.

## Type Performance

SpacetimeDB optimizes reading and writing by taking advantage of memory layout. Several factors affect performance:

**Prefer smaller types.** Use the smallest integer type that fits your data range. A `u8` storing values 0-255 uses less memory and bandwidth than a `u64` storing the same values. This reduces storage, speeds up serialization, and improves cache efficiency.

**Prefer fixed-size types.** Fixed-size types (`u32`, `f64`, fixed-size structs) allow SpacetimeDB to compute memory offsets directly. Variable-size types (`String`, `Vec<T>`) require additional indirection. When performance matters, consider fixed-size alternatives:
- Use `[u8; 32]` instead of `Vec<u8>` for fixed-length hashes or identifiers
- Use an enum with a fixed set of variants instead of a `String` for categorical data

**Consider column ordering.** Types require alignment in memory. A `u64` aligns to 8-byte boundaries, while a `u8` aligns to 1-byte boundaries. When smaller types precede larger ones, the compiler may insert padding bytes to satisfy alignment requirements. Ordering columns from largest to smallest alignment can reduce padding and improve memory density.

For example, a struct with fields `(u8, u64, u8)` may require 24 bytes due to padding, while `(u64, u8, u8)` requires only 16 bytes. This optimization is not something to follow religiously, but it can help performance in memory-intensive scenarios.

These optimizations apply across all supported languages.

## Type Reference

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

| Category | Type | TypeScript Type | Description |
|----------|------|-----------------|-------------|
| Primitive | `t.bool()` | `boolean` | Boolean value |
| Primitive | `t.string()` | `string` | UTF-8 string |
| Primitive | `t.f32()` | `number` | 32-bit floating point |
| Primitive | `t.f64()` | `number` | 64-bit floating point |
| Primitive | `t.i8()` | `number` | Signed 8-bit integer |
| Primitive | `t.u8()` | `number` | Unsigned 8-bit integer |
| Primitive | `t.i16()` | `number` | Signed 16-bit integer |
| Primitive | `t.u16()` | `number` | Unsigned 16-bit integer |
| Primitive | `t.i32()` | `number` | Signed 32-bit integer |
| Primitive | `t.u32()` | `number` | Unsigned 32-bit integer |
| Primitive | `t.i64()` | `bigint` | Signed 64-bit integer |
| Primitive | `t.u64()` | `bigint` | Unsigned 64-bit integer |
| Primitive | `t.i128()` | `bigint` | Signed 128-bit integer |
| Primitive | `t.u128()` | `bigint` | Unsigned 128-bit integer |
| Primitive | `t.i256()` | `bigint` | Signed 256-bit integer |
| Primitive | `t.u256()` | `bigint` | Unsigned 256-bit integer |
| Composite | `t.object(name, obj)` | `{ [K in keyof Obj]: T<Obj[K]> }` | Product/object type for nested data |
| Composite | `t.enum(name, variants)` | `{ tag: 'variant' } \| { tag: 'variant', value: T }` | Sum/enum type (tagged union) |
| Composite | `t.array(element)` | `T<Element>[]` | Array of elements |
| Composite | `t.option(value)` | `Value \| undefined` | Optional value |
| Composite | `t.unit()` | `{}` | Zero-field product type |
| Special | `t.identity()` | `Identity` | Unique identity for authentication |
| Special | `t.connectionId()` | `ConnectionId` | Client connection identifier |
| Special | `t.timestamp()` | `Timestamp` | Absolute point in time (microseconds since Unix epoch) |
| Special | `t.timeDuration()` | `TimeDuration` | Relative duration in microseconds |
| Special | `t.scheduleAt()` | `ScheduleAt` | Column type for scheduling reducer execution |

</TabItem>
<TabItem value="csharp" label="C#">

| Category | Type | Description |
|----------|------|-------------|
| Primitive | `bool` | Boolean value |
| Primitive | `string` | UTF-8 string |
| Primitive | `float` | 32-bit floating point |
| Primitive | `double` | 64-bit floating point |
| Primitive | `sbyte`, `short`, `int`, `long` | Signed integers (8-bit to 64-bit) |
| Primitive | `byte`, `ushort`, `uint`, `ulong` | Unsigned integers (8-bit to 64-bit) |
| Primitive | `SpacetimeDB.I128`, `SpacetimeDB.I256` | Signed 128-bit and 256-bit integers |
| Primitive | `SpacetimeDB.U128`, `SpacetimeDB.U256` | Unsigned 128-bit and 256-bit integers |
| Composite | `struct` with `[SpacetimeDB.Type]` | Product type for nested data |
| Composite | `TaggedEnum<Variants>` | Sum type (tagged union) |
| Composite | `List<T>` | List of elements |
| Composite | `T?` | Nullable/optional value |
| Special | `Identity` | Unique identity for authentication |
| Special | `ConnectionId` | Client connection identifier |
| Special | `Timestamp` | Absolute point in time (microseconds since Unix epoch) |
| Special | `TimeDuration` | Relative duration in microseconds |
| Special | `ScheduleAt` | When a scheduled reducer should execute |

</TabItem>
<TabItem value="rust" label="Rust">

| Category | Type | Description |
|----------|------|-------------|
| Primitive | `bool` | Boolean value |
| Primitive | `String` | UTF-8 string |
| Primitive | `f32`, `f64` | Floating point numbers |
| Primitive | `i8`, `i16`, `i32`, `i64`, `i128` | Signed integers |
| Primitive | `u8`, `u16`, `u32`, `u64`, `u128` | Unsigned integers |
| Composite | `struct` with `#[derive(SpacetimeType)]` | Product type for nested data |
| Composite | `enum` with `#[derive(SpacetimeType)]` | Sum type (tagged union) |
| Composite | `Vec<T>` | Vector of elements |
| Composite | `Option<T>` | Optional value |
| Special | `Identity` | Unique identity for authentication |
| Special | `ConnectionId` | Client connection identifier |
| Special | `Timestamp` | Absolute point in time (microseconds since Unix epoch) |
| Special | `TimeDuration` | Relative duration in microseconds |
| Special | `ScheduleAt` | When a scheduled reducer should execute |

</TabItem>
</Tabs>

## Complete Example

The following example demonstrates a table using primitive, composite, and special types:

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
import { table, t } from 'spacetimedb/server';

// Define a nested object type for coordinates
const Coordinates = t.object('Coordinates', {
  x: t.f64(),
  y: t.f64(),
  z: t.f64(),
});

// Define an enum for status
const Status = t.enum('Status', {
  Active: t.unit(),
  Inactive: t.unit(),
  Suspended: t.object('SuspendedInfo', { reason: t.string() }),
});

const player = table(
  { name: 'player', public: true },
  {
    // Primitive types
    id: t.u64().primaryKey().autoInc(),
    name: t.string(),
    level: t.u8(),
    experience: t.u32(),
    health: t.f32(),
    score: t.i64(),
    is_online: t.bool(),

    // Composite types
    position: Coordinates,
    status: Status,
    inventory: t.array(t.u32()),
    guild_id: t.option(t.u64()),

    // Special types
    owner: t.identity(),
    connection: t.option(t.connectionId()),
    created_at: t.timestamp(),
    play_time: t.timeDuration(),
  }
);
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
using SpacetimeDB;

public static partial class Module
{
    // Define a nested struct type for coordinates
    [SpacetimeDB.Type]
    public partial struct Coordinates
    {
        public double X;
        public double Y;
        public double Z;
    }

    // Define an enum for status
    [SpacetimeDB.Type]
    public partial record Status : TaggedEnum<(
        Unit Active,
        Unit Inactive,
        string Suspended
    )> { }

    [SpacetimeDB.Table(Name = "Player", Public = true)]
    public partial struct Player
    {
        // Primitive types
        [SpacetimeDB.PrimaryKey]
        [SpacetimeDB.AutoInc]
        public ulong Id;
        public string Name;
        public byte Level;
        public uint Experience;
        public float Health;
        public long Score;
        public bool IsOnline;

        // Composite types
        public Coordinates Position;
        public Status Status;
        public List<uint> Inventory;
        public ulong? GuildId;

        // Special types
        public Identity Owner;
        public ConnectionId? Connection;
        public Timestamp CreatedAt;
        public TimeDuration PlayTime;
    }
}
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
use spacetimedb::{SpacetimeType, Identity, ConnectionId, Timestamp, TimeDuration};

// Define a nested struct type for coordinates
#[derive(SpacetimeType)]
pub struct Coordinates {
    x: f64,
    y: f64,
    z: f64,
}

// Define an enum for status
#[derive(SpacetimeType)]
pub enum Status {
    Active,
    Inactive,
    Suspended { reason: String },
}

#[spacetimedb::table(name = player, public)]
pub struct Player {
    // Primitive types
    #[primary_key]
    #[auto_inc]
    id: u64,
    name: String,
    level: u8,
    experience: u32,
    health: f32,
    score: i64,
    is_online: bool,

    // Composite types
    position: Coordinates,
    status: Status,
    inventory: Vec<u32>,
    guild_id: Option<u64>,

    // Special types
    owner: Identity,
    connection: Option<ConnectionId>,
    created_at: Timestamp,
    play_time: TimeDuration,
}
```

</TabItem>
</Tabs>

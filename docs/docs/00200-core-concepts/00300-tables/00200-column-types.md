---
title: Column Types
slug: /tables/column-types
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';


Columns define the structure of your tables. SpacetimeDB supports a variety of column types optimized for performance.

## Primitive Types

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

## Structured Types

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

| Type | Returns | TypeScript Type | Description |
|------|---------|----------------|-------------|
| `t.object(name, obj)` | `ProductBuilder<Obj>` | `{ [K in keyof Obj]: T<Obj[K]> }` | Product/object type for nested or structured data |
| `t.row(obj)` | `RowBuilder<Obj>` | `{ [K in keyof Obj]: T<Obj[K]> }` | Row type for table schemas (allows column metadata) |
| `t.enum(name, variants)` | `SumBuilder<Obj>` or `SimpleSumBuilder` | `{ tag: 'variant' } \| { tag: 'variant', value: T }` | Sum/enum type (tagged union or simple enum) |
| `t.array(element)` | `ArrayBuilder<Element>` | `T<Element>[]` | Array of the given element type |
| `t.unit()` | `UnitBuilder` | `{}` or `undefined` | Zero-field product type (unit) |
| `t.option(value)` | `OptionBuilder<Value>` | `Value \| undefined` | Optional value type |

</TabItem>
<TabItem value="csharp" label="C#">

| Type | Description |
|------|-------------|
| `TaggedEnum<Variants>` | Tagged union/enum type for sum types |
| `T?` | Nullable/optional value |
| `List<T>` | List of elements |

</TabItem>
<TabItem value="rust" label="Rust">

| Type | Description |
|------|-------------|
| `enum` with `#[derive(SpacetimeType)]` | Sum type/tagged union |
| `Option<T>` | Optional value |
| `Vec<T>` | Vector of elements |

</TabItem>
</Tabs>

## Special Types

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

| Type | Returns | TypeScript Type | Description |
|------|---------|----------------|-------------|
| `t.identity()` | `IdentityBuilder` | `Identity` | Unique identity for authentication |
| `t.connectionId()` | `ConnectionIdBuilder` | `ConnectionId` | Client connection identifier |
| `t.timestamp()` | `TimestampBuilder` | `Timestamp` | Absolute point in time (microseconds since Unix epoch) |
| `t.timeDuration()` | `TimeDurationBuilder` | `TimeDuration` | Relative duration in microseconds |
| `t.scheduleAt()` | `ColumnBuilder<ScheduleAt, …>` | `ScheduleAt` | Special column type for scheduling reducer execution |

</TabItem>
<TabItem value="csharp" label="C#">

| Type | Description |
|------|-------------|
| `Identity` | Unique identity for authentication |
| `ConnectionId` | Client connection identifier |
| `Timestamp` | Absolute point in time (microseconds since Unix epoch) |
| `TimeDuration` | Relative duration in microseconds |
| `ScheduleAt` | When a scheduled reducer should execute (either at a specific time or at repeating intervals) |

</TabItem>
<TabItem value="rust" label="Rust">

| Type | Description |
|------|-------------|
| `Identity` | Unique identity for authentication |
| `ConnectionId` | Client connection identifier |
| `Timestamp` | Absolute point in time (microseconds since Unix epoch) |
| `Duration` | Relative duration |
| `ScheduleAt` | When a scheduled reducer should execute (either `Time(Timestamp)` or `Interval(Duration)`) |

</TabItem>
</Tabs>

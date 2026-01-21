# Benchmark Failure Analysis

Generated from: `/__w/SpacetimeDB/SpacetimeDB/tools/xtask-llm-benchmark/../../docs/llms/docs-benchmark-details.json`

## Summary

- **Total failures analyzed**: 36

---

# SpacetimeDB Benchmark Test Failures Analysis

## Rust / rustdoc_json Failures

### Group: Compile/Publish Errors (3 failures)

#### Failure: t_002_scheduled_table
**The generated code**:
```rust
use spacetimedb::{table, reducer, ReducerContext, Table, ScheduleAt};

#[table(name = tick_timer, scheduled(reducer = tick, column = scheduled_at))]
pub struct TickTimer {
    #[primary_key]
    #[auto_inc]
    scheduled_id: u64,
    scheduled_at: ScheduleAt,
}

#[reducer]
pub fn tick(_ctx: &ReducerContext, _timer: TickTimer) {
}

#[reducer(init)]
pub fn init(ctx: &ReducerContext) {
    ctx.db
        .tick_timer()
        .insert(TickTimer {
            scheduled_id: 0,
            scheduled_at: ScheduleAt::EveryMicros(50_000),
        });
}
```
**The golden example**:
```rust
use spacetimedb::{reducer, table, ReducerContext, ScheduleAt, Table};
use std::time::Duration;

#[table(name = tick_timer, scheduled(tick))]
pub struct TickTimer {
    #[primary_key]
    #[auto_inc]
    pub scheduled_id: u64,
    pub scheduled_at: ScheduleAt,
}

#[reducer]
pub fn tick(_ctx: &ReducerContext, _schedule: TickTimer) {
}

#[reducer(init)]
pub fn init(ctx: &ReducerContext) {
    let every_50ms: ScheduleAt = Duration::from_millis(50).into();
    ctx.db.tick_timer().insert(TickTimer {
        scheduled_id: 0,
        scheduled_at: every_50ms,
    });
}
```
**The error**: `publish_error: spacetime publish failed (exit=1)`

**Explain the difference**: The main issues in the generated code are:
- The `scheduled` attribute should not specify both `reducer` and `column`.
- The use of `ScheduleAt::EveryMicros(...)` should be replaced with the `Interval` type using `Duration`.

**Root cause**: Documentation on how to properly specify scheduled attributes in structured tables might be unclear or lacking comprehensive examples.

**Recommendation**: Update the documentation to clarify the use of scheduled columns and attributes, providing a clear example of how to set timed intervals properly.

---

#### Failure: t_016_sum_type_columns
**The generated code**:
```rust
use spacetimedb::{table, reducer, ReducerContext, Table, SpacetimeType};

#[derive(SpacetimeType)]
pub struct Rect {
    width: i32,
    height: i32,
}

#[derive(SpacetimeType)]
pub enum Shape {
    Circle(i32),
    Rectangle(Rect),
}

#[table(name = drawing)]
pub struct Drawing {
    #[primary_key]
    id: i32,
    a: Shape,
    b: Shape,
}

#[reducer]
pub fn seed(ctx: &ReducerContext) {
    let drawing = ctx.db.drawing();
    for row in drawing.iter() {
        drawing.delete(&row);
    }
    drawing.insert(Drawing {
        id: 1,
        a: Shape::Circle(10),
        b: Shape::Rectangle(Rect { width: 4, height: 6 }),
    });
}
```
**The golden example**:
```rust
use spacetimedb::{reducer, table, ReducerContext, SpacetimeType, Table};

#[derive(SpacetimeType, Clone, Debug)]
pub struct Rect {
    pub width: i32,
    pub height: i32,
}

#[derive(SpacetimeType, Clone, Debug)]
pub enum Shape {
    Circle(i32),
    Rectangle(Rect),
}

#[table(name = drawing)]
pub struct Drawing {
    #[primary_key]
    pub id: i32,
    pub a: Shape,
    pub b: Shape,
}

#[reducer]
pub fn seed(ctx: &ReducerContext) {
    ctx.db.drawing().insert(Drawing {
        id: 1,
        a: Shape::Circle(10),
        b: Shape::Rectangle(Rect { width: 4, height: 6 }),
    });
}
```
**The error**: `publish_error: spacetime publish failed (exit=1)`

**Explain the difference**: The generated code lacked public visibility on struct fields necessary for serialization and actual utilization within the SpacetimeDB context.

**Root cause**: Insufficient documentation on the visibility requirements for SpacetimeType structures may lead to confusion regarding struct accessibility.

**Recommendation**: Emphasize the necessity of public fields with detailed examples for types and their usage in the documentation.

---

### Group: Other Failures (6 failures)

#### Failure: t_003_struct_in_table
**The generated code**:
```rust
use spacetimedb::{table, reducer, ReducerContext, SpacetimeType};

#[derive(SpacetimeType)]
pub struct Position {
    x: i32,
    y: i32,
}

#[table(name = entity)]
pub struct Entity {
    #[primary_key]
    id: i32,
    pos: Position,
}

#[reducer(init)]
pub fn init(_ctx: &ReducerContext) {}
```
**The golden example**:
```rust
use spacetimedb::{table, SpacetimeType};

#[derive(SpacetimeType, Clone, Debug)]
pub struct Position {
    pub x: i32,
    pub y: i32,
}

#[table(name = entity)]
pub struct Entity {
    #[primary_key]
    pub id: i32,
    pub pos: Position,
}
```
**The error**: `schema_parity: reducers differ - expected [], got ["init()"]`

**Explain the difference**: While the generated code correctly defines a table and a reducer, the lack of public access for fields in the `Position` struct caused it to not serialize properly.

**Root cause**: The documentation may not sufficiently stress the importance of field visibility for user-defined types in SpacetimeDB.

**Recommendation**: Include explicit instructions or examples that reinforce the need for `pub` visibility for struct fields used in tables and reducers.

---

#### Failure: t_012_spacetime_product_type
**The generated code**:
```rust
use spacetimedb::{table, reducer, ReducerContext, Table, SpacetimeType};

#[derive(SpacetimeType)]
struct Score {
    left: i32,
    right: i32,
}

#[table(name = result)]
struct ResultRow {
    #[primary_key]
    id: i32,
    value: Score,
}

#[reducer]
fn set_score(ctx: &ReducerContext, id: i32, left: i32, right: i32) {
    ctx.db.result().insert(ResultRow { id, value: Score { left, right } });
}
```
**The golden example**:
```rust
use spacetimedb::{reducer, table, ReducerContext, SpacetimeType, Table};

#[derive(SpacetimeType, Clone, Debug)]
pub struct Score {
    pub left: i32,
    pub right: i32,
}

#[table(name = result)]
pub struct ResultRow {
    #[primary_key]
    pub id: i32,
    pub value: Score,
}

#[reducer]
pub fn set_score(ctx: &ReducerContext, id: i32, left: i32, right: i32) {
    ctx.db.result().insert(ResultRow { id, value: Score { left, right } });
}
```
**The error**: `spacetime sql failed: no such table: result`.

**Explain the difference**: The missing public access on the `ResultRow` struct fields prevented successful serialization and table mapping.

**Root cause**: Again, the documentation lacks sufficient emphasis on public attributes for user-defined types in the context of SpacetimeDB.

**Recommendation**: Add a guide on the visibility and accessibility requirements for structs that are intended to be used as database records.

---

## C# / docs Failures (5 total)

### Group: Other Failures (4 failures)

#### Failure: t_014_elementary_columns
**The generated code**:
```csharp
using SpacetimeDB;

public static partial class Module
{
    [SpacetimeDB.Table(Name = "Primitive", Public = true)]
    public partial struct Primitive
    {
        [SpacetimeDB.PrimaryKey]
        public int Id;
        public int Count;
        public long Total;
        public float Price;
        public double Ratio;
        public bool Active;
        public string Name;
    }

    [SpacetimeDB.Reducer]
    public static void Seed(ReducerContext ctx)
    {
        ctx.Db.Primitive.Insert(new Primitive
        {
            Id = 1,
            Count = 2,
            Total = 3000000000L,
            Price = 1.5f,
            Ratio = 2.25,
            Active = true,
            Name = "Alice"
        });
    }
}
```
**The golden example**:
```csharp
using SpacetimeDB;

public static partial class Module
{
    [Table(Name = "Primitive")]
    public partial struct Primitive
    {
        [PrimaryKey] 
        public int Id;
        public int Count;
        public long Total;
        public float Price;
        public double Ratio;
        public bool Active;
        public string Name;
    }

    [Reducer]
    public static void Seed(ReducerContext ctx)
    {
        ctx.Db.Primitive.Insert(new Primitive
        {
            Id = 1,
            Count = 2,
            Total = 3000000000L,
            Price = 1.5f,
            Ratio = 2.25,
            Active = true,
            Name = "Alice"
        });
    }
}
```
**The error**: `no such table: Primitive`.

**Explain the difference**: The generated code incorrectly added the `Public` attribute in the table definition, causing it not to align with expected database schemas.

**Root cause**: There may be unclear directives about accessibility requirements for struct definitions within the documentation.

**Recommendation**: Clarify in the documentation when to use accessibility modifiers and provide straightforward examples of valid struct declarations.

---

This analysis summarizes actionable insights for documentation enhancements based on SpacetimeDB benchmark failures. By updating documentation to address the common pitfalls outlined above, it can greatly reduce frustration and improve the developer experience.

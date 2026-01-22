# Benchmark Failure Analysis

Generated from: `/__w/SpacetimeDB/SpacetimeDB/tools/xtask-llm-benchmark/../../docs/llms/docs-benchmark-details.json`

## Summary

- **Total failures analyzed**: 34

---

# Analysis of SpacetimeDB Benchmark Test Failures

## Rust / rustdoc_json Failures

### Compile/Publish Errors
#### Failure Group: `t_002_scheduled_table` and `t_017_scheduled_columns`

**Generated Code**:
```rust
use spacetimedb::{ReducerContext, ScheduleAt, Table};

#[spacetimedb::table(name = tick_timer, schedule(reducer = tick, column = scheduled_at))]
pub struct TickTimer {
    #[primary_key]
    #[auto_inc]
    scheduled_id: u64,
    scheduled_at: ScheduleAt,
}

#[spacetimedb::reducer(init)]
pub fn init(ctx: &ReducerContext) {
    ctx.db.tick_timer().insert(TickTimer {
        scheduled_id: 0,
        scheduled_at: ScheduleAt::RepeatMicros(50_000),
    });
}
```

**Expected Example**:
```rust
use spacetimedb::{reducer, table, ReducerContext, ScheduleAt, Table};
use std::time::Duration;

#[table(name = tick_timer, scheduled(tick))]
pub struct TickTimer {
    #[primary_key]
    pub scheduled_id: u64,
    pub scheduled_at: ScheduleAt,
}

#[reducer]
pub fn tick(_ctx: &ReducerContext, _row: TickTimer) -> Result<(), String> {
    Ok(())
}

#[reducer(init)]
pub fn init(ctx: &ReducerContext) -> Result<(), String> {
    ctx.db.tick_timer().insert(TickTimer {
        scheduled_id: 0,
        scheduled_at: ScheduleAt::Interval(Duration::from_millis(50).into()),
    });
    Ok(())
}
```

**Error Message**: 
`publish_error: spacetime publish failed (exit=1)`

**Explanation of Differences**:
- The `reducer` and `table` macros were not correctly defined with required attributes such as `#[reducer]`.
- The generated code used `ScheduleAt::RepeatMicros` instead of `ScheduleAt::Interval(Duration::from_millis(...))`.

**Root Cause**:
The documentation is unclear regarding the required attributes for reducers and tables, especially with respect to scheduling functionality.

**Recommendation**:
Update documentation to include examples of proper usage patterns for scheduled tables and reducers:
- Specify correct syntax for the `ScheduleAt` enum options.
- Illustrate with more examples showing usage of the `#[table]` and `#[reducer]` attributes.

---

### Other Failures
#### Failure Group: `t_003_struct_in_table` and `t_012_spacetime_product_type`

**Generated Code**:
```rust
use spacetimedb::{ReducerContext, Table, SpacetimeType};

#[derive(SpacetimeType, Clone)]
pub struct Position {
    x: i32,
    y: i32,
}

#[spacetimedb::table(name = entity)]
pub struct Entity {
    #[primary_key]
    id: i32,
    pos: Position,
}

#[spacetimedb::reducer(init)]
pub fn init(_ctx: &ReducerContext) {}
```

**Expected Example**:
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

**Error Message**: 
`schema_parity: reducers differ - expected [], got ["add_entity()", "init()"]`

**Explanation of Differences**:
- Missing `pub` in struct fields (`x`, `y`, `id`), which hindered schema detection in the DB.
- Lack of `Debug` and `Clone` traits in output.

**Root Cause**:
Documentation should specify visibility requirements for struct properties, especially for those that are part of a schema.

**Recommendation**:
Revise docs to explain the importance of visibility modifiers (`pub`) in struct definitions. Provide clear examples where struct attributes must be public to function properly.

---

### Failures Specific to Scheduled Columns and Constraints

#### Failure Group: `t_016_sum_type_columns`

**Generated Code**:
```rust
use spacetimedb::{ReducerContext, Table};

#[derive(spacetimedb::SpacetimeType)]
pub enum Shape {
    Circle(i32),
    Rectangle(Rect),
}

#[spacetimedb::table(name = drawing)]
pub struct Drawing {
    #[primary_key]
    id: i32,
    a: Shape,
    b: Shape,
}
```

**Expected Example**:
```rust
use spacetimedb::{reducer, table, ReducerContext, SpacetimeType};

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
```

**Error Message**:
`Error: no such table: 'drawings'`

**Explanation of Differences**:
- Missing `pub` in struct fields (`id`, `a`, `b`), which prevents proper schema definition.

**Root Cause**:
The documentation is unclear about the need for visibility specifiers in enums and structs used within the DB context.

**Recommendation**:
Add details to documentation highlighting that all fields in struct and enum definitions should be marked as public for proper database interaction. Provide consistent examples.

---

## Rust / docs Failures

### Timeout Issues

#### Failure Group: Multiple Test Timeouts

**Common Problem**: 
Several tests including `t_001_basic_tables` and `t_020_ecs` experienced timeouts during execution.

**Root Cause**:
This likely relates to complex processing without sufficient optimization or poorly defined database schema causing query execution to take too long.

**Recommendation**:
1. Include benchmarks or performance expectations in documentation for various queries.
2. Suggest practices or patterns for optimizing database schema and queries to avoid timeouts.

---

## C# / docs Failures

### Other Failures
#### Failure Group: `t_014_elementary_columns` and `t_016_sum_type_columns`

**Generated Code**:
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
}
```

**Expected Example**:
```csharp
using SpacetimeDB;

public static partial class Module
{
    [Table(Name = "Primitive")]
    public partial struct Primitive
    {
        [PrimaryKey] public int Id;
        public int Count;
        public long Total;
        public float Price;
        public double Ratio;
        public bool Active;
        public string Name;
    }
}
```

**Error Message**:
`Error: no such table: 'primitive'`

**Explanation of Differences**:
- Extraneous `Public = true` parameter in the attribute definition.
- Missing proper struct visibility modifiers.

**Root Cause**:
Documentation does not clarify the effects of the visibility modifier on table definitions.

**Recommendation**:
Revise the documentation to clarify:
1. The specific usage of attribute parameters and their implications. 
2. Clarify that struct properties must be public to be correctly recognized in the DB schema.

---

This analysis identifies specific issues in implementation and documentation that led to benchmark failures in SpacetimeDB. Addressing these will enhance clarity and improve user experience in building SpacetimeDB applications.

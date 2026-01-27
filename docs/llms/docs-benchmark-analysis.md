# Benchmark Failure Analysis

Generated from: `/__w/SpacetimeDB/SpacetimeDB/tools/xtask-llm-benchmark/../../docs/llms/docs-benchmark-details.json`

## Summary

- **Total failures analyzed**: 34

---

# Analysis of SpacetimeDB Benchmark Test Failures

## Rust / `rustdoc_json` Failures

### Compile/Publish Errors

#### t_002_scheduled_table and t_017_scheduled_columns

**1. The generated code**:
```rust
use spacetimedb::{table, reducer, ReducerContext, Table, ScheduleAt};

#[table(name = tick_timer, schedule(reducer = tick, column = scheduled_at))]
pub struct TickTimer {
    #[primary_key]
    #[auto_inc]
    scheduled_id: u64,
    scheduled_at: ScheduleAt,
}

#[reducer(init)]
pub fn init(ctx: &ReducerContext) {
    if ctx.db.tick_timer().count() == 0 {
        ctx.db.tick_timer().insert(TickTimer {
            scheduled_id: 0,
            scheduled_at: ScheduleAt::repeat_micros(50_000),
        });
    }
}

#[reducer]
pub fn tick(_ctx: &ReducerContext, _scheduled: TickTimer) {}
```

**2. The golden example**:
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

**3. The error**: 
`publish_error: spacetime publish failed (exit=1)`

**4. Explain the difference**:
The LLM-generated code uses `ScheduleAt::repeat_micros(50_000)` while the golden example uses `ScheduleAt::Interval(Duration::from_millis(50).into())`, which is the correct way to specify a scheduling interval.

**5. Root cause**: 
The documentation may not clearly explain the necessary type conversions for `ScheduleAt`, specifically the use of `Duration`.

**6. Recommendation**: 
Update the documentation to specify that `ScheduleAt` should use `Duration` for intervals. Provide clear examples demonstrating the correct usage.

---

### Other Failures

#### t_003_struct_in_table

**1. The generated code**:
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

**2. The golden example**:
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

**3. The error**: 
`schema_parity: reducers differ - expected [], got ["init()"]`

**4. Explain the difference**:
The LLM-generated code references a reducer (`init`) that does not match the expected structure of the table, which requires the `init` function to have a return type conforming to `Result<(), String>`.

**5. Root cause**: 
The documentation does not make it clear that all reducers need to behave consistently with input/signatures.

**6. Recommendation**: 
Clarify in the documentation that the reducer functions must follow certain patterns and signatures, especially with regard to return types.

---

### Sum Type Failures

#### t_013_spacetime_sum_type and t_015_product_type_columns

**1. The generated code**:
```rust
use spacetimedb::{table, reducer, ReducerContext, Table, SpacetimeType, UniqueColumn};

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

#[table(name = result)]
pub struct ResultRow {
    #[primary_key]
    id: i32,
    value: Shape,
}

#[reducer]
pub fn set_circle(ctx: &ReducerContext, id: i32, radius: i32) {
    ctx.db.result().id().delete(&id);
    ctx.db.result().insert(ResultRow {
        id,
        value: Shape::Circle(radius),
    });
}
```

**2. The golden example**:
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

#[table(name = result)]
pub struct ResultRow {
    #[primary_key]
    pub id: i32,
    pub value: Shape,
}

#[reducer]
pub fn set_circle(ctx: &ReducerContext, id: i32, radius: i32) {
    ctx.db.result().insert(ResultRow { id, value: Shape::Circle(radius) });
}
```

**3. The error**: 
`no such table: result` or `no such table: profile`

**4. Explain the difference**:
The generated code omits declaring public visibility for the fields in the structs, which is necessary for SpacetimeDB to properly reflect the structure of the data.

**5. Root cause**: 
Lack of clarity in the documentation regarding field visibility in structs.

**6. Recommendation**: 
Enhance the documentation to explicitly state that all struct fields used in SpacetimeDB must be `pub`, and provide examples to illustrate this requirement.

---

### Constraints and Schema Issues

#### t_018_constraints

**1. The generated code**:
```rust
use spacetimedb::{table, reducer, ReducerContext, Table};

#[table(name = account, index(name = by_name, btree(columns = [name])))]
pub struct Account {
    #[primary_key]
    id: i32,
    #[unique]
    email: String,
    name: String,
}

#[reducer]
pub fn seed(ctx: &ReducerContext) {
    ctx.db.account().insert(Account { id: 1, email: "a@example.com".to_string(), name: "Alice".to_string() });
    ctx.db.account().insert(Account { id: 2, email: "b@example.com".to_string(), name: "Bob".to_string() });
}
```

**2. The golden example**:
```rust
use spacetimedb::{reducer, table, ReducerContext, Table};

#[table(
    name = account,
    index(name = by_name, btree(columns = [name]))
)]
pub struct Account {
    #[primary_key]
    pub id: i32,
    #[unique]
    pub email: String,
    pub name: String,
}

#[reducer]
pub fn seed(ctx: &ReducerContext) {
    ctx.db.account().insert(Account { id: 1, email: "a@example.com".into(), name: "Alice".into() });
    ctx.db.account().insert(Account { id: 2, email: "b@example.com".into(), name: "Bob".into() });
}
```

**3. The error**: 
`no such table: account`

**4. Explain the difference**:
The generated code lacks `pub` visibility for the fields in the `Account` struct and fails to use `.into()` for string initialization.

**5. Root cause**: 
Documentation does not emphasize the importance of field visibility and conversions when using SpacetimeDB.

**6. Recommendation**: 
Update the documentation to clarify that struct fields must be public and suggest correct practices for type conversions, including `.into()` for string initialization.

---

## C# / `docs` Failures

### Scheduled Columns Failures

#### t_016_sum_type_columns and t_017_scheduled_columns

**1. The generated code**:
```csharp
using SpacetimeDB;

public static partial class Module
{
    [SpacetimeDB.Type]
    public partial struct Circle
    {
        public int Radius;
    }

    [SpacetimeDB.Type]
    public partial struct Rectangle
    {
        public int Width;
        public int Height;
    }

    [SpacetimeDB.Type]
    public partial record Shape : TaggedEnum<(
        Circle Circle,
        Rectangle Rectangle
    )> { }

    [SpacetimeDB.Table(Name = "Drawing", Public = true)]
    public partial struct Drawing
    {
        [SpacetimeDB.PrimaryKey]
        public int Id;
        public Shape A;
        public Shape B;
    }
}
```

**2. The golden example**:
```csharp
using SpacetimeDB;

public static partial class Module
{
    [Type]
    public partial struct Circle { public int Radius; }

    [Type]
    public partial struct Rectangle { public int Width; public int Height; }

    [Type]
    public partial record Shape : TaggedEnum<(Circle Circle, Rectangle Rectangle)> { }

    [Table(Name = "Drawing")]
    public partial struct Drawing
    {
        [PrimaryKey] public int Id;
        public Shape A;
        public Shape B;
    }
}
```

**3. The error**: 
`no such table: drawings`

**4. Explain the difference**:
The generated code lacks `public` access modifiers for structs, which leads to conflicts with the accessibility required when accessing tables.

**5. Root cause**: 
Insufficient explanation in the documentation about the required access modifiers for types used with SpacetimeDB.

**6. Recommendation**: 
Add more guidance in the documentation on the necessity of public access modifiers for types to ensure proper table access.

---

This comprehensive analysis highlights the specific sinks, root causes, and actionable recommendations to improve both the SpacetimeDB documentation and the generated code's adherence to expected standards.

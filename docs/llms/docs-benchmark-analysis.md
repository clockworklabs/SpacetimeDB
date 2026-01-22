# Benchmark Failure Analysis

Generated from: `/__w/SpacetimeDB/SpacetimeDB/tools/xtask-llm-benchmark/../../docs/llms/docs-benchmark-details.json`

## Summary

- **Total failures analyzed**: 35

---

# Analysis of SpacetimeDB Benchmark Test Failures

## Rust / rustdoc_json Failures

### Compile/Publish Errors

#### 1. Test: t_002_scheduled_table

**The generated code**:
```rust
use spacetimedb::{table, reducer, ReducerContext, Table, ScheduleAt};

#[table(name = tick_timer, schedule(column = scheduled_at, reducer = tick))]
pub struct TickTimer {
    #[primary_key]
    #[auto_inc]
    scheduled_id: u64,
    scheduled_at: ScheduleAt,
}

#[reducer]
pub fn tick(_ctx: &ReducerContext) {
    // Scheduled reducer invoked by tick_timer
}

#[reducer(init)]
pub fn init(ctx: &ReducerContext) {
    ctx.db.tick_timer().insert(TickTimer {
        scheduled_id: 0,
        scheduled_at: ScheduleAt::RepeatMicros(50_000),
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
    scheduled_id: u64,
    scheduled_at: ScheduleAt,
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

**Error**: `publish_error: spacetime publish failed (exit=1)`

**Explain the difference**:
- The incorrect `schedule` attribute used improper syntax (column and reducer mentioned separately).
- The `tick` reducer should accept a `TickTimer` row parameter.
- The `scheduled_at` assignment is incorrect (incorrect type used).

**Root cause**:
Documentation does not clarify the need to use the `scheduled` attribute correctly and specify function signatures for reducers expected by the API.

**Recommendation**:
Update documentation to “Must use attribute `scheduled(tick)` and ensure the reducer functions accept the required parameters as specified.” Example:

```rust
#[table(name = tick_timer, scheduled(tick))]
// correct usage
```

---

#### 2. Test: t_017_scheduled_columns

**The generated code**:
```rust
use spacetimedb::{reducer, table, ReducerContext, ScheduleAt, Table};

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
            scheduled_at: ScheduleAt::repeat_us(50_000),
        });
    }
}

#[reducer(scheduled)]
pub fn tick(_ctx: &ReducerContext, _row: TickTimer) {}
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
pub fn tick(_ctx: &ReducerContext, _schedule: TickTimer) {}

#[reducer(init)]
pub fn init(ctx: &ReducerContext) {
    let every_50ms: ScheduleAt = Duration::from_millis(50).into();
    ctx.db.tick_timer().insert(TickTimer {
        scheduled_id: 0,
        scheduled_at: every_50ms,
    });
}
```

**Error**: `publish_error: spacetime publish failed (exit=1)`

**Explain the difference**:
- Incorrect `schedule` attribute syntax causes failures in specifying a reducer.
- The `scheduled_at` type is incorrectly set to `repeat_us`, not conforming to the Interval structure.

**Root cause**:
Lack of detailed guidelines in documentation for correctly annotating and defining reducer routes expected during schema description.

**Recommendation**:
Include a guide section in the documentation detailing all attributes for tables and reducers, specifically emphasizing parameter types and structure expectations. 

---

### Other Failures

#### 3. Test: t_003_struct_in_table

**The generated code**:
```rust
use spacetimedb::{table, reducer, ReducerContext, Table, SpacetimeType};

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

#[reducer]
pub fn add(ctx: &ReducerContext, id: i32, x: i32, y: i32) {
    ctx.db.entity().insert(Entity { id, pos: Position { x, y } });
}
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

**Error**: `schema_parity: reducers differ - expected [], got ["add()"]`

**Explain the difference**:
- The reducer function should not be necessary if there is no reduction logic within the example.
- The `Position` struct lacks appropriate visibility modifiers, leading to nil results in the API.

**Root cause**:
Documentation does not clarify when reducers are expected and how to define and declare public fields necessary for SpacetimeDB API to function correctly.

**Recommendation**:
Clarify in the documentation that reducers should only be included when necessary, along with showcasing fields with visibility for structs.

---

#### 4. Test: t_013_spacetime_sum_type

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

#[table(name = result)]
pub struct ResultRow {
    #[primary_key]
    id: i32,
    value: Shape,
}

#[reducer]
pub fn set_circle(ctx: &ReducerContext, id: i32, radius: i32) {
    ctx.db.result().insert(ResultRow {
        id,
        value: Shape::Circle(radius),
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

**Error**: `sum_type_row_parity: spacetime sql failed: Error: no such table: result`

**Explain the difference**:
- Visibility modifiers are absent on the `Rect` and `Shape` structs.
- The naming convention for the struct is inconsistent with usage.

**Root cause**:
Documentation does not specify the significance of visibility in structs and enums directly influencing the usage of their produced database entries.

**Recommendation**:
Amend documentation to highlight the importance of public fields and specifically demonstrate the effect of struct and enum visibility.

---

### Additional Recommendations

1. **Documentation Structure**: Enhance the overall organization and clarity in the documentation relating to syntax rules. Providing clear examples of common pitfalls with API usages can prevent such issues.

2. **Consistency**: Ensure that examples maintain consistent use of visibility and thorough descriptions of error types relevant to expected outcomes.

3. **Error Handling**: Clarify the expected patterns and structures for error handling within API calls and data flows.

By implementing these documentation changes and clarifications, debug routines should become more intuitive, resulting in fewer benchmark test failures.

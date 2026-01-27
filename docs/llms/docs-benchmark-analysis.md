# Benchmark Failure Analysis

Generated from: `/__w/SpacetimeDB/SpacetimeDB/tools/xtask-llm-benchmark/../../docs/llms/docs-benchmark-details.json`

## Summary

- **Total failures analyzed**: 35

---

# Analysis of SpacetimeDB Benchmark Failures

## Rust / rustdoc_json Failures

### Compile/Publish Errors (2 failures)

#### t_002_scheduled_table
**Generated Code**:
```rust
use spacetimedb::{table, reducer, ReducerContext, Table, UniqueColumn, ScheduleAt};

#[table(name = tick_timer, schedule(reducer = tick, column = scheduled_at))]
pub struct TickTimer {
    #[primary_key]
    #[auto_inc]
    scheduled_id: u64,
    scheduled_at: ScheduleAt,
}

#[reducer(init)]
pub fn init(ctx: &ReducerContext) {
    let tbl = ctx.db.tick_timer();

    let mut keep_id: Option<u64> = None;
    for row in tbl.iter() {
        if keep_id.is_none() {
            let mut r = row;
            r.scheduled_at = ScheduleAt::IntervalMicros(50_000);
            let updated = tbl.scheduled_id().update(r);
            keep_id = Some(updated.scheduled_id);
        } else {
            tbl.scheduled_id().delete(&row.scheduled_id);
        }
    }

    if keep_id.is_none() {
        let _ = tbl.insert(TickTimer {
            scheduled_id: 0,
            scheduled_at: ScheduleAt::IntervalMicros(50_000),
        });
    }
}

#[reducer]
pub fn tick(_ctx: &ReducerContext) {}
```

**Expected (Golden Code)**:
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

**Differences**:
- The LLM used `schedule(reducer = tick, column = scheduled_at)` instead of the expected `scheduled(tick)`.
- The `tick` reducer function lacked a return type.
- The `init` function's `scheduled_at` field was incorrectly assigned using `IntervalMicros(50_000)` instead of `Interval(Duration::from_millis(50).into())`.

**Root Cause**: Misunderstanding of how to set up scheduling and return types for reducer functions in Rust.

**Recommendation**: Update the documentation to clarify:
- How to define scheduling within the table attribute.
- The need for specifying return types in reducer functions.
- Correct usage examples for setting up intervals for scheduling.

---

#### t_017_scheduled_columns
**Generated Code**:
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
        ctx.db
            .tick_timer()
            .insert(TickTimer {
                scheduled_id: 0,
                scheduled_at: ScheduleAt::repeat_micros(50_000),
            });
    }
}

#[reducer]
pub fn tick(_ctx: &ReducerContext, _row: TickTimer) {}
```

**Expected (Golden Code)**:
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

**Differences**:
- The LLM again used the incorrect syntax for scheduling.
- Used `ScheduleAt::repeat_micros(50_000)` while the expected syntax is using `Duration::from_millis(...)`.

**Root Cause**: Similar to the first failure, a misunderstanding of the correct APIs for table scheduling and the desired syntax.

**Recommendation**: Amend documentation or the code generation model to ensure the correct formatting and usage patterns are demonstrated, especially around scheduling and types.

---

### Other Failures (5 failures)

#### t_013_spacetime_sum_type (2/3 tests passed)
**Generated Code**:
```rust
use spacetimedb::{ReducerContext, Table, SpacetimeType};

#[derive(SpacetimeType)]
pub struct Rect {
    pub width: i32,
    pub height: i32,
}

#[derive(SpacetimeType)]
pub enum Shape {
    Circle(i32),
    Rectangle(Rect),
}

#[spacetimedb::table(name = result)]
pub struct ResultRow {
    #[primary_key]
    pub id: i32,
    pub value: Shape,
}

#[spacetimedb::reducer]
pub fn set_circle(ctx: &ReducerContext, id: i32, radius: i32) {
    ctx.db.result().insert(ResultRow {
        id,
        value: Shape::Circle(radius),
    });
}
```

**Expected (Golden Code)**:
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

**Error**: `no such table: result`.

**Differences**:
- The LLM missed implementing the `Clone` and `Debug` traits for `Rect` and `Shape`.
- Notably, this could have contributed to the inability to handle implications regarding mutability and state during tests.

**Root Cause**: The omission of `Clone` and `Debug` traits in derived types may confuse the necessary behavior for interacting with the database and running appropriate tests.

**Recommendation**: Include compliance with trait derivations in the documentation and examples to reflect necessary traits for database interaction.

---

#### t_015_product_type_columns (2/3 tests passed)
**Generated Code**:
```rust
use spacetimedb::{table, reducer, ReducerContext, Table, SpacetimeType};

#[derive(SpacetimeType)]
pub struct Address {
    street: String,
    zip: i32,
}

#[derive(SpacetimeType)]
pub struct Position {
    x: i32,
    y: i32,
}

#[table(name = profile)]
pub struct Profile {
    #[primary_key]
    id: i32,
    home: Address,
    work: Address,
    pos: Position,
}

#[reducer]
pub fn seed(ctx: &ReducerContext) {
    ctx.db.profile().insert(Profile {
        id: 1,
        home: Address { street: "1 Main".to_string(), zip: 11111 },
        work: Address { street: "2 Broad".to_string(), zip: 22222 },
        pos: Position { x: 7, y: 9 },
    });
}
```

**Expected (Golden Code)**:
```rust
use spacetimedb::{reducer, table, ReducerContext, SpacetimeType, Table};

#[derive(SpacetimeType, Clone, Debug)]
pub struct Address {
    pub street: String,
    pub zip: i32,
}

#[derive(SpacetimeType, Clone, Debug)]
pub struct Position {
    pub x: i32,
    pub y: i32,
}

#[table(name = profile)]
pub struct Profile {
    #[primary_key]
    pub id: i32,
    pub home: Address,
    pub work: Address,
    pub pos: Position,
}

#[reducer]
pub fn seed(ctx: &ReducerContext) {
    ctx.db.profile().insert(Profile {
        id: 1,
        home: Address { street: "1 Main".into(), zip: 11111 },
        work: Address { street: "2 Broad".into(), zip: 22222 },
        pos: Position { x: 7, y: 9 },
    });
}
```

**Error**: `no such table: profile`.

**Differences**:
- Missing the `pub` visibility modifier on the struct fields resulted in inadequate exposure to the database layer for runtime reflection or generation.

**Root Cause**: The LLM lacks clarity in visibility context leading to struct field inaccessibility.

**Recommendation**: Update documentation examples to emphasize the importance of `pub` modifiers for public API, particularly in database-related types.

---

### Recommendation Summary
The fundamental issues in the test failures stem from:
- Incorrect syntax (particularly in `schedule` attributes).
- Missing visibility modifiers (`pub`).
- Omitted trait implementations necessary for database operations.

Enhancing the clarity, correctness, and completeness of documentation is essential. Specific examples that match the expected outputs must illustrate successful patterns for users. This would significantly lower the occurrence of erroneous submissions, leading to successful builds and test runs.

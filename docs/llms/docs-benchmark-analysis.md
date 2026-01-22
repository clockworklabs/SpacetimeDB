# Benchmark Failure Analysis

Generated from: `/__w/SpacetimeDB/SpacetimeDB/tools/xtask-llm-benchmark/../../docs/llms/docs-benchmark-details.json`

## Summary

- **Total failures analyzed**: 36

---

# Analysis of SpacetimeDB Benchmark Test Failures

## Rust / rustdoc_json Failures (10 Total)

### Compile/Publish Errors (2 Failures)

#### 1. t_002_scheduled_table
- **The generated code**:
    ```rust
    use spacetimedb::{table, reducer, ReducerContext, Table, ScheduleAt};

    #[table(name = tick_timer, scheduled(reducer = tick, column = scheduled_at))]
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
    pub fn tick(_ctx: &ReducerContext) {
    }
    ```
- **The golden example**:
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
- **The error**: `publish_error: spacetime publish failed (exit=1)`
- **Explain the difference**:
    - The schedule reducer was improperly defined, and the set value for `scheduled_at` was incorrect. It should use an `Interval` rather than a repeat.
- **Root cause**: Documentation does not clarify the proper use of `ScheduleAt` and the specifics of scheduling behaviors.
- **Recommendation**: Update documentation with clear examples demonstrating how to define and use scheduled tables correctly.

---

#### 2. t_017_scheduled_columns
- **The generated code**:
    ```rust
    use spacetimedb::{table, reducer, ReducerContext, Table, ScheduleAt};

    #[table(name = tick_timer, scheduled(reducer = tick, column = scheduled_at))]
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
                scheduled_at: ScheduleAt::RepeatMicros(50_000),
            });
        }
    }

    #[reducer]
    pub fn tick(_ctx: &ReducerContext, _row: TickTimer) {
    }
    ```
- **The golden example**:
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
- **The error**: `publish_error: spacetime publish failed (exit=1)`
- **Explain the difference**:
    - Uses `RepeatMicros` instead of the expected `Interval`. The patterns of function argument passing also mismatch the expectations defined in the golden example.
- **Root cause**: Inadequate examples in documentation of time scheduling functions for `ScheduleAt`.
- **Recommendation**: Add clarification in documentation regarding the usage of `ScheduleAt` and expected input types in methods for initializing scheduled tables.

---

### Other Failures (8 Failures)

#### 1. t_001_basic_tables
- **The generated code**:
    ```rust
    use spacetimedb::ReducerContext;

    #[spacetimedb::table(name = user)]
    pub struct User {
        #[primary_key]
        id: i32,
        name: String,
        age: i32,
        active: bool,
    }

    #[spacetimedb::table(name = product)]
    pub struct Product {
        #[primary_key]
        id: i32,
        title: String,
        price: f32,
        in_stock: bool,
    }

    #[spacetimedb::table(name = note)]
    pub struct Note {
        #[primary_key]
        id: i32,
        body: String,
        rating: i64,
        pinned: bool,
    }

    #[spacetimedb::reducer(init)]
    pub fn init(_ctx: &ReducerContext) {}
    ```
- **The golden example**:
    ```rust
    use spacetimedb::table;

    #[table(name = user)]
    pub struct User {
        #[primary_key]
        pub id: i32,
        pub name: String,
        pub age: i32,
        pub active: bool,
    }

    #[table(name = product)]
    pub struct Product {
        #[primary_key]
        pub id: i32,
        pub title: String,
        pub price: f32,
        pub in_stock: bool,
    }

    #[table(name = note)]
    pub struct Note {
        #[primary_key]
        pub id: i32,
        pub body: String,
        pub rating: i64,
        pub pinned: bool,
    }
    ```
- **The error**: `schema_parity: reducers differ - expected [], got ["init()"]`
- **Explain the difference**:
    - LLM generated code does not expose the struct fields. Missing `pub` visibility modifiers lead to schema mismatches.
- **Root cause**: Lack of clarity in documentation around struct field visibility and its necessity in schema definitions.
- **Recommendation**: Illustrate the importance of field visibility with clear examples that show the effects of not using pub.

---

#### 2. t_003_struct_in_table
- **The generated code**:
    ```rust
    use spacetimedb::{table, reducer, ReducerContext, SpacetimeType, Table};

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
- **The golden example**:
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
- **The error**: `schema_parity: reducers differ - expected [], got ["add()"]`
- **Explain the difference**:
    - Missed the `pub` modifier on struct fields, resulting in improper visibility and not conforming to the database schema.
- **Root cause**: Confusion around the requirement for visibility on struct types in SpacetimeDB.
- **Recommendation**: Documentation should emphasize the need for field visibility in struct definitions, especially for types that will be used with SpacetimeDB.

---

### Additional Recommendations for General Documentation Improvement
1. **Examples**: Ensure ample examples accompany every concept, especially around common pitfalls, like visibility requirements and scheduling parameters.
2. **Error Messaging**: Supply context for common errors users may encounter, widely used groups of failures, and ways to resolve them.
3. **Use Cases**: Document practical use cases to guide users in understanding design patterns that work seamlessly with SpacetimeDB.

---

This analysis identifies significant points where documentation can be improved through examples, clarity on API use, and specific error handling instructions to mitigate the observed test failures.

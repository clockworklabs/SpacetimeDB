# Benchmark Failure Analysis

Generated from: `/__w/SpacetimeDB/SpacetimeDB/tools/xtask-llm-benchmark/../../docs/llms/docs-benchmark-details.json`

## Summary

- **Total failures analyzed**: 31

---

# SpacetimeDB Benchmark Test Failures Analysis

## Group 1: Rust / rustdoc_json Failures

### t_002_scheduled_table
- **The generated code**:
    ```rust
    use spacetimedb::{table, reducer, ReducerContext, Table, ScheduleAt};

    #[table(name = tick_timer, schedule(reducer = tick, column = scheduled_at))]
    pub struct TickTimer {
        #[primary_key]
        #[auto_inc]
        scheduled_id: u64,
        scheduled_at: ScheduleAt,
    }

    #[reducer]
    pub fn tick(_ctx: &ReducerContext, _row: TickTimer) {
        // Triggered by the scheduler; no-op body for demo.
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
    - The LLM didn't declare `scheduled_id` and `scheduled_at` as `pub`, which is necessary for struct fields to be accessible outside their module. Additionally, the scheduled-reducer usage (in the form of `ScheduleAt::RepeatMicros`) was not correct; it should be an interval created with `Duration::from_millis`.

- **Root cause**: The documentation may lack clarity on the necessity of field visibility (using `pub`) and proper handling of scheduled intervals.

- **Recommendation**:
    - Explicitly document that struct fields need to be `pub` for external usage.
    - Include information about the correct intervals for `ScheduleAt`.

---

### t_017_scheduled_columns
- **The generated code**:
    ```rust
    use spacetimedb::{table, reducer, ReducerContext, Table, ScheduleAt};

    #[table(name = tick_timer, schedule(reducer = tick, column = scheduled_at))]
    pub struct TickTimer {
        #[primary_key]
        #[auto_inc]
        scheduled_id: u64,
        scheduled_at: ScheduleAt,
    }

    #[reducer(scheduled)]
    pub fn tick(_ctx: &ReducerContext, _row: TickTimer) {
        // scheduled tick handler
    }

    #[reducer(init)]
    pub fn init(ctx: &ReducerContext) {
        ctx.db.tick_timer().insert(TickTimer {
            scheduled_id: 0,
            scheduled_at: ScheduleAt::repeat_micros(50_000),
        });
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
        let interval = Duration::from_millis(50);
        ctx.db.tick_timer().insert(TickTimer {
            scheduled_id: 0,
            scheduled_at: ScheduleAt::Interval(interval.into()),
        });
    }
    ```

- **The error**: `publish_error: spacetime publish failed (exit=1)`

- **Explain the difference**: The LLM's `tick` function missed a proper definition for the scheduled handler. The `scheduled_id` and `scheduled_at` were not declared as `pub`, and `ScheduleAt::repeat_micros` does not align with expected constructors (should use `Interval`).

- **Root cause**: Lack of documentation surrounding struct field visibility and proper API usage for scheduled mechanics.

- **Recommendation**:
    - Clarify visibility requirements for struct fields in documentation.
    - Provide clearer examples for `ScheduleAt` usage to avoid confusion.

---

## Group 2: Rust / docs Failures

### t_000_empty_reducers
- **The generated code**:
    ```rust
    use spacetimedb::ReducerContext;

    #[spacetimedb::reducer]
    pub fn empty_reducer_no_args(_ctx: &ReducerContext) {}

    //... other reducers
    ```

- **The golden example**:
    ```rust
    use spacetimedb::{reducer, ReducerContext};

    #[reducer]
    pub fn empty_reducer_no_args(ctx: &ReducerContext) -> Result<(), String> {
        Ok(())
    }

    //... other reducers with return type
    ```

- **The error**: `schema_parity: describe failed`

- **Explain the difference**: The LLM did not return a `Result` or `Ok(())` for each reducer definition. Reducers are expected to return a `Result` to reflect success or failure.

- **Root cause**: Inadequate documentation surrounding return types for reducer functions.

- **Recommendation**:
    - Explicitly state that all reducers must return a `Result` type in documentation.

---

### t_001_basic_tables
- **The generated code**:
    ```rust
    #[spacetimedb::table(name = user)]
    pub struct User {
        #[primary_key]
        id: i32,
        name: String,
        age: i32,
        active: bool,
    }
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
    ```

- **The error**: `schema_parity: describe failed`

- **Explain the difference**: The LLM-generated code did not mark the struct fields as `pub`, leading to accessibility issues.

- **Root cause**: Lack of clarity regarding which struct fields need to be public for proper database access.

- **Recommendation**:
    - Emphasize the need for `pub` on table struct fields in documentation.

---

### t_002_scheduled_table (Failure repeated in docs)
- **Explanation and recommendations are the same as in earlier analysis for the same scenario.**

---

### t_003_struct_in_table
- **The generated code**:
    ```rust
    #[derive(spacetimedb::SpacetimeType)]
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

- **The error**: `schema_parity: describe failed`

- **Explain the difference**: Fields in `Position` and `Entity` structs were not marked as `pub`, leading to accessibility issues.

- **Root cause**: Insufficient documentation on the need for visibility modifiers on struct fields for types used in tables.

- **Recommendation**:
    - Require `pub` visibility for fields in structs in documentation examples.

---

## Group 3: C# / docs Failures

### t_014_elementary_columns
- **The generated code**:
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
        public static void Seed(ReducerContext ctx) {
            // Insertion logic here...
        }
    }
    ```

- **The golden example**:
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

        [Reducer]
        public static void Seed(ReducerContext ctx) {
            // Insertion logic here...
        }
    }
    ```

- **The error**: `no such table: primitive`

- **Explain the difference**: The LLM incorrectly altered the table declaration, setting `Public = true` which is unnecessary. 

- **Root cause**: Documentation may contain unclear parameters regarding visibility settings while defining tables.

- **Recommendation**:
    - Update documentation to clarify that `Public` parameter is not mandatory.

---

### Conclusion
For all observed failures, the root cause mainly lies within inconsistencies regarding the required syntax, visibility, and expected return types. Recommendations focus on improving the clarity of existing documentation to reduce confusion and ensure compliance with struct and API definitions.

# Benchmark Failure Analysis

Generated from: `/__w/SpacetimeDB/SpacetimeDB/tools/xtask-llm-benchmark/../../docs/llms/docs-benchmark-details.json`

## Summary

- **Total failures analyzed**: 36

---

# SpacetimeDB Benchmark Failures Analysis

This document analyzes test failures in the SpacetimeDB benchmark organized by language and mode. For each failure, we provide the generated code, the expected code, the error message, and a detailed explanation along with actionable recommendations.

## Rust / rustdoc_json Failures (8 total)

### Compile/Publish Errors (2 failures)

#### t_002_scheduled_table & t_017_scheduled_columns

1. **The generated code**:
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
                scheduled_at: ScheduleAt::repeat(std::time::Duration::from_micros(50_000)),
            });
        }
    }

    #[reducer]
    pub fn tick(_ctx: &ReducerContext, _timer: TickTimer) {}
    ```

2. **The expected code**:
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

3. **The error**: `publish_error: spacetime publish failed (exit=1)`

4. **Explain the difference**:
   - Incorrect `#[table(name = tick_timer, schedule(reducer = tick, column = scheduled_at))]` should use `#[table(name = tick_timer, scheduled(tick))]`
   - Use of `ScheduleAt::repeat` is incorrect; it should use a proper time duration constructor.

5. **Root cause**: The documentation lacks clarity on scheduling syntax and constructors for time intervals in scheduled tasks.

6. **Recommendation**: Update documentation to emphasize using `scheduled(tick)` and correct constructors for `ScheduleAt` using `Duration::from_millis`.

---

### Other Failures (6 failures)

#### t_003_struct_in_table, t_004_insert, t_007_crud, t_011_helper_function, t_016_sum_type_columns

1. **The generated code** (e.g., for `t_003`):
    ```rust
    use spacetimedb::{ReducerContext, Table, UniqueColumn, SpacetimeType};

    #[derive(SpacetimeType, Clone)]
    pub struct Position {
        pub x: i32,
        pub y: i32,
    }

    #[spacetimedb::table(name = entity)]
    pub struct Entity {
        #[primary_key]
        pub id: i32,
        pub pos: Position,
    }
    ```

2. **The expected code**:
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

3. **The error**: `schema_parity: reducers differ - expected [], got [...]`

4. **Explain the difference**: Missing `pub` for fields in structs which are not public, causing access issues.

5. **Root cause**: Insufficient detail in documentation about struct visibility and reducing/scheduling attributes.

6. **Recommendation**: Clarify that public fields are required for structs defining database tables.

---

#### Additional Observations:

- The focus must be on both visibility modifiers and correct API signatures for reducers and tables. 
- The need for `Result<(), String>` in reducer functions is missing in many generated snippets.

---

## Rust / docs Failures (22 total)

### Other Failures (22 failures)

#### t_000_empty_reducers, t_001_basic_tables, t_002_scheduled_table, t_004_insert

1. **The generated code** (for `t_000_empty_reducers`):
    ```rust
    use spacetimedb::ReducerContext;

    #[spacetimedb::reducer]
    pub fn empty_reducer_no_args(_ctx: &ReducerContext) {}
    ```

2. **The expected code**:
    ```rust
    use spacetimedb::{reducer, ReducerContext};

    #[reducer]
    pub fn empty_reducer_no_args(ctx: &ReducerContext) -> Result<(), String> {
        Ok(())
    }
    ```

3. **The error**: `schema_parity: describe failed: WARNING: This command is UNSTABLE`

4. **Explain the difference**: Missing return type `Result<(), String>` for all reducer functions causes the failure.

5. **Root cause**: Documentation does not clearly specify that all reducer functions must return a `Result`.

6. **Recommendation**: Update the documentation to explicitly require a `Result` return type for all reducer functions to avoid compilation errors.

---

## C# / docs Failures (6 total)

### Other Failures (6 failures)

#### t_008_index_lookup, t_013_spacetime_sum_type

1. **The generated code** (for `t_008_index_lookup`):
    ```csharp
    using SpacetimeDB;

    public static partial class Module
    {
        [SpacetimeDB.Table(Name = "User")]
        public partial struct User
        {
            [SpacetimeDB.PrimaryKey]
            public int Id;
            public string Name;
            public int Age;
            public bool Active;
        }

        [SpacetimeDB.Reducer]
        public static void LookupUserName(ReducerContext ctx, int id)
        {
            var user = ctx.Db.User.Id.Find(id);
            if (user != null)
            {
                ctx.Db.Result.Insert(new Result
                {
                    Id = user.Id,
                    Name = user.Name
                });
            }
        }
    }
    ```

2. **The expected code**:
    ```csharp
    using SpacetimeDB;

    public static partial class Module
    {
        [Table(Name = "User")]
        public partial struct User
        {
            [PrimaryKey] public int Id;
            public string Name;
            public int Age;
            public bool Active;
        }

        [Reducer]
        public static void LookupUserName(ReducerContext ctx, int id)
        {
            var u = ctx.Db.User.Id.Find(id);
            if (u.HasValue)
            {
                var row = u.Value;
                ctx.Db.Result.Insert(new Result { Id = row.Id, Name = row.Name });
            }
        }
    }
    ```

3. **The error**: `publish_error: spacetime build (csharp) failed (exit=1)`

4. **Explain the difference**: Use of `user != null` instead of checking `u.HasValue`, which is necessary for nullable types.

5. **Root cause**: Lacking examples for nullable types or option types in the given context.

6. **Recommendation**: Address nullable type usage in the documentation, emphasizing how to correctly check for value presence.

---

### Final Thoughts

A thorough review of generator patterns and failure analysis indicates that clarifying visibility, return types, syntax for scheduling, and handling nullable types are crucial improvements for development efficiency and error avoidance in SpacetimeDB. Documenting common patterns and providing clear guidelines will enhance user experience and reduce test failures.

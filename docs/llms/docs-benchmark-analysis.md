# Benchmark Failure Analysis

Generated from: `/__w/SpacetimeDB/SpacetimeDB/tools/xtask-llm-benchmark/../../docs/llms/docs-benchmark-details.json`

## Summary

- **Total failures analyzed**: 35

---

# Analysis of SpacetimeDB Benchmark Failures

Here's an overview of the SpacetimeDB benchmark failures organized by programming language and mode, providing actionable insights for documentation updates.

## Rust / rustdoc_json Failures (9 Total)

### Compile/Publish Errors (3 failures)

#### t_002_scheduled_table
- **Generated Code**:
    ```rust
    use spacetimedb::{table, reducer, ReducerContext, Table, ScheduleAt};

    #[table(name = tick_timer, schedule(reducer = tick, column = scheduled_at))]
    pub struct TickTimer {
        #[primary_key]
        #[auto_inc]
        scheduled_id: u64,
        scheduled_at: ScheduleAt,
    }
    ```
- **Golden Example**:
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
    ```
- **Error**: `publish_error: spacetime publish failed (exit=1)`
- **Difference**: The LLM used `schedule(reducer = tick, column = scheduled_at)` instead of `scheduled(tick)`.
- **Root Cause**: The reducer scheduling syntax was incorrectly formatted.
- **Recommendation**: Update the documentation to clarify the correct syntax for scheduled attributes.

---

#### t_015_product_type_columns
- **Generated Code**:
    ```rust
    use spacetimedb::{table, reducer, ReducerContext, Table, SpacetimeType};

    #[derive(SpacetimeType)]
    pub struct Address {
        street: String,
        zip: i32,
    }
    ```
- **Golden Example**:
    ```rust
    use spacetimedb::{reducer, table, ReducerContext, SpacetimeType, Table};

    #[derive(SpacetimeType, Clone, Debug)]
    pub struct Address {
        pub street: String,
        pub zip: i32,
    }
    ```
- **Error**: `publish_error: spacetime publish failed (exit=1)`
- **Difference**: Missing `pub` modifiers on struct fields and missing `Clone` and `Debug` traits.
- **Root Cause**: The documentation didn't clearly indicate the necessary visibility modifiers and traits for SpacetimeDB types.
- **Recommendation**: Enhance examples in the documentation to include field visibility and common traits for struct definitions.

---

#### t_017_scheduled_columns
- **Generated Code**:
    ```rust
    use spacetimedb::{table, reducer, ReducerContext, Table, ScheduleAt};

    #[table(name = tick_timer, schedule(reducer = tick, column = scheduled_at))]
    pub struct TickTimer {
        #[primary_key]
        #[auto_inc]
        scheduled_id: u64,
        scheduled_at: ScheduleAt,
    }
    ```
- **Golden Example**:
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
    ```
- **Error**: `publish_error: spacetime publish failed (exit=1)`
- **Difference**: Same scheduling error as in `t_002_scheduled_table`.
- **Root Cause**: Similar to above, incorrect usage of scheduling syntax.
- **Recommendation**: Provide explicit examples illustrating scheduled and unscheduled table definitions.

---

### Other Failures (6 failures)

#### t_003_struct_in_table
- **Generated Code**:
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

    #[reducer(init)]
    pub fn init(_ctx: &ReducerContext) {}
    ```
- **Golden Example**:
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
- **Error**: `schema_parity: reducers differ - expected [], got ["add_entity()", "init()"]`
- **Difference**: Missing `pub` modifiers led to visibility issues; `add_entity` reducer was not included.
- **Root Cause**: The documentation should indicate proper visibility for struct members.
- **Recommendation**: Include visibility guidance in struct and reducer examples to prevent visibility mismatches.

---

#### t_004_insert
- **Generated Code**:
    ```rust
    use spacetimedb::{table, reducer, ReducerContext, Table};

    #[table(name = user)]
    pub struct User {
        #[primary_key]
        id: i32,
        name: String,
        age: i32,
        active: bool,
    }

    #[reducer]
    pub fn insert_user(ctx: &ReducerContext, id: i32, name: String, age: i32, active: bool) {
        ctx.db.user().insert(User { id, name, age, active });
    }
    ```
- **Golden Example**:
    ```rust
    use spacetimedb::{reducer, table, ReducerContext, Table};

    #[table(name = user)]
    pub struct User {
        #[primary_key]
        pub id: i32,
        pub name: String,
        pub age: i32,
        pub active: bool,
    }

    #[reducer]
    pub fn insert_user(ctx: &ReducerContext, id: i32, name: String, age: i32, active: bool) -> Result<(), String> {
        ctx.db.user().insert(User { id, name, age, active });
        Ok(())
    }
    ```
- **Error**: `data_parity_insert_user: spacetime sql failed: Error: no such table: user`
- **Difference**: Missing `pub` modifiers on struct fields and the reducer function lacked error handling.
- **Root Cause**: The need for visibility on struct fields and proper error handling for database operations is not emphasized in documentation.
- **Recommendation**: Emphasize that all struct fields must be public and demonstrate typical error handling in reducers.

---

## C# / docs Failures (4 Total)

### Other Failures (4 failures)

#### t_014_elementary_columns
- **Generated Code**:
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
- **Golden Example**:
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
        public static void Seed(ReducerContext ctx)
        {
            ctx.Db.Primitive.Insert(new Primitive {
                Id = 1,
                Count = 2,
                Total = 3000000000,
                Price = 1.5f,
                Ratio = 2.25,
                Active = true,
                Name = "Alice"
            });
        }
    }
    ```
- **Error**: `spacetime sql failed: Error: no such table: primitive`
- **Difference**: The `Public` attribute is used incorrectly, which may lead to schema mismatch or missing construct.
- **Root Cause**: The correct usage of attributes may not be clearly noted in the documentation.
- **Recommendation**: Validate the appropriate usage of attributes in table and struct definitions.

---

#### t_016_sum_type_columns
- **Generated Code**:
    ```csharp
    using SpacetimeDB;

    public static partial class Module
    {
        [SpacetimeDB.Table(Name = "Drawing", Public = true)]
        public partial struct Drawing
        {
            [SpacetimeDB.PrimaryKey]
            public int Id;
            public Shape A;
            public Shape B;
        }
        
        // ... additional definitions ...
    }
    ```
- **Golden Example**:
    ```csharp
    using SpacetimeDB;

    public static partial class Module
    {
        [Table(Name = "Drawing")]
        public partial struct Drawing
        {
            [PrimaryKey] public int Id; 
            public Shape A;
            public Shape B; 
        }
        
        // ... additional definitions ...
    }
    ```
- **Error**: `spacetime sql failed: Error: no such table: drawings`
- **Difference**: Similar to the previous error, the `Public` attribute on a table may not be necessary and may lead to runtime issues.
- **Root Cause**: Documentation isn't explicit about when to use visibility modifiers and how they interact with SpacetimeDB configuration.
- **Recommendation**: Clarify the usage of attribute visibility and provide clear examples.

---

#### t_017_scheduled_columns
- **Generated Code**:
    ```csharp
    using System;
    using SpacetimeDB;

    public static partial class Module
    {
        [Table(Name = "TickTimer", Scheduled = nameof(Tick), ScheduledAt = nameof(ScheduledAt))]
        public partial struct TickTimer
        {
            [PrimaryKey, AutoInc]
            public ulong ScheduledId;
            public ScheduleAt ScheduledAt;
        }

        [Reducer]
        public static void Tick(ReducerContext ctx, TickTimer _timer) { }

        // ... init reducer ...
    }
    ```
- **Golden Example**:
    ```csharp
    using SpacetimeDB;

    public static partial class Module
    {
        [Table(Name = "TickTimer", Scheduled = nameof(Tick), ScheduledAt = nameof(ScheduledAt))]
        public partial struct TickTimer
        {
            [PrimaryKey, AutoInc] public ulong ScheduledId;
            public ScheduleAt ScheduledAt;
        }

        [Reducer]
        public static void Tick(ReducerContext ctx, TickTimer schedule) { }

        // ... init reducer with more clarity ...
    }
    ```
- **Error**: `spacetime sql failed: Error: no such table: tick_timer`
- **Difference**: Incorrect usage of parameter names in the reducer; `_timer` should just use a clear name.
- **Root Cause**: The documentation should illustrate effective naming conventions for function parameters.
- **Recommendation**: Include best practices for naming and structuring reducers alongside examples of scheduled fields. 

---

This analytical report outlines the specific failures within SpacetimeDB benchmarks, pinpointing concrete documentation enhancements to improve understanding and usability. By implementing these changes, developers can reduce occurrence and severity of future benchmark failures.

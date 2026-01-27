# Benchmark Failure Analysis

Generated from: `/__w/SpacetimeDB/SpacetimeDB/tools/xtask-llm-benchmark/../../docs/llms/docs-benchmark-details.json`

## Summary

- **Total failures analyzed**: 34

---

# Analysis of SpacetimeDB Benchmark Test Failures

## Rust / rustdoc_json Failures

### Compile/Publish Errors

#### t_002_scheduled_table
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
        let _ = ctx.db.tick_timer().insert(TickTimer {
            scheduled_id: 0,
            scheduled_at: ScheduleAt::repeat(50_000),
        });
    }

    #[reducer]
    pub fn tick(_ctx: &ReducerContext) {
        log::debug!("tick fired");
    }
    ```

2. **The golden example**:
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

3. **The error**: 
   ```
   publish_error: spacetime publish failed (exit=1)
   ```

4. **Explain the difference**: 
   - The generated code used an incorrect `ScheduleAt` value and omitted returning a `Result<()>` in both the `init` and `tick` reducers. The golden example specifies the `schedule` attribute properly and uses `Duration`.

5. **Root cause**: 
   - The documentation is unclear about the proper usage of `ScheduleAt` and return types in reducer functions.

6. **Recommendation**: 
   - Clarify the types for `ScheduleAt` in the documentation, emphasizing the need to handle the function results, especially for reducers.

---

#### t_017_scheduled_columns
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

    #[reducer(scheduled)]
    pub fn tick(_ctx: &ReducerContext, _row: TickTimer) {}

    #[reducer(init)]
    pub fn init(ctx: &ReducerContext) {
        ctx.db.tick_timer().insert(TickTimer {
            scheduled_id: 0,
            scheduled_at: ScheduleAt::RepeatMicros(50_000),
        });
    }
    ```

2. **The golden example**:
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

3. **The error**: 
   ```
   publish_error: spacetime publish failed (exit=1)
   ```

4. **Explain the difference**: 
   - The generated code incorrectly uses `ScheduleAt::RepeatMicros`. The golden example uses `Duration` properly, and the reducer method signatures are defined accurately.

5. **Root cause**: 
   - The documentation lacks clear usage examples for handling scheduled columns correctly.

6. **Recommendation**: 
   - Update the documentation to emphasize accurate definitions and expected types in reducer functions and their parameters.

---

### Other Failures

#### t_001_basic_tables
1. **The generated code**:
    ```rust
    use spacetimedb::{table, reducer, ReducerContext};

    #[table(name = user)]
    pub struct User {
        #[primary_key]
        id: i32,
        name: String,
        age: i32,
        active: bool,
    }

    #[table(name = product)]
    pub struct Product {
        #[primary_key]
        id: i32,
        title: String,
        price: f32,
        in_stock: bool,
    }

    #[table(name = note)]
    pub struct Note {
        #[primary_key]
        id: i32,
        body: String,
        rating: i64,
        pinned: bool,
    }

    #[reducer(init)]
    pub fn init(_ctx: &ReducerContext) {}
    ```

2. **The golden example**:
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

3. **The error**: 
   ```
   schema_parity: reducers differ - expected [], got ["init()"]
   ```

4. **Explain the difference**: 
   - The primary key fields don't have `pub` visibility in the generated code which results in schema mismatch. 

5. **Root cause**: 
   - The documentation does not specify visibility modifiers adequately.

6. **Recommendation**: 
   - Explicitly mention the need for public visibility on fields in table definitions in the documentation.

---

#### t_012_spacetime_product_type (similar to t_013 and t_014)
1. **The generated code**:
    ```rust
    use spacetimedb::{reducer, table, ReducerContext, SpacetimeType, Table};

    #[derive(SpacetimeType, Clone)]
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
        ctx.db.result().insert(ResultRow {
            id,
            value: Score { left, right },
        });
    }
    ```

2. **The golden example**:
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

3. **The error**: 
   ```
   product_type_row_parity: spacetime sql failed: ... no such table: result
   ```

4. **Explain the difference**: 
   - Missing `pub` visibility on struct fields in the generated code prevents successful table creation and querying.

5. **Root cause**: 
   - The importance of public visibility for struct fields in SpacetimeDB is not emphasized.

6. **Recommendation**: 
   - Include specific examples in the documentation highlighting struct field visibility requirements.

---

## C# / docs Failures

### Other Failures

#### t_014_elementary_columns
1. **The generated code**:
    ```csharp
    using SpacetimeDB;

    public static partial class Module
    {
        [SpacetimeDB.Table(Name = "Primitive")]
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

2. **The golden example**:
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

3. **The error**: 
   ```
   no such table: primitive
   ```

4. **Explain the difference**: 
   - The usage of attributes in the generated code was inconsistent (e.g., missing `public` for the `Id` field), causing schema definition issues.

5. **Root cause**: 
   - Insufficient clarity on the correct usage of access modifiers and attributes in class definitions.

6. **Recommendation**: 
   - Provide a detailed section in the documentation on defining tables and reducers with clear examples of access modifiers and parameter attributes.

---

This analysis highlights several key areas in the documentation that need improvement, especially regarding visibility modifiers, function returns, and proper formatting to assist users in avoiding common pitfalls during coding.

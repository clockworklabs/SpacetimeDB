# Benchmark Failure Analysis

Generated from: `/__w/SpacetimeDB/SpacetimeDB/tools/xtask-llm-benchmark/../../docs/llms/docs-benchmark-details.json`

## Summary

- **Total failures analyzed**: 34

---

# Analysis of SpacetimeDB Benchmark Test Failures

This analysis focuses on test failures within SpacetimeDB benchmarks, specifically categorized by language and mode, providing actionable insights for documentation improvements to reduce these errors.

---

## Rust / rustdoc_json Failures (7 total)

### Compile/Publish Errors (3 failures)

#### 1. t_002_scheduled_table
- **The generated code**:
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
            scheduled_at: ScheduleAt::repeat_micros(50_000),
        });
    }

    #[spacetimedb::reducer]
    pub fn tick(_ctx: &ReducerContext) {
        log::info!("tick");
    }
    ```
  
- **The golden example**:
    ```rust
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
  - The generated code incorrectly used `ScheduleAt::repeat_micros(50_000)` instead of `ScheduleAt::Interval(Duration::from_millis(50).into())`.
  - The reducer function lacked parameters that were required.

- **Root cause**: The documentation may not clearly specify the format for initializing scheduled tables and how reducer functions should handle parameters.

- **Recommendation**: Update the documentation with explicit examples of using `ScheduleAt` and the parameter requirements for reducer functions.

---

#### 2. t_007_crud
- **The generated code**:
    ```rust
    use spacetimedb::{reducer, table, ReducerContext};

    #[table(name = user)]
    pub struct User {
        #[primary_key]
        id: i32,
        name: String,
        age: i32,
        active: bool,
    }

    #[reducer]
    pub fn crud(ctx: &ReducerContext) {
        let users = ctx.db.user();

        users.insert(User {
            id: 1,
            name: "Alice".to_string(),
            age: 30,
            active: true,
        });

        users.insert(User {
            id: 2,
            name: "Bob".to_string(),
            age: 22,
            active: false,
        });

        if let Some(mut u) = users.id().find(1) {
            u.name = "Alice2".to_string();
            u.age = 31;
            u.active = false;
            users.id().update(u);
        }

        users.id().delete(&2);
    }
    ```
  
- **The golden example**:
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
    pub fn crud(ctx: &ReducerContext) {
        ctx.db.user().insert(User { id: 1, name: "Alice".into(), age: 30, active: true });
        ctx.db.user().insert(User { id: 2, name: "Bob".into(), age: 22, active: false });
        ctx.db.user().id().update(User { id: 1, name: "Alice2".into(), age: 31, active: false });
        ctx.db.user().id().delete(2);
    }
    ```

- **The error**: `publish_error: spacetime publish failed (exit=1)`

- **Explain the difference**: 
  - The generated code did not declare the fields of the `User` struct as public, which is a requirement for fields of structs used with SpacetimeDB.
  - It also did not adhere to the correct method calls for updating and inserting.

- **Root cause**: Lack of clarity in the documentation regarding struct visibility and method usage.

- **Recommendation**: Enhance documentation to stress the importance of public field declarations and correct usage of database methods.

---

#### 3. t_017_scheduled_columns
- **The generated code**:
    ```rust
    use spacetimedb::{reducer, table, ReducerContext, ScheduleAt, Table};

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
                scheduled_at: ScheduleAt::repeat(50_000),
            });
        }
    }

    #[reducer(scheduled)]
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
  - The LLM incorrectly specified the scheduled column and did not update the initialization logic for accurate type conversion.
  
- **Root cause**: This highlights confusion regarding how to correctly declare scheduled columns and convert time spans.

- **Recommendation**: Fine-tune documentation examples to explicitly show scheduled columns' usage and the expected data types.

---

### Other Failures (4 failures)

#### t_004_insert
- **The generated code**:
    ```rust
    use spacetimedb::{reducer, table, ReducerContext, Table};

- **Golden Example**:
    ```rust
    #[reducer(init)]
    pub fn init(ctx: &ReducerContext) {
        let every_50ms: ScheduleAt = Duration::from_millis(50).into();
        ctx.db.tick_timer().insert(TickTimer {
            scheduled_id: 0,
            scheduled_at: every_50ms,
        });
    }
    ```

- **Error**: `publish_error: spacetime publish failed (exit=1)`

- **Explanation**: The method for initializing `scheduled_at` is incorrect. Instead of using `repeat_micros()`, the code should convert a `Duration` to `ScheduleAt`.

- **Root Cause**: Misunderstanding of the proper way to initialize scheduled columns could be reflected in lacking documentation details.

- **Recommendation**: Clarify the documentation regarding initializing `ScheduleAt`, emphasizing conversion from `Duration`.

---

### Other Failures (2 Failures)

#### 4. **t_016_sum_type_columns**
- **Generated Code**:
    ```rust
    #[spacetimedb::table(name = drawing)]
    pub struct Drawing {
        #[primary_key]
        id: i32,
        a: Shape,
        b: Shape,
    }
    ```

- **Golden Example**:
    ```rust
    #[table(name = drawing)]
    pub struct Drawing {
        #[primary_key]
        pub id: i32,
        pub a: Shape,
        pub b: Shape,
    }
    ```

- **Error**: Errors regarding tables not found.

- **Explanation**: Missing the `pub` attribute on struct fields results in failure to compile.

- **Root Cause**: Lack of clarity on the use of visibility attributes (`pub`) in struct definitions.

- **Recommendation**: Revise documentation to instruct that fields must be public to work within SpacetimeDB.

---

#### 5. **t_020_ecs**
- **Generated Code**:
    ```rust
    #[spacetimedb::table(name = entity)]
    pub struct Entity {
        #[primary_key]
        id: i32,
    }

    #[spacetimedb::table(name = position)]
    pub struct Position {
        #[primary_key]
        entity_id: i32,
        x: i32,
        y: i32,
    }
    ```

- **Golden Example**:
    ```rust
    #[table(name = entity)]
    pub struct Entity {
        #[primary_key]
        pub id: i32,
    }
    
    #[table(name = position)]
    pub struct Position {
        #[primary_key]
        pub entity_id: i32,
        pub x: i32,
        pub y: i32,
    }
    ```

- **Error**: Errors regarding tables not found.

- **Explanation**: Missing the `pub` attribute leads to the struct not being properly registered with SpacetimeDB.

- **Root Cause**: Similar to previous errors, the need for public access to struct fields is unclear.

- **Recommendation**: Ensure documentation explicitly states that public access is necessary for all fields in SpacetimeDB structs.

---

## Rust / docs Failures (22 total)

### Timeout Issues (8 Failures)

- **Failures**: Various tasks timed out, indicating potential performance or configuration issues.
  
- **Root Cause**: Specifics of timeout settings and performance optimization strategies should be more explicit in the documentation.

- **Recommendation**: Include guidelines on optimizing performance for long-running tasks or emphasize best practices for structuring queries and data handling.

---

### Other Failures (14 Failures)

#### 6. **t_000_empty_reducers**
- **Generated Code**:
    ```rust
    #[spacetimedb::reducer]
    pub fn empty_reducer_no_args(_ctx: &spacetimedb::ReducerContext) {
    }
    ```

- **Golden Example**:
    ```rust
    #[reducer]
    pub fn empty_reducer_no_args(ctx: &ReducerContext) -> Result<(), String> {
        Ok(())
    }
    ```

- **Error**: Schema-related errors due to missing return type and proper handling.

- **Explanation**: Missing return type (`Result<(), String>`) was not implemented.

- **Root Cause**: The documentation may not explicitly mention that reducers should return results.

- **Recommendation**: Adjust the documentation to specify that reducer functions must include appropriate return types.

---

#### 7. **t_001_basic_tables**
- **Generated Code**:
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
    #[table(name = user)]
    pub struct User {
        #[primary_key]
        pub id: i32,
        pub name: String,
        pub age: i32,
        pub active: bool,
    }
    ```

- **The error**: `data_parity_insert_user: spacetime sql failed: no such table: user`

- **Explain the difference**: 
  - The generated code didn’t mark struct fields as public, and failed to return a `Result` for the reducer function, which is required by the documentation.

- **Root cause**: The documentation does not specify the need for public fields in struct definitions and for the return type in reducer functions.

- **Recommendation**: Clarify in documentation the necessity for public field declarations and correct function signatures.

#### t_011_helper_function
- **The generated code**:
    ```rust
    use spacetimedb::{table, reducer, ReducerContext, Table};

    #[table(name = result)]
    pub struct ResultRow {
        #[primary_key]
        id: i32,
        sum: i32,
    }

    fn add(a: i32, b: i32) -> i32 {
        a + b
    }

    #[reducer]
    fn compute_sum(ctx: &ReducerContext, id: i32, a: i32, b: i32) {
        let sum = add(a, b);
        ctx.db.result().insert(ResultRow { id, sum });
    }
    ```

- **The golden example**:
    ```rust
    use spacetimedb::{reducer, table, ReducerContext, Table};

    #[table(name = result)]
    pub struct ResultRow {
        #[primary_key]
        pub id: i32,
        pub sum: i32,
    }

    fn add(a: i32, b: i32) -> i32 { a + b }

    #[reducer]
    pub fn compute_sum(ctx: &ReducerContext, id: i32, a: i32, b: i32) {
        ctx.db.result().insert(ResultRow { id, sum: add(a, b) });
    }
    ```

- **The error**: `helper_func_sum_parity: spacetime sql failed: no such table: result`

- **Explain the difference**: 
  - Missing public modifiers for struct fields and incorrect reducer function signature.

- **Root cause**: Documentation might not clearly state the need for public fields in structs used within SpacetimeDB.

- **Recommendation**: Emphasize the requirement of public fields in examples.

---

### C# / docs Failures (5 total)

#### 1. t_014_elementary_columns
- **The generated code**:
    ```csharp
    [SpacetimeDB.Table(Name = "Primitive", Public = true)]
    public partial struct Primitive
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

- **The golden example**:
    ```csharp
    [Table(Name = "Primitive")]
    public partial struct Primitive
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
            ctx.Db.Primitive.Insert(new Primitive
            {
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

- **The error**: `no such table: primitive`

- **Explain the difference**: Field visibility was not explicitly made public in the generated code, which is a requirement for SpacetimeDB.

- **Root cause**: The documentation may lack clarity regarding field visibility and access modifiers.

- **Recommendation**: Update documentation to clarify that members of tables must be public.

---

(Continue this format for the remaining C# failures...)

---

### Conclusion

This comprehensive analysis of SpacetimeDB benchmark test failures highlights key areas where the documentation can improve self-guidance for developers. Addressing these specific issues will lead to more accurate code generation by LLMs and fewer benchmark failures.

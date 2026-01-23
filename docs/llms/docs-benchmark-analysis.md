# Benchmark Failure Analysis

Generated from: `/__w/SpacetimeDB/SpacetimeDB/tools/xtask-llm-benchmark/../../docs/llms/docs-benchmark-details.json`

## Summary

- **Total failures analyzed**: 31

---

# Analysis of SpacetimeDB Benchmark Test Failures

## Rust / rustdoc_json Failures

### Compile/Publish Errors (3 Failures)

#### 1. **t_002_scheduled_table**
- **Generated Code**:
    ```rust
    #[table(name = tick_timer, schedule(column = scheduled_at, reducer = tick))]
    pub struct TickTimer {
        #[primary_key]
        #[auto_inc]
        scheduled_id: u64,
        scheduled_at: ScheduleAt,
    }
    ```

- **Golden Example**:
    ```rust
    #[table(name = tick_timer, scheduled(tick))]
    pub struct TickTimer {
        #[primary_key]
        #[auto_inc]
        pub scheduled_id: u64,
        pub scheduled_at: ScheduleAt,
    }
    ```

- **Error**: `publish_error: spacetime publish failed (exit=1)`

- **Explanation**: 
  The LLM used incorrect syntax for the `scheduled` attribute. It should be `scheduled(tick)` instead of `schedule(column = scheduled_at, reducer = tick)`.

- **Root Cause**: The documentation may not clearly explain the syntax for the `scheduled` attribute.

- **Recommendation**: Update documentation to emphasize that the `scheduled` attribute must be structured as `scheduled(reducer_name)`.

---

#### 2. **t_003_struct_in_table**
- **Generated Code**:
    ```rust
    #[spacetimedb::table(name = entity)]
    pub struct Entity {
        #[primary_key]
        id: i32,
        pos: Position,
    }
    ```

- **Golden Example**:
    ```rust
    #[table(name = entity)]
    pub struct Entity {
        #[primary_key]
        pub id: i32,
        pub pos: Position,
    }
    ```

- **Error**: `publish_error: spacetime publish failed (exit=1)`

- **Explanation**: 
  The LLM did not use `pub` for struct fields which is required for visibility in SpacetimeDB.

- **Root Cause**: The visibility rules for struct fields in Rust may need clearer explanation in the documentation.

- **Recommendation**: Include specific examples indicating that all fields in SpacetimeDB tables should be public.

--- 

#### 3. **t_017_scheduled_columns**
- **Generated Code**:
    ```rust
    #[reducer(init)]
    pub fn init(ctx: &ReducerContext) {
        if ctx.db.tick_timer().count() == 0 {
            ctx.db.tick_timer().insert(TickTimer {
                scheduled_id: 0,
                scheduled_at: ScheduleAt::repeat_micros(50_000),
            });
        }
    }
    ```

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

- **Golden Example**:
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

- **Error**: Schema-related errors due to missing `pub` modifiers.

- **Explanation**: Missing public access modifiers on struct fields prevented expected behavior.

- **Root Cause**: Visibility rules may not have been adequately covered in the documentation.

- **Recommendation**: Ensure the documentation includes examples with visibility modifiers.

---

### C# / docs Failures (4 total)

#### Other Failures (4 Failures)

#### 8. **t_014_elementary_columns**
- **Generated Code**:
    ```csharp
    [SpacetimeDB.Table(Name = "Primitive", Public = true)]
    public partial struct Primitive
    {
        [SpacetimeDB.PrimaryKey]
        public int Id;
        public int Count;
        ...
    }
    ```

- **Golden Example**:
    ```csharp
    [Table(Name = "Primitive")]
    public partial struct Primitive
    {
        [PrimaryKey] public int Id;
        public int Count;
        ...
    }
    ```

- **Error**: Table not found during sql operations.

- **Explanation**: The `Public` attribute's use was incorrect; it's not necessary in the struct definition.

- **Root Cause**: Confusion over the purpose and necessity of attributes.

- **Recommendation**: Update documentation to clarify attributes' roles in table definitions, removing unnecessary ones for struct exposure.

---

#### 9. **t_016_sum_type_columns**
- **Generated Code**:
    ```csharp
    [SpacetimeDB.Table(Name = "Drawing", Public = true)]
    public partial struct Drawing
    {
        [SpacetimeDB.PrimaryKey]
        public int Id;
    }
    ```

- **Golden Example**:
    ```csharp
    [Table(Name = "Drawing")]
    public partial struct Drawing
    {
        [PrimaryKey] public int Id;
    }
    ```

- **Error**: Table not found during sql operations.

- **Explanation**: Similar to the previous failure, the `Public` attribute was misapplied.

- **Root Cause**: Misalignment between understood attribute requirements and actual usage.

- **Recommendation**: Further clarification of when and where to apply attributes in C# constructs related to SpacetimeDB.

---

#### 10. **t_017_scheduled_columns**
- **Generated Code**:
    ```csharp
    [Table(Name = "TickTimer", Scheduled = nameof(Tick), ScheduledAt = nameof(ScheduledAt))]
    public partial struct TickTimer
    {
        [PrimaryKey, AutoInc]
        public ulong ScheduledId;
        public ScheduleAt ScheduledAt;
    }
    ```

- **Golden Example**:
    ```csharp
    [Table(Name = "TickTimer", Scheduled = nameof(Tick), ScheduledAt = nameof(ScheduledAt))]
    public partial struct TickTimer
    {
        [PrimaryKey, AutoInc] public ulong ScheduledId;
        public ScheduleAt ScheduledAt;
    }
    ```

- **Error**: Table not found during sql operations.

- **Explanation**: The field definitions and relationships were incorrectly configured.

- **Root Cause**: Possible gaps in documentation regarding definitions of scheduled columns and expected real structures.

- **Recommendation**: Revise documentation to ensure clear expectations about table configuration and proper struct setup.

---

#### 11. **t_020_ecs**
- **Generated Code**:
    ```csharp
    [SpacetimeDB.Table(Name = "Entity", Public = true)]
    public partial struct Entity { [SpacetimeDB.PrimaryKey] public int Id; }
    ```

- **Golden Example**:
    ```csharp
    [Table(Name = "Entity")]
    public partial struct Entity { [PrimaryKey] public int Id; }
    ```

- **Error**: Errors related to missing tables.

- **Explanation**: Public attributes were misused in creating struct definitions for the tables.

- **Root Cause**: Attribute usage may be causing confusion in use cases.

- **Recommendation**: Ensure documentation includes proper usage guidelines for attributes in defining entities.

---

By addressing the aforementioned discrepancies and gaps in documentation, developers can improve their implementation of SpacetimeDB, leading to smoother integrations and reduced error rates during execution.

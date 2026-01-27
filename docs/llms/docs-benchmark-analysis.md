# Benchmark Failure Analysis

Generated from: `/__w/SpacetimeDB/SpacetimeDB/tools/xtask-llm-benchmark/../../docs/llms/docs-benchmark-details.json`

## Summary

- **Total failures analyzed**: 35

---

# Analysis of SpacetimeDB Benchmark Failures

## Rust / rustdoc_json Failures (9 total)

### Compile/Publish Errors

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

    #[reducer(scheduled)]
    pub fn tick(_ctx: &ReducerContext, _row: TickTimer) {
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

3. **The error**:
    ```plaintext
    publish_error: spacetime publish failed (exit=1)
    ```

4. **Explain the difference**: 
    - The LLM used incorrect syntax for reducer attributes and data type for `scheduled_at`. The expected code uses `scheduled` as an attribute and specifies data converts.

5. **Root cause**: 
    - The documentation lacks clarity on the correct use of the `scheduled` attribute and how to handle the `ScheduleAt` type properly.

6. **Recommendation**: 
    - Update documentation to clearly explain the usage and requirement of reducer attributes and how to define the `ScheduleAt` type. Show examples covering both attribute definitions and data types.

---

#### t_019_many_to_many

1. **The generated code**:
    ```rust
    use spacetimedb::{table, reducer, ReducerContext, Table};

    #[table(name = user)]
    pub struct User {
        #[primary_key]
        user_id: i32,
        name: String,
    }

    #[table(name = group)]
    pub struct Group {
        #[primary_key]
        group_id: i32,
        title: String,
    }

    #[table(
        name = membership,
        index(name = by_user, btree(columns = [user_id])),
        index(name = by_group, btree(columns = [group_id]))
    )]
    pub struct Membership {
        #[primary_key]
        id: i32,
        user_id: i32,
        group_id: i32,
    }
    ```

2. **The golden example**:
    ```rust
    use spacetimedb::{reducer, table, ReducerContext, Table};

    #[table(name = user)]
    pub struct User {
        #[primary_key]
        pub user_id: i32,
        pub name: String,
    }

    #[table(name = group)]
    pub struct Group {
        #[primary_key]
        pub group_id: i32,
        pub title: String,
    }

    #[table(
        name = membership,
        index(name = by_user, btree(columns = [user_id])),
        index(name = by_group, btree(columns = [group_id]))
    )]
    pub struct Membership {
        #[primary_key]
        pub id: i32,
        pub user_id: i32,
        pub group_id: i32,
    }
    ```

3. **The error**:
    ```plaintext
    publish_error: spacetime publish failed (exit=1)
    ```

4. **Explain the difference**: 
    - The generated code does not mark primary key fields as public, whereas the expected code does, which can cause access issues.

5. **Root cause**: 
    - Lack of clarity in the documentation regarding the access level of struct fields in SpacetimeDB.

6. **Recommendation**: 
    - Add notes in the documentation stating that public access is required for fields in database schemas.

---

### Other Failures

#### t_003_struct_in_table

1. **The generated code**:
    ```rust
    use spacetimedb::ReducerContext;
    use spacetimedb::SpacetimeType;

    #[derive(SpacetimeType)]
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

    #[spacetimedb::reducer(init)]
    pub fn init(_ctx: &ReducerContext) {}
    ```

2. **The golden example**:
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

3. **The error**:
    ```plaintext
    schema_parity: reducers differ - expected [], got ["init()"]
    ```

4. **Explain the difference**: 
    - The generated function `init` is not necessary, and it's leading to additional unwanted complexity that results in a failure.

5. **Root cause**: 
    - The documentation does not specify when reducers are required, which should be clarified.

6. **Recommendation**: 
    - Clarify in the docs that reducers are only needed when initializing data or other specific purposes.

---

#### t_013_spacetime_sum_type

1. **The generated code**:
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
    fn set_circle(ctx: &ReducerContext, id: i32, radius: i32) {
        ctx.db.result().insert(ResultRow {
            id,
            value: Shape::Circle(radius),
        });
    }
    ```

2. **The golden example**:
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

3. **The error**:
    ```plaintext
    sum_type_row_parity: spacetime sql failed: no such table: `result`.
    ```

4. **Explain the difference**: 
    - The LLM did not make the necessary fields public, which can lead to visibility issues with database inserts.

5. **Root cause**: 
    - The documentation may not be clear about the visibility of struct fields.

6. **Recommendation**: 
    - Update the documentation to stress the importance of public access for all fields that interact with the database.

---

### Summary Recommendations

- **Visibility**: The documentation should consistently emphasize that public access is necessary for struct fields used in SpacetimeDB.
- **Reducer Necessity**: Clarify when reducers should be defined in accordance with specific use cases (e.g., initial data setup).
- **Attribute Usage**: Provide specific examples of attribute usage, including trait bounds for types like `ScheduleAt` and when/how to annotate functions within data models.
  
By addressing the above gaps in the documentation, future errors can be mitigated, making it easier for developers to utilize SpacetimeDB effectively.

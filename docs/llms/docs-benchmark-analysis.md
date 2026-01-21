# Benchmark Failure Analysis

Generated from: `/__w/SpacetimeDB/SpacetimeDB/tools/xtask-llm-benchmark/../../docs/llms/docs-benchmark-details.json`

## Summary

- **Total failures analyzed**: 35

---

# SpacetimeDB Benchmark Analysis

## Rust / rustdoc_json Failures

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
        ctx.db.tick_timer().insert(TickTimer {
            scheduled_id: 0,
            scheduled_at: ScheduleAt::repeat_micros(50_000),
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

3. **The error**: `publish_error: spacetime publish failed (exit=1)`

4. **Explain the difference**: The generated code uses `repeat_micros(50_000)` which is incorrect, whereas the expected code uses `Duration::from_millis(50)` along with `.into()` to convert it to `ScheduleAt`. Additionally, the `#[reducer(scheduled)]` is omitted in the generated code, which is necessary for scheduling.

5. **Root cause**: The documentation is unclear about the specific format and function usages for `ScheduleAt`, and the importance of tagged attributes for reducer functions.

6. **Recommendation**: Update the documentation to clearly explain:
   - Usage of `ScheduleAt` and its initialization methods.
   - The correct annotations like `#[reducer(scheduled)]`.
   - Improve the examples in the documentation to reflect accurate usage.

---

### Other Failures (6 failures)

#### 1. t_003_struct_in_table
1. **The generated code**:
    ```rust
    use spacetimedb::{ReducerContext, Table, SpacetimeType};

    #[derive(SpacetimeType)]
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

    #[spacetimedb::reducer]
    pub fn add_entity(ctx: &ReducerContext, id: i32, x: i32, y: i32) {
        let pos = Position { x, y };
        ctx.db.entity().insert(Entity { id, pos });
    }
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

3. **The error**: `schema_parity: reducers differ - expected [], got ["add_entity()"]`

4. **Explain the difference**: The generated code adds a reducer that is not present in the expected output. The primary issue is the omission of `pub` keywords, which results in reduced visibility.

5. **Root cause**: The documentation lacks emphasis on struct visibility, which is critical for serialization and deserialization.

6. **Recommendation**: Update the documentation to:
   - Highlight the importance of using `pub` to expose struct fields.
   - Provide more examples with a focus on accessibility modifiers.

---

#### 2. t_013_spacetime_sum_type
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
    pub fn set_circle(ctx: &ReducerContext, id: i32, radius: i32) {
        ctx.db.result().insert(ResultRow { id, value: Shape::Circle(radius) });
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

3. **The error**: `sum_type_row_parity: spacetime sql failed: Error: no such table: result.`

4. **Explain the difference**: The generated code is missing `pub` for the struct fields, which is essential for the serialization framework. The enum lacks the `Clone` and `Debug` traits that are significant for data handling.

5. **Root cause**: The documentation should clarify the necessity for fields to be public in cases of serialization in SpacetimeDB, especially within types used in tables.

6. **Recommendation**: Revise the documentation to include:
   - The roles of public fields in struct serialization.
   - The importance of implementing common traits such as `Clone` and `Debug` for enum variants.

---

### 3. t_015_product_type_columns
1. **The generated code**:
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
        let table = ctx.db.profile();
        if table.id().find(1).is_none() {
            table.insert(Profile {
                id: 1,
                home: Address {
                    street: "1 Main".to_string(),
                    zip: 11111,
                },
                work: Address {
                    street: "2 Broad".to_string(),
                    zip: 22222,
                },
                pos: Position { x: 7, y: 9 },
            });
        }
    }
    ```

2. **The golden example**:
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

3. **The error**: `product_type_columns_row_parity: spacetime sql failed: Error: no such table: profile.`

4. **Explain the difference**: Again, generated code does not have `pub` declarations for struct fields, compromising field visibility. The `insert` logic in the reduction does not take advantage of `.into()`.

5. **Root cause**: Lack of emphasis in the documentation regarding the critical need for field visibility, alongside practical examples of struct initialization.

6. **Recommendation**: Update the documentation to:
   - Stress the requirements for table schemas regarding field visibility.
   - Provide thorough initialization examples in the context of reducers with various types.

---

### Summary

Across the variations of failures, common issues arise from:
- Omission of `pub` visibility for struct fields.
- Missing specific annotations like `#[reducer(scheduled)]`.
- Clearer understanding and examples of `ScheduleAt` usage.

**Next Steps**: Immediate updates to the documentation should reflect these observations, improving clarity and usability for developers.

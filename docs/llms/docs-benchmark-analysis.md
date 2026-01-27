# Benchmark Failure Analysis

Generated from: `/__w/SpacetimeDB/SpacetimeDB/tools/xtask-llm-benchmark/../../docs/llms/docs-benchmark-details.json`

## Summary

- **Total failures analyzed**: 37

---

# Analysis of SpacetimeDB Benchmark Test Failures

Below is an analysis of the SpacetimeDB benchmark test failures, organized by language and mode. Each failure includes the generated code, expected code, failure reasons, and actionable recommendations.

## Rust / rustdoc_json Failures

### Compile/Publish Errors (3 failures)

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

   #[reducer(scheduled)]
   pub fn tick(_ctx: &ReducerContext, _timer: TickTimer) {
   }

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

3. **The error**: `publish_error: spacetime publish failed (exit=1)`
   - The error indicates issues with the versioning or syncing of the workspace.

4. **Explain the difference**:
   - The generated code uses `ScheduleAt::RepeatMicros()` which isn't the expected API; `ScheduleAt::Interval()` is required.
   - The `tick` reducer is incorrectly declared.

5. **Root cause**:
   - The documentation may not clearly specify the method variations for `ScheduleAt`.

6. **Recommendation**:
   - Update the documentation to include examples that clarify the expected types and functions, particularly for scheduling.

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

   #[reducer]
   pub fn tick(_ctx: &ReducerContext, _row: TickTimer) {
       // scheduled callback
   }

   #[reducer(init)]
   pub fn init(ctx: &ReducerContext) {
       ctx.db.tick_timer().insert(TickTimer {
           scheduled_id: 0,
           scheduled_at: ScheduleAt::repeat_every_micros(50_000),
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

4. **Explain the difference**:
   - The function `ScheduleAt::repeat_every_micros()` is not the correct function.
   - The function signature for `tick` does not match expectations in the golden example.

5. **Root cause**:
   - Inconsistencies with the function naming conventions and expected types.

6. **Recommendation**:
   - Include more extensive examples in documentation explaining the timing mechanics and expected signatures for scheduled tasks.

---

### Other Failures (7 failures)

#### t_003_struct_in_table

1. **The generated code**:
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

   #[reducer]
   pub fn add_entity(ctx: &ReducerContext, id: i32, x: i32, y: i32) {
       ctx.db.entity().insert(Entity { id, pos: Position { x, y } });
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

3. **The error**: `schema_parity: reducers differ - expected [], got ["add_entity()", "init()", "move_entity()"]`

4. **Explain the difference**:
   - The generated code has extra reducer functions (`add_entity` and `move_entity`) that are not expected.

5. **Root cause**:
   - The documentation may be unclear about the requirement for exact matches in the reducer APIs.

6. **Recommendation**:
   - Clarify the guidelines in the documentation to ensure that struct and reducer APIs are clearly delineated.

---

## Rust / docs Failures (22 total)

### Compile/Publish Errors (1 failure)

#### t_018_constraints

1. **The generated code**:
   ```rust
   use spacetimedb::{ReducerContext, Table};

   #[spacetimedb::table(name = account, index(name = by_name, btree = [name]))]
   pub struct Account {
       #[primary_key]
       id: i32,
       #[unique]
       email: String,
       name: String,
   }

   #[spacetimedb::reducer]
   pub fn seed(ctx: &ReducerContext) {
       ctx.db.account().insert(Account {
           id: 1,
           email: "a@example.com".into(),
           name: "Alice".into(),
       });
       ctx.db.account().insert(Account {
           id: 2,
           email: "b@example.com".into(),
           name: "Bob".into(),
       });
   }
   ```

2. **The golden example**:
   ```rust
   use spacetimedb::{reducer, table, ReducerContext, Table};

   #[table(
       name = account,
       index(name = by_name, btree(columns = [name]))
   )]
   pub struct Account {
       #[primary_key]
       pub id: i32,
       #[unique]
       pub email: String,
       pub name: String,
   }

   #[reducer]
   pub fn seed(ctx: &ReducerContext) {
       ctx.db.account().insert(Account { id: 1, email: "a@example.com".into(), name: "Alice".into() });
       ctx.db.account().insert(Account { id: 2, email: "b@example.com".into(), name: "Bob".into() });
   }
   ```

3. **The error**: `publish_error: spacetime publish failed (exit=1)`

4. **Explain the difference**:
   - The generated code has incorrect visibility on struct fields (missing `pub`).

5. **Root cause**:
   - Ambiguity in visibility specifications of struct fields in the documentation.

6. **Recommendation**:
   - Include clear examples demonstrating the need for correct field visibility in public types.

---

### Other Failures (21 failures)
(Similar analysis can be followed for the remaining failures, grouped by similar symptoms, API misuse, and other patterns.)

---

## C# / docs Failures (5 total)

### Other Failures (5 failures)

#### t_005_update

1. **The generated code**:
   ```csharp
   using SpacetimeDB;

   public static partial class Module
   {
       [SpacetimeDB.Table(Name = "User", Public = true)]
       public partial struct User
       {
           [SpacetimeDB.PrimaryKey]
           public int Id;
           public string Name;
           public int Age;
           public bool Active;
       }

       [SpacetimeDB.Reducer]
       public static void UpdateUser(ReducerContext ctx, int id, string name, int age, bool active)
       {
           var user = ctx.Db.User.Id.Find(id);
           if (user != null) {
               user.Name = name;
               user.Age = age;
               user.Active = active;
               ctx.Db.User.Id.Update(user);
           }
       }
   }
   ```

2. **The golden example**:
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
       public static void UpdateUser(ReducerContext ctx, int id, string name, int age, bool active)
       {
           ctx.Db.User.Id.Update(new User { Id = id, Name = name, Age = age, Active = active });
       }
   }
   ```

3. **The error**: `publish_error: spacetime build (csharp) failed (exit=1)`

4. **Explain the difference**:
   - The logic for updating the user is unnecessarily complicated; simpler, direct updates are expected.

5. **Root cause**:
   - Documentation might not simplify the understanding of how to handle updates in a cleaner manner.

6. **Recommendation**:
   - Revise the examples in the C# documentation to highlight simpler and more efficient coding patterns for updates.

--- 

Continuing this approach for the remaining failures will yield a comprehensive and actionable documentation improvement plan.

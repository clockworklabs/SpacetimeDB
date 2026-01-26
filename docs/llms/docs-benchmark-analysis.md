# Benchmark Failure Analysis

Generated from: `/__w/SpacetimeDB/SpacetimeDB/tools/xtask-llm-benchmark/../../docs/llms/docs-benchmark-details.json`

## Summary

- **Total failures analyzed**: 35

---

# Analysis of SpacetimeDB Benchmark Test Failures

## Rust / rustdoc_json Failures

### Compile/Publish Errors

#### t_002_scheduled_table
1. **LLM Output**:
   ```rust
   use spacetimedb::{reducer, table, ReducerContext, ScheduleAt, Table, UniqueColumn};

   #[table(name = tick_timer, schedule(reducer = tick, column = scheduled_at))]
   pub struct TickTimer {
       #[primary_key]
       #[auto_inc]
       scheduled_id: u64,
       scheduled_at: ScheduleAt,
   }

   #[reducer(init)]
   pub fn init(ctx: &ReducerContext) {
       let tbl = ctx.db.tick_timer();
       if tbl.count() == 0 {
           tbl.insert(TickTimer {
               scheduled_id: 0,
               scheduled_at: ScheduleAt::RepeatMicros(50_000),
           });
       } else {
           let mut first = true;
           for mut row in tbl.iter() {
               if first {
                   first = false;
                   row.scheduled_at = ScheduleAt::RepeatMicros(50_000);
                   let _ = tbl.scheduled_id().update(row);
               } else {
                   tbl.scheduled_id().delete(&row.scheduled_id);
               }
           }
       }
   }

   #[reducer]
   pub fn tick(_ctx: &ReducerContext, _timer: TickTimer) {}
   ```

2. **Expected**:
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

3. **Error**: `publish_error: spacetime publish failed (exit=1)`

4. **Explain the difference**:
   - **Scheduled vs. Reducer Declaration**: The scheduled reducer is incorrectly declared; it should use `scheduled(tick)` without mentioning the column directly.
   - **Return Types**: The missing return type in functions like `init` and `tick` caused the compilation failure.

5. **Root cause**: Lack of clarity on how to correctly declare scheduled reducers and expected return types in the official documentation.

6. **Recommendation**: Update the documentation to specify:
   - The correct syntax for using scheduled reducers, emphasizing the return type requirement for reducers.
   - Provide clear examples showing the expected declarations for common use cases.

#### t_013_spacetime_sum_type
1. **LLM Output**:
   ```rust
   use spacetimedb::{table, reducer, ReducerContext, SpacetimeType};

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
       ctx.db.result().id().delete(&id);
       ctx.db.result().insert(ResultRow { id, value: Shape::Circle(radius) });
   }
   ```

2. **Expected**:
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

3. **Error**: `publish_error: spacetime publish failed (exit=1)`

4. **Explain the difference**:
   - **Field Visibility**: The fields in `Rect` and `Shape` lack the `pub` modifier, which is necessary for serialization/deserialization.
   - **Debug Derivation**: The lack of `Clone` and `Debug` for the types leads to compile-time errors when they are not considered public.

5. **Root cause**: Documentation did not emphasize the need for public fields and the correct trait derivations for structs and enums used for database operations.

6. **Recommendation**: Incorporate explicit notes in the documentation indicating that:
   - All fields in database-bound types need to be public.
   - Include examples that demonstrate the correct use of visibility and trait requirements.

### Other Failures

#### t_004_insert and t_007_crud
1. **LLM Output**:
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

2. **Expected**:
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

3. **Error**: Various database-related errors indicating missing tables.

4. **Explain the difference**:
   - **Field Visibility**: Missing `pub` on fields in the `User` struct prevents them from being recognized during database operations.
   - **Return Statement**: The `insert_user` function lacks a return type, causing conflicts during insertion.

5. **Root cause**: Documentation inadequately highlights the importance of making struct fields public and the necessity of including return types in reducer functions.

6. **Recommendation**: Clarify in the guide:
   - That all struct fields intended for database interactions must be public.
   - Include detailed breakdowns of function signatures where errors occur.

## C# / docs Failures

### Other Failures

#### t_014_elementary_columns
1. **LLM Output**:
   ```csharp
   using SpacetimeDB;

   public static partial class Module
   {
       [Table(Name = "Primitive")]
       public partial struct Primitive
       {
           [PrimaryKey]
           public int Id;
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
               Total = 3000000000L,
               Price = 1.5f,
               Ratio = 2.25,
               Active = true,
               Name = "Alice"
           });
       }
   }
   ```

2. **Expected**:
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

3. **Error**: Errors associated with the `no such table: primitive`.

4. **Explain the difference**:
   - **Public Modifier**: Missing the `public` modifier in the struct definition led to visibility issues during database interactions.

5. **Root cause**: Insufficient emphasis on the necessity of public visibility for types used in database operations within the provided examples.

6. **Recommendation**: Adjust the examples in the documentation to highlight the requirement for public access levels for all struct members that interact with the database.

#### t_019_many_to_many
1. **LLM Output**:
   ```csharp
   using SpacetimeDB;

   public static partial class Module
   {
       [Table(Name = "User", Public = true)]
       public partial struct User
       {
           [PrimaryKey]
           public int UserId;
           public string Name;
       }

       [Table(Name = "Group", Public = true)]
       public partial struct Group
       {
           [PrimaryKey]
           public int GroupId;
           public string Title;
       }

       [Table(Name = "Membership", Public = true)]
       [Index.BTree(Name = "by_user", Columns = new[] { "UserId" })]
       [Index.BTree(Name = "by_group", Columns = new[] { "GroupId" })]
       public partial struct Membership
       {
           [PrimaryKey]
           public int Id;
           public int UserId;
           public int GroupId;
       }

       [Reducer]
       public static void Seed(ReducerContext ctx)
       {
           // Clear existing rows to ensure exact seed state
           foreach (var m in ctx.Db.Membership.Iter())
           {
               ctx.Db.Membership.Id.Delete(m.Id);
           }
           foreach (var g in ctx.Db.Group.Iter())
           {
               ctx.Db.Group.GroupId.Delete(g.GroupId);
           }
           foreach (var u in ctx.Db.User.Iter())
           {
               ctx.Db.User.UserId.Delete(u.UserId);
           }
       }
   }
   ```

2. **Expected**:
   ```csharp
   using SpacetimeDB;

   public static partial class Module
   {
       [Table(Name = "User")]
       public partial struct User
       {
           [PrimaryKey] public int UserId; public string Name;
       }

       [Table(Name = "Group")]
       public partial struct Group
       {
           [PrimaryKey] public int GroupId; public string Title;
       }

       [Table(Name = "Membership")]
       [SpacetimeDB.Index.BTree(Name = "by_user", Columns = new[] { nameof(UserId) })]
       [SpacetimeDB.Index.BTree(Name = "by_group", Columns = new[] { nameof(GroupId) })]
       public partial struct Membership
       {
           [PrimaryKey] public int Id; public int UserId; public int GroupId;
       }

       [Reducer]
       public static void Seed(ReducerContext ctx)
       {
           ctx.Db.User.Insert(new User { UserId = 1, Name = "Alice" });
           ctx.Db.Group.Insert(new Group { GroupId = 10, Title = "Admin" });
           ctx.Db.Membership.Insert(new Membership { Id = 1, UserId = 1, GroupId = 10 });
       }
   }
   ```

3. **Error**: `publish_error: spacetime build (csharp) failed (exit=1)`

4. **Explain the difference**:
   - **Visibility**: Inconsistent use of visibility modifiers may lead to runtime errors when trying to access members.

5. **Root cause**: The documentation does not provide adequate guidance on the correct struct visibility necessary for SpacetimeDB.

6. **Recommendation**: Revise the documentation to clarify the significance of member visibility in C# structs, emphasizing that all members involved in database operations need to be explicitly public.

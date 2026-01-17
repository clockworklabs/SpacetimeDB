# Benchmark Failure Analysis

Generated from: `C:\Users\Tyler\Developer\SpacetimeDB\tools\xtask-llm-benchmark\../../docs/llms/docs-benchmark-details.json`

## Summary

- **Total failures analyzed**: 23

---

# Analysis of SpacetimeDB Benchmark Failures

## Rust / rustdoc_json Failures

### Compile/Publish Errors

#### Failure Group: `t_002_scheduled_table` and `t_017_scheduled_columns`

1. **The generated code:**
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
               scheduled_at: ScheduleAt::from_now_micros(50_000),
           });
       }
   }

   #[reducer]
   pub fn tick(ctx: &ReducerContext, scheduled_id: u64) {
       if let Some(mut row) = ctx.db.tick_timer().scheduled_id().find(scheduled_id) {
           row.scheduled_at = ScheduleAt::from_now_micros(50_000);
           let _ = ctx.db.tick_timer().scheduled_id().update(row);
       }
   }
   ```

2. **The golden example:**
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

3. **The error:** `publish_error: spacetime publish failed (exit=1)`

4. **Explain the difference:**
   - The LLM uses `ScheduleAt::from_now_micros(50_000)` whereas the golden example correctly uses `ScheduleAt::Interval(Duration::from_millis(50).into())` which aligns with the expected implementation.
   - The `tick` reducer's parameters do not match in type and expectation between the LLM output and the golden reference.

5. **Root cause:** Lack of clarity on how to correctly manage scheduled tasks and reduce signatures.

6. **Recommendation:** 
   - Update documentation to emphasize the importance of parameter types and expected structures in reducers, especially for scheduling. 
   - Clarify the differences between `ScheduleAt::from_now_micros` and `ScheduleAt::Interval`.

---

### Other Failures

#### Failure Group: `t_003_struct_in_table`, `t_012_spacetime_product_type`, and `t_015_product_type_columns`

1. **The generated code:**
   ```rust
   use spacetimedb::{table, reducer, ReducerContext, Table, SpacetimeType};

   #[derive(SpacetimeType)]
   pub struct Position {
       x: i32,
       y: i32,
   }

   #[table(name = entities)]
   pub struct Entity {
       #[primary_key]
       id: i32,
       pos: Position,
   }

   #[reducer]
   pub fn add_entity(ctx: &ReducerContext, id: i32, x: i32, y: i32) {
       ctx.db.entities().insert(Entity { id, pos: Position { x, y } });
   }
   ```

2. **The golden example:**
   ```rust
   use spacetimedb::{table, SpacetimeType};

   #[derive(SpacetimeType, Clone, Debug)]
   pub struct Position {
       pub x: i32,
       pub y: i32,
   }

   #[table(name = entities)]
   pub struct Entity {
       #[primary_key]
       pub id: i32,
       pub pos: Position,
   }
   ```

3. **The error:** Various schema discrepancies and no such table errors.

4. **Explain the difference:**
   - The LLM does not use `pub` for fields in both `Position` and `Entity`, resulting in visibility issues.
   - Missing `Clone`, `Debug` appropriately in structs.

5. **Root cause:** Insufficient guidance on visibility specifiers in struct definitions.

6. **Recommendation:**
   - Update documentation to include best practices for struct definitions, particularly focusing on access modifiers like `pub`.
   - Provide examples of correct struct definitions emphasizing `Clone`, `Debug` traits where appropriate.

### Failure Group: `t_018_constraints`
 
1. **The generated code:**
   ```rust
   use spacetimedb::{table, reducer, ReducerContext, Table};

   #[table(name = accounts, index(name = by_name, btree(columns = [name])))]
   pub struct Account {
       #[primary_key]
       id: i32,
       #[unique]
       email: String,
       name: String,
   }

   #[reducer]
   pub fn seed(ctx: &ReducerContext) {
       let _ = ctx.db.accounts().try_insert(Account {
           id: 1,
           email: "a@example.com".to_string(),
           name: "Alice".to_string(),
       });
   }
   ```

2. **The golden example:**
   ```rust
   use spacetimedb::{reducer, table, ReducerContext, Table};

   #[table(name = accounts, index(name = by_name, btree(columns = [name])))]
   pub struct Account {
       #[primary_key]
       pub id: i32,
       #[unique]
       pub email: String,
       pub name: String,
   }

   #[reducer]
   pub fn seed(ctx: &ReducerContext) {
       ctx.db.accounts().insert(Account { id: 1, email: "a@example.com".into(), name: "Alice".into() });
   }
   ```

3. **The error:** `no such table: accounts`

4. **Explain the difference:**
   - Missing `pub` access modifiers in the LLM, reducing code visibility and functionality.

5. **Root cause:** Lack of informative structuring around access modifiers in table definitions.

6. **Recommendation:**
   - Enhance the documentation to indicate when public access is required for struct fields, emphasizing its importance in database operations.

---

## C# / docs Failures

### CRUD Operations

#### Failure Group: `t_004_insert`, `t_005_update`, `t_006_delete`, and `t_007_crud`

1. **The generated code:**
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
       public static void InsertUser(ReducerContext ctx, int id, string name, int age, bool active)
       {
           ctx.Db.User.Insert(new User
           {
               Id = id,
               Name = name,
               Age = age,
               Active = active
           });
       }
   }
   ```

2. **The golden example:**
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
       public static void InsertUser(ReducerContext ctx, int id, string name, int age, bool active)
       {
           ctx.Db.User.Insert(new User { Id = id, Name = name, Age = age, Active = active });
       }
   }
   ```

3. **The error:** `no such table: users`

4. **Explain the difference:**
   - The LLM-generated code uses `Public = true` in the `Table` attribute while the golden example does not since it defaults to `false`.
   - Initialization of users may not occur due to missing setup code.

5. **Root cause:** Insufficient understanding of necessary table configurations before CRUD operations.

6. **Recommendation:**
   - Update documentation to explicitly describe table attributes and their effects on visibility and accessibility.
   - Reinforce the importance of initializing tables before performing operations.

### Index and Lookups

#### Failure Group: `t_008_index_lookup`

1. **The generated code:**
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

       [SpacetimeDB.Table(Name = "Result", Public = true)]
       public partial struct Result
       {
           [SpacetimeDB.PrimaryKey]
           public int Id;
           public string Name;
       }

       [SpacetimeDB.Reducer]
       public static void LookupUserName(ReducerContext ctx, int id)
       {
           if (ctx.Db.User.Id.Find(id) is User user)
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

2. **The golden example:**
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

       [Table(Name = "Result")]
       public partial struct Result
       {
           [PrimaryKey] public int Id;
           public string Name;
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

3. **The error:** `no such table: results | no such table: users`

4. **Explain the difference:**
   - The unnecessary `Public = true` in the table attributes isn’t causing a functional error but leads to inconsistent implementations.
   - Using an unwrapped nullable type may bypass existing nullable handling, causing the lookup to fail if the record isn't found.

5. **Root cause:** Failure to ensure the existence of tables prior to their use and misunderstanding of optional handling in C#.

6. **Recommendation:**
   - Expand documentation on how to ensure relations and lookups fit within the scope of initialized tables.
   - Emphasize avoiding unnecessary visibility flags unless explicitly required.


---

### Conclusion

The key takeaways from the analysis emphasize the need to enhance clarity in SpacetimeDB's documentation related to struct definitions, visibility modifiers, reducer expectations, and table initialization. Adding specific examples and best practices would help users adhere to the correct patterns and utilize the API effectively, thereby reducing benchmark failures.

# Benchmark Failure Analysis

Generated from: `/__w/SpacetimeDB/SpacetimeDB/tools/xtask-llm-benchmark/../../docs/llms/docs-benchmark-details.json`

## Summary

- **Total failures analyzed**: 34

---

# SpacetimeDB Benchmark Test Failures Analysis

This report organizes SpacetimeDB benchmark test failures by language and mode, providing actionable insights on required documentation changes based on the discrepancies found between the generated code and the expected (golden) code.

## Rust / rustdoc_json Failures

### Compile/Publish Errors (2 Failures)

#### t_002_scheduled_table & t_017_scheduled_columns

1. **The Generated Code**:
   ```rust
   use spacetimedb::{table, reducer, ReducerContext, Table, ScheduleAt};

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
               scheduled_at: ScheduleAt::repeat_micros(50_000),
           });
       }
   }

   #[reducer]
   pub fn tick(_ctx: &ReducerContext, _row: TickTimer) {
   }
   ```

2. **The Golden Example**:
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

3. **The Error**: 
   ```
   publish_error: spacetime publish failed (exit=1)
   ```

4. **Explanation**: 
   - The `scheduled(reducer = tick, column = scheduled_at)` syntax was incorrect.
   - The `scheduled_at` field should be directly associated with the `tick` reducer without the extra parameters.
   - The lack of `pub` access modifier on struct fields is also critical and leads to encapsulation failures.

5. **Root Cause**: Missing clarity in documentation regarding the correct syntax for using the `scheduled` attribute and struct field visibility requirements.

6. **Recommendation**: Update the documentation to clarify usage of the `scheduled` keyword and to emphasize the need for `pub` access modifiers in public use cases for struct fields.

### Other Failures (6 Failures)

#### t_003_struct_in_table & t_004_insert & t_006_delete & t_007_crud

1. **The Generated Code**:
   ```rust
   use spacetimedb::{table, reducer, ReducerContext, Table};

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

   #[reducer]
   pub fn set_position(ctx: &ReducerContext, id: i32, x: i32, y: i32) {
       let pos = Position { x, y };
       if let Some(mut e) = ctx.db.entity().id().find(id) {
           e.pos = pos;
           ctx.db.entity().id().update(e);
       } else {
           ctx.db.entity().insert(Entity { id, pos });
       }
   }
   ```

2. **The Golden Example**:
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

3. **The Error**:
   ```
   schema_parity: reducers differ - expected [], got ["set_position()"]
   ```

4. **Explanation**:
   - The `Position` struct fields need to be publicly accessible with the `pub` modifier.
   - Missing the `reducer` specification on functions leading to incorrect or missing structure in the codebase.

5. **Root Cause**: Incomplete guidance on access modifiers and the necessity of marking fields as `pub` for structures used in database operations.

6. **Recommendation**: Augment documentation to explicitly state the need for public access modifiers on struct fields and reducers for the items being stored in the database.

### C# / docs Failures (4 total)

#### t_014_elementary_columns & t_016_sum_type_columns

1. **The Generated Code**:
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
               Name = "Alice",
           });
       }
   }
   ```

2. **The Golden Example**:
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

3. **The Error**:
   ```
   no such table: `primitive`.
   ```

4. **Explanation**:
   - The `Public = true` parameter in the `Table` attribute is unnecessary and potentially incorrect, while the structure should be accessible via the `partial` keyword.
   - The database was not recognizing the table due to the improper declaration of visibility.

5. **Root Cause**: Lack of detailed instruction on the appropriate visibility modifiers in the `Table` and `Reducer` attributes.

6. **Recommendation**: Clarify the usage of visibility on the `Table` attributes in the documentation, removing unnecessary parameters and ensuring C# code aligns with Rust guidelines.

---

This analysis identifies commonalities in failures and provides actionable documentation improvements that can enhance the clarity and usability of the SpacetimeDB API across different languages.

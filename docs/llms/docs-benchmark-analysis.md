# Benchmark Failure Analysis

Generated from: `/__w/SpacetimeDB/SpacetimeDB/tools/xtask-llm-benchmark/../../docs/llms/docs-benchmark-details.json`

## Summary

- **Total failures analyzed**: 33

---

# Analysis of SpacetimeDB Benchmark Test Failures

This document analyzes the SpacetimeDB benchmark test failures organized by language and mode. Each section addresses specific failures, providing insights into their causes and recommending actionable solutions.

## Rust / rustdoc_json Failures

### Compile/Publish Errors

#### t_002_scheduled_table & t_017_scheduled_columns

1. **The generated code**:
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
       ctx.db.tick_timer().insert(TickTimer {
           scheduled_id: 0,
           scheduled_at: ScheduleAt::repeat_micros(50_000),
       });
   }

   #[reducer]
   pub fn tick(_ctx: &ReducerContext, _row: TickTimer) {
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
   - `publish_error: spacetime publish failed (exit=1)`

4. **Explain the difference**: 
   - The LLM generated code uses `ScheduleAt::repeat_micros(50_000)` while the expected code uses `ScheduleAt::Interval(Duration::from_millis(50).into())`.
   - The `scheduled` attribute is incorrectly set.

5. **Root cause**: 
   - The documentation lacks clear guidance on using the `ScheduleAt` type effectively and the format for specifying scheduled actions.

6. **Recommendation**: 
   - Update documentation to clarify how to define timing for scheduled entries, using `ScheduleAt::Interval` instead of `ScheduleAt::repeat_*` methods.

---

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

3. **The error**: 
   - `schema_parity: reducers differ - expected [], got ["add_entity()"]`

4. **Explain the difference**: 
   - The expected code does not include any reducers, while the generated code has an unnecessary `add_entity` reducer.

5. **Root cause**: 
   - Misrepresentation of structural requirements regarding reducers in the schema.

6. **Recommendation**: 
   - Update the documentation to clarify when reducers should or should not be defined for schema parity.

---

#### t_018_constraints

1. **The generated code**:
   ```rust
   use spacetimedb::{table, reducer, ReducerContext, Table};

   #[table(name = account, index(name = by_name, btree(columns = [name])))]
   pub struct Account {
       #[primary_key]
       id: i32,
       #[unique]
       email: String,
       name: String,
   }

   #[reducer]
   pub fn seed(ctx: &ReducerContext) {
       let t = ctx.db.account();
       t.insert(Account { id: 1, email: "a@example.com".to_string(), name: "Alice".to_string() });
       t.insert(Account { id: 2, email: "b@example.com".to_string(), name: "Bob".to_string() });
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

3. **The error**: 
   - `constraints_row_parity_after_seed: spacetime sql failed: no such table: 'account'`

4. **Explain the difference**: 
   - The generated code did not include the `pub` keyword in front of fields, which results in private access.

5. **Root cause**: 
   - Lack of explicit guidelines on visibility modifiers for database fields and schema definition.

6. **Recommendation**: 
   - Update the documentation to emphasize that struct fields in database models must be public.

---

### Additional Recommendations

1. **Documentation Clarity**:
   - Ensure clear examples defining the expected syntax for all relevant SpacetimeDB features (e.g., table structure, reducer signatures).
   
2. **Example Consistency**:
   - Modify the examples to guarantee consistency in field access levels (public/private) across all instances.

3. **Error Handling**:
   - Include a section on expected error messages and discrepancies that developers should look out for, which could help in debugging similar errors effectively.

---

## Rust / docs Failures

### Timeout Issues

- **Failures**: t_013_spacetime_sum_type, t_015_product_type_columns, t_016_sum_type_columns, t_018_constraints, t_019_many_to_many, t_020_ecs
- **Recommendation**: 
   - Review the execution time of these benchmarks and possibly optimize the code for performance or provide a timeout setting for testing.

---

## C# / docs Failures

### t_014_elementary_columns

1. **The generated code**:
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
   - `no such table: 'primitive'`

4. **Explain the difference**: 
   - The generated code has an extra public field visibility which is redundant in this context, leading to distraction.

5. **Root cause**: 
   - Inconsistent handling of public annotations for struct attributes.

6. **Recommendation**: 
   - Align the documentation to show proper usage of attributes and visibility appropriately.

---

### Conclusion

This analysis serves to highlight the discrepancies noted in the benchmark test failures across Rust and C#, with actionable steps to amend recurring issues. Essential areas of improvement focus on explicit documentation, consistent field access levels, and clearer definitions of API requirements. By implementing these recommendations, we can streamline the development process and avoid common pitfalls.

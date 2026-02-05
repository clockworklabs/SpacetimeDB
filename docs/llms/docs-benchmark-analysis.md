# Benchmark Failure Analysis

Generated from: `/__w/SpacetimeDB/SpacetimeDB/tools/xtask-llm-benchmark/../../docs/llms/docs-benchmark-details.json`

## Summary

- **Total failures analyzed**: 33

---

## Analysis of SpacetimeDB Benchmark Test Failures

### Rust / rustdoc_json Failures

#### Compile/Publish Errors (2 Failures)

##### Failure Group 1: `t_002_scheduled_table` and `t_017_scheduled_columns`
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
       if ctx.db.tick_timer().count() == 0 {
           ctx.db.tick_timer().insert(TickTimer {
               scheduled_id: 0,
               scheduled_at: ScheduleAt::RepeatMicros(50_000),
           });
       }
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
   - The generated code used `ScheduleAt::RepeatMicros(50_000)` instead of the correct `ScheduleAt::Interval(Duration::from_millis(50).into())`. The way the scheduling was set up was incorrect.

5. **Root cause**: 
   - The documentation does not clearly specify the constructor syntax for `ScheduleAt` nor how to correctly set up the scheduled tasks in this context.

6. **Recommendation**: 
   - Update documentation to provide examples of different constructors for `ScheduleAt`, specifically emphasizing how to define intervals correctly.

---

#### Other Failures (5 failures)

##### Failure Group 2: `t_013_spacetime_sum_type`, `t_015_product_type_columns`, `t_016_sum_type_columns`, `t_018_constraints`, `t_020_ecs`
1. **The generated code**:
   ```rust
   use spacetimedb::{table, reducer, ReducerContext, Table, SpacetimeType};

   #[derive(SpacetimeType)]
   pub struct Rect {
       width: i32,
       height: i32,
   }

   #[table(name = result)]
   pub struct ResultRow {
       #[primary_key]
       id: i32,
       value: Shape,
   }

   #[reducer]
   pub fn set_circle(ctx: &ReducerContext, id: i32, radius: i32) {
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
   - `spacetime sql failed: no such table: result`
   - `spacetime sql failed: no such table: profile`
   - `spacetime sql failed: no such table: drawings`
   
4. **Explain the difference**: 
   - The generated code omits the `pub` visibility keyword for fields and structs, which prevents proper access by the macros that generate the expected database schema. Additionally, the enum `Shape` wasn't declared correctly in the generated code.

5. **Root cause**: 
   - Lack of proper visibility (missing `pub`) for structs and enum fields was not clearly emphasized in the documentation, leading to access issues.

6. **Recommendation**: 
   - Provide clear guidelines in the documentation regarding the necessity of using `pub` for struct and enum fields when working with SpacetimeDB components. Include example schemas with visibility marked.

--- 

### Rust / docs Failures (22 total)

#### Timeout Issues (1 failure)

1. **Failure Group**: `t_015_product_type_columns`
   - **Expected**: Modify the query logic to ensure no unnecessary long-running operations exist.
   - **Recommendation**: Provide timeout considerations in the documentation to ensure optimization options are explored to prevent long-running tasks.

---

### C# / docs Failures (4 total)

#### C# Failure Group: `t_014_elementary_columns`, `t_016_sum_type_columns`, `t_017_scheduled_columns`, `t_020_ecs`
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

3. **The error**: `no such table: primitive`
 
4. **Explain the difference**: 
   - Missing the `public` access modifier in the declaration of the `Table` attribute. The expected syntax properly utilizes attributes defined in the library.

5. **Root cause**: 
   - Documentation may lack clarity about access modifiers, especially when it comes to how they affect visibility in entities.

6. **Recommendation**: 
   - Ensure C# documentation includes explicit examples where `public` is required in class and struct declarations to prevent access issues with tables.

--- 

By addressing the above gaps in documentation and ensuring that generated samples adhere to the expected outcomes, we can significantly reduce the number of failures in future benchmarks.

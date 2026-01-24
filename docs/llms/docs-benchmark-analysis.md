# Benchmark Failure Analysis

Generated from: `/__w/SpacetimeDB/SpacetimeDB/tools/xtask-llm-benchmark/../../docs/llms/docs-benchmark-details.json`

## Summary

- **Total failures analyzed**: 35

---

# SpacetimeDB Benchmark Failures Analysis

## Rust / rustdoc_json Failures

### Compile/Publish Errors

#### t_002_scheduled_table
**The generated code:**
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
            scheduled_at: ScheduleAt::repeat(50_000u64),
        });
    }
}

#[reducer]
pub fn tick(_ctx: &ReducerContext, _scheduled_id: u64) {
}
```

**The golden example:**
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

**The error:** `publish_error: spacetime publish failed (exit=1)`

**Explain the difference:**  
1. The LLM generated code uses incorrect API methods and structures. For instance, it uses `ScheduleAt::repeat(50_000u64)` instead of the correct `ScheduleAt::Interval(Duration::from_millis(50).into())`.
2. The reducer function `tick` should also return a `Result<(), String>` type as in the golden example.

**Root cause:** The documentation may lack clarity on the specific types and return values required, especially around scheduling and reducers.

**Recommendation:**  
Update documentation to clarify the expected types and return values. Use the following:
- Make sure to specify correct API functions and structures in examples.

#### t_017_scheduled_columns
**The generated code:**
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
    let tbl = ctx.db.tick_timer();
    if tbl.count() == 0 {
        tbl.insert(TickTimer {
            scheduled_id: 0,
            scheduled_at: ScheduleAt::RepeatMicros(50_000),
        });
    }
}

#[reducer]
pub fn tick(_ctx: &ReducerContext, _row: TickTimer) {
}
```

**The golden example:**
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

**The error:** `publish_error: spacetime publish failed (exit=1)`

**Explain the difference:**  
1. The LLM incorrectly uses an outdated or wrong structure for scheduling with `ScheduleAt::RepeatMicros(50_000)`.
   
**Root cause:** There is likely a mismatch in expected vs. generated syntax, particularly around the scheduling configuration settings.

**Recommendation:**  
Reiterate the correct usage of scheduling in the updated documentation with ample examples.

---

### Other Failures

#### t_001_basic_tables
**The generated code:**
```rust
use spacetimedb::ReducerContext;

#[spacetimedb::table(name = user)]
pub struct User {
    #[primary_key]
    id: i32,
    name: String,
    age: i32,
    active: bool,
}

#[spacetimedb::table(name = product)]
pub struct Product {
    #[primary_key]
    id: i32,
    title: String,
    price: f32,
    in_stock: bool,
}

#[spacetimedb::table(name = note)]
pub struct Note {
    #[primary_key]
    id: i32,
    body: String,
    rating: i64,
    pinned: bool,
}

#[spacetimedb::reducer(init)]
pub fn init(_ctx: &ReducerContext) {}
```

**The golden example:**
```rust
use spacetimedb::table;

#[table(name = user)]
pub struct User {
    #[primary_key]
    pub id: i32,
    pub name: String,
    pub age: i32,
    pub active: bool,
}

#[table(name = product)]
pub struct Product {
    #[primary_key]
    pub id: i32,
    pub title: String,
    pub price: f32,
    pub in_stock: bool,
}

#[table(name = note)]
pub struct Note {
    #[primary_key]
    pub id: i32,
    pub body: String,
    pub rating: i64,
    pub pinned: bool,
}
```

**The error:** `schema_parity: reducers differ - expected [], got ["init()"]`

**Explain the difference:** The generated code does not include any public fields in the struct definitions, which is a requirement for proper table mapping.

**Root cause:** Public visibility for struct fields may not have been emphasized in the existing documentation.

**Recommendation:** 
Explain the importance of making struct fields public in documentation examples to prevent mismatches during schema checks.

---

### Insert and CRUD Related Tests

#### t_004_insert
**The generated code:**
```rust
use spacetimedb::{reducer, table, ReducerContext, Table};

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

**The golden example:**
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

**The error:** `data_parity_insert_user: spacetime sql failed: Error: no such table: user.`

**Explain the difference:** Missing the `pub` visibility specifier for struct fields and failing to return a result type in reducer functions.

**Root cause:** Lack of emphasis on the need for public struct fields in the documentation and clear result type specifications.

**Recommendation:** 
Strengthen language around public field requirements and return types in reducer functions in the documentation.

---

## C# / docs Failures

### Timeout Issues

#### t_020_ecs
*This failure is expected in both Rust and C#, highlighting a systemic timeout issue.*

---

### Other Failures

#### t_002_scheduled_table
**The generated code:**
```csharp
using SpacetimeDB;

public static partial class Module
{
    [Table(Name = "TickTimer", Scheduled = nameof(Tick), ScheduledAt = nameof(TickTimer.ScheduledAt))]
    public partial struct TickTimer
    {
        [PrimaryKey, AutoInc] public ulong ScheduledId;
        public ScheduleAt ScheduledAt;
    }

    [Reducer]
    public static void Tick(ReducerContext ctx, TickTimer timer) { }

    [Reducer(ReducerKind.Init)]
    public static void Init(ReducerContext ctx)
    {
        var interval = new TimeDuration { Microseconds = 50_000 };
        ctx.Db.TickTimer.Insert(new TickTimer
        {
            ScheduledAt = new ScheduleAt.Interval(interval)
        });
    }
}
```

**The expected example:**
```csharp
using SpacetimeDB;

public static partial class Module
{
    [Table(Name = "TickTimer", Scheduled = nameof(Tick), ScheduledAt = nameof(TickTimer.ScheduledAt))]
    public partial struct TickTimer
    {
        [PrimaryKey, AutoInc] public ulong ScheduledId;
        public ScheduleAt ScheduledAt;
    }

    [Reducer]
    public static void Tick(ReducerContext ctx, TickTimer _row) { }

    [Reducer(ReducerKind.Init)]
    public static void Init(ReducerContext ctx)
    {
        var interval = new TimeDuration { Microseconds = 50_000 };
        ctx.Db.TickTimer.Insert(new TickTimer
        {
            ScheduledId = 0,
            ScheduledAt = new ScheduleAt.Interval(interval)
        });
    }
}
```

**The error:** `publish_error: 500 Internal Server Error`

**Explain the difference:** In the generated code, the scheduled fields are improperly initialized, and the reducer method signatures do not entirely match expectations regarding types and method names.

**Root cause:** Possibly unclear examples in the documentation regarding the scheduled fields and method signature conventions.

**Recommendation:** 
Revise documentation examples to ensure all aspects of the scheduled field implementations and reducer methods are included, particularly concerning required fields.

---

### Conclusion
The main failures across languages predominantly stem from three issues:
1. Lack of clarity on public struct field visibility.
2. Missing result type requirements in reducer methods.
3. Incorrect method signatures and scheduling configurations.

Specific recommendations center around **enhancing documentation with clearer examples**, **emphasizing visibility requirements**, and detailing the **return types for functions**. This will help streamline the development process and mitigate these common errors in future benchmarks.

# Benchmark Failure Analysis

Generated from: `/__w/SpacetimeDB/SpacetimeDB/tools/xtask-llm-benchmark/../../docs/llms/docs-benchmark-details.json`

## Summary

- **Total failures analyzed**: 35

---

# Analysis of SpacetimeDB Benchmark Test Failures

## Rust / Rustdoc_JSON Failures

### Compile/Publish Errors (4 failures)

#### Failure Group: Scheduled Functions

**1. Code Examples**

**LLM Output**:
```rust
#[spacetimedb::table(name = tick_timer, schedule(reducer = tick, column = scheduled_at))]
pub struct TickTimer {
    #[primary_key]
    #[auto_inc]
    scheduled_id: u64,
    scheduled_at: ScheduleAt,
}

#[spacetimedb::reducer(init)]
pub fn init(ctx: &ReducerContext) {
    ctx.db.tick_timer().insert(TickTimer {
        scheduled_id: 0,
        scheduled_at: ScheduleAt::Interval(Duration::from_micros(50_000).into()),
    });
}

#[spacetimedb::reducer]
pub fn tick(_ctx: &ReducerContext, _row: TickTimer) {
}
```

**Expected (golden)**:
```rust
#[table(name = tick_timer, scheduled(tick))]
pub struct TickTimer {
    #[primary_key]
    #[auto_inc]
    pub scheduled_id: u64,
    pub scheduled_at: ScheduleAt,
}

#[reducer(init)]
pub fn init(ctx: &ReducerContext) -> Result<(), String> {
    ctx.db.tick_timer().insert(TickTimer {
        scheduled_id: 0,
        scheduled_at: ScheduleAt::Interval(Duration::from_millis(50).into()),
    });
    Ok(())
}

#[reducer]
pub fn tick(_ctx: &ReducerContext, _row: TickTimer) -> Result<(), String> {
    Ok(())
}
```

**2. Error**: `publish_error: spacetime publish failed (exit=1)`

**3. Differences**:
- The expected code uses the correct syntax for initializing the scheduled function and includes return types (`Result<(), String>`).
- The `scheduled_at` value is initialized using `Duration::from_millis(50)` instead of `Duration::from_micros(50_000)`.

**4. Root Cause**: 
The generated functions lacked return types, which are critical for reducers in Rust, and the duration initialization was incorrect.

**5. Recommendation**: 
- Update the documentation to specify that all reducer functions should have return types.
- Clarify the correct usage of time duration initialization for `ScheduleAt`.

#### Failure Group: Structs in Tables

**1. Code Examples**

**LLM Output**:
```rust
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
```

**Expected (golden)**:
```rust
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

**2. Error**: `schema_parity: reducers differ - expected [], got ["add_entity()"]`

**3. Differences**:
- The original code lacked public access modifiers for struct fields, which is required for the SpacetimeDB framework.

**4. Root Cause**: 
LLM output does not follow the requirement for public fields in data structures intended for use with SpacetimeDB.

**5. Recommendation**: 
- Update the docstring examples to include access modifiers for struct fields when using `SpacetimeType`.

---

### Other Failures (4 failures)

#### Failure Group: Incomplete Functions and Missing Return Types

**1. Code Examples**

**LLM Output**:
```rust
#[spacetimedb::reducer]
pub fn empty_reducer_no_args(_ctx: &ReducerContext) { }

#[spacetimedb::reducer]
pub fn empty_reducer_with_int(_ctx: &ReducerContext, count: i32) { }
```

**Expected (golden)**:
```rust
#[reducer]
pub fn empty_reducer_no_args(ctx: &ReducerContext) -> Result<(), String> {
    Ok(())
}

#[reducer]
pub fn empty_reducer_with_int(ctx: &ReducerContext, count: i32) -> Result<(), String> {
    Ok(())
}
```

**2. Error**: `schema_parity: describe failed`

**3. Differences**:
- The reducers in the generated code did not have the required return types (`Result<(), String>`) and failed to match the expected function signatures.

**4. Root Cause**: 
Inconsistent documentation specifying both function signatures and expected return types for reducers.

**5. Recommendation**: 
- Ensure that all documentation explicitly mentions that all reducer functions need to return `Result<(), String>`.

---

## C# / Docs Failures

### Other Failures (4 failures)

#### Failure Group: Table Not Found Errors

**1. Code Examples**

**LLM Output**:
```csharp
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
```

**Expected (golden)**:
```csharp
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
```

**2. Error**: `no such table: 'primitive'`

**3. Differences**:
- The generated code defined the table with the `Public` modifier unnecessarily and lacked proper annotations in the expected format.

**4. Root Cause**: 
Inadequate understanding of how to define database tables properly in the SpacetimeDB context from the documentation.

**5. Recommendation**: 
- Revise the documentation to emphasize how tables should be correctly defined with respect to visibility modifiers.

--- 

This analysis highlights common patterns in the failures, including issues with function return types, public access modifiers, and correct syntax for API functions in both Rust and C#. The recommendations aim to enhance clarity and ensure adherence to SpacetimeDB’s requirements.

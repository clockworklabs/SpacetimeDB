# Benchmark Failure Analysis

Generated from: `/__w/SpacetimeDB/SpacetimeDB/tools/xtask-llm-benchmark/../../docs/llms/docs-benchmark-details.json`

## Summary

- **Total failures analyzed**: 35

---

# SpacetimeDB Benchmark Test Failure Analysis

## Summary of Analysis
This document organizes the test failures by language (Rust, C#, etc.) and provides detailed analysis of common issues. Each failure will include comparisons between the LLM-generated and expected code, identify the root causes, and recommend actionable changes to the documentation.

---

## Rust / rustdoc_json Failures (8 total)

### Common Issues
1. **Faulty Function Signatures and Types**: Many failures stem from incorrect function signatures and types in declared reducers and tables.
   
2. **Missing Expected Traits/Attributes**: LLM-generated code often lacks the required public visibility (e.g., `pub`), causing structure and attribute mismatches.

3. **Incorrect Use of Scheduling**: The scheduling function setup was incorrect in multiple instances, failing to align with the expected API-style.

---

### 1. Compile/Publish Errors

#### a. t_002_scheduled_table
**LLM Output**:
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
```

**Expected (golden)**:
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

#[reducer(init)]
pub fn init(ctx: &ReducerContext) -> Result<(), String> {
    ctx.db.tick_timer().insert(TickTimer {
        scheduled_id: 0,
        scheduled_at: ScheduleAt::Interval(Duration::from_millis(50).into()),
    });
    Ok(())
}
```
**Error**: `publish_error: spacetime publish failed (exit=1)`

**Difference**: 
- Missing `pub` visibility on struct fields.
- Usage of `ScheduleAt::RepeatMicros` instead of `ScheduleAt::Interval(Duration::from_millis(...).into())`.

**Root Cause**: 
The documentation lacks clear examples of field visibility and correct usage of scheduling parameters.

**Recommendation**: 
Update the documentation to include visibility modifiers and examples utilizing `ScheduleAt::Interval`.

---

#### b. t_017_scheduled_columns
**Similar to t_002_scheduled_table**. 

- **Action Required**: Same recommendations as above.

---

### 2. Other Failures

#### a. t_012_spacetime_product_type
**LLM Output**:
```rust
use spacetimedb::{table, reducer, ReducerContext, Table, SpacetimeType};

#[derive(SpacetimeType)]
struct Score {
    left: i32,
    right: i32,
}

#[table(name = result)]
struct ResultRow {
    #[primary_key]
    id: i32,
    value: Score,
}

#[reducer]
fn set_score(ctx: &ReducerContext, id: i32, left: i32, right: i32) {
    ctx.db.result().insert(ResultRow {
        id,
        value: Score { left, right },
    });
}
```

**Expected (golden)**:
```rust
use spacetimedb::{reducer, table, ReducerContext, SpacetimeType, Table};

#[derive(SpacetimeType, Clone, Debug)]
pub struct Score {
    pub left: i32,
    pub right: i32,
}

#[table(name = result)]
pub struct ResultRow {
    #[primary_key]
    pub id: i32,
    pub value: Score,
}

#[reducer]
pub fn set_score(ctx: &ReducerContext, id: i32, left: i32, right: i32) {
    ctx.db.result().insert(ResultRow { id, value: Score { left, right } });
}
```

**Error**: `no such table: "result"` 

**Difference**: 
- Missing `pub` declaration on the struct and fields.

**Root Cause**: 
The need for public access levels on types and fields is not sufficiently covered in the documentation.

**Recommendation**: 
Add clear examples demonstrating the use of `pub` in type definitions.

---

### Additional Failures
- The patterns and problems arising in these similar failures (t_013, t_015, t_016, t_018) align closely with public visibility issues or missing API attributes.
- **Action Required**: Follow similar recommendations emphasizing public access in documented type structures.

---

## Rust / docs Failures (22 total)

The issues observed in the Rust documentation failures mirror the previous section, particularly around building, struct declarations, and public visibility.

### Example: t_000_empty_reducers
The LLM-generated code lacks return types on various reducers, while the golden example properly uses `Result<(), String>`.

**Actionable Insight**: Clearly document the importance of declaring function return types on reducers.

---

## C# / docs Failures (5 total)

### Common Issues:
1. **Visibility Modifiers**: Many C# failures stem from missing or wrong visibility modifiers.
2. **Attributes and Enums**: Incorrect use or omission of expected attributes (like `[Type]`) was frequent.

---

### Example: t_013_spacetime_sum_type
**LLM Output**:
```csharp
using SpacetimeDB;
using SpacetimeDB.Types;

public static partial class Module
{
    [SpacetimeDB.Type]
    public partial struct Circle { public int Radius; }
    
    [SpacetimeDB.Table(Name = "Result", Public = true)]
    public partial struct Result
    {
        [SpacetimeDB.PrimaryKey] public int Id;
        public Shape Value;
    }
}
```

**Expected (golden)**:
```csharp
using SpacetimeDB;

public static partial class Module
{
    [Type] public partial struct Circle { public int Radius; }
    
    [Table(Name = "Result")]
    public partial struct Result
    {
        [PrimaryKey] public int Id;
        public Shape Value;
    }
}
```
**Error**: `publish_error: spacetime build (csharp) failed`

**Difference**: Incorrectly specified `Public = true` attribute in the table declaration; missing the `[Type]` attribute.

**Root Cause**: Lack of explicit examples and standards regarding visibility and attribute application in the documentation.

**Recommendation**: Incorporate detailed examples that differentiate between public and default access levels, especially in C# attributes.

---

## Conclusion
The analysis reveals consistent issues to be addressed across various tests. The primary points of failure involve:
- Visibility modifiers.
- Correct API usage of structure and attributes.
- Suggest clarifying the conditions and requirements for successful compilation in the documentation to enhance developer understanding and reduce confusion.

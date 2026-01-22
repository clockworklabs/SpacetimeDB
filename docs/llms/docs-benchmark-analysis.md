# Benchmark Failure Analysis

Generated from: `/__w/SpacetimeDB/SpacetimeDB/tools/xtask-llm-benchmark/../../docs/llms/docs-benchmark-details.json`

## Summary

- **Total failures analyzed**: 34

---

# SpacetimeDB Benchmark Test Failures Analysis

This document analyzes the SpacetimeDB benchmark test failures. Each failure is broken down into its components, specifying the discrepancies between the generated code and the expected golden examples. Specific, actionable recommendations for documentation adjustments are provided to avoid such issues in the future.

## Rust / rustdoc_json Failures

### 1. General Errors with Code Generation

#### **Failures Overview**: The following tests failed due to various issues regarding table definitions, missing `pub` keywords, and improper syntax in function signatures.

#### **Failures Grouped by Cause**

---

### a. **Issue with `pub` Keyword and Function Signatures**

#### **Failures**: 
- t_002_scheduled_table
- t_015_product_type_columns
- t_017_scheduled_columns

#### 1. **t_002_scheduled_table**
**Error**: publish_error: spacetime publish failed (exit=1)

**LLM Output**:
```rust
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
        scheduled_at: ScheduleAt::AfterMicros(50_000),
    });
}
```

**Expected**:
```rust
#[table(name = tick_timer, scheduled(tick))]
pub struct TickTimer {
    #[primary_key]
    #[auto_inc]
    pub scheduled_id: u64,
    pub scheduled_at: ScheduleAt,
}

#[reducer]
pub fn tick(_ctx: &ReducerContext, _schedule: TickTimer) { }

#[reducer(init)]
pub fn init(ctx: &ReducerContext) {
    ctx.db.tick_timer().insert(TickTimer {
        scheduled_id: 0,
        scheduled_at: ScheduleAt::Interval(Duration::from_millis(50).into()),
    });
}
```

---

#### **2. t_015_product_type_columns**
**Error**: publish_error: spacetime publish failed (exit=1)

**LLM Output**:
```rust
#[derive(SpacetimeType)]
pub struct Address {
    street: String,
    zip: i32,
}

#[table(name = profile)]
pub struct Profile {
    #[primary_key]
    id: i32,
    home: Address,
    work: Address,
}
```

**Expected**:
```rust
#[derive(SpacetimeType, Clone, Debug)]
pub struct Address {
    pub street: String,
    pub zip: i32,
}

#[table(name = profile)]
pub struct Profile {
    #[primary_key]
    pub id: i32,
    pub home: Address,
    pub work: Address,
}
```

---

#### **3. t_017_scheduled_columns**
**Error**: publish_error: spacetime publish failed (exit=1)

**LLM Output**:
```rust
#[table(name = tick_timer, schedule(reducer = tick, column = scheduled_at))]
pub struct TickTimer {
    #[primary_key]
    #[auto_inc]
    scheduled_id: u64,
    scheduled_at: ScheduleAt,
}
```

**Expected**:
```rust
#[table(name = tick_timer, scheduled(tick))]
pub struct TickTimer {
    #[primary_key]
    #[auto_inc]
    pub scheduled_id: u64,
    pub scheduled_at: ScheduleAt,
}
```

---

### **Explanation**
- **Difference**: The generated code often lacks the `pub` specifier on struct fields and functions, making them inaccessible outside their module. Function signatures also often miss the required return types.
- **Root Cause**: Documentation may not clarify that all fields in a SpacetimeDB struct need to be public.
- **Recommendation**: Update the documentation to specify that all fields in structs used in SpacetimeDB contexts should be marked as `pub`, along with specifying the necessity of return types for all reducer functions.

---

### b. **Errors Regarding Missing or Incorrect Implementation**

#### **Failures**: 
- t_013_spacetime_sum_type
- t_016_sum_type_columns
- t_018_constraints

#### 1. **t_013_spacetime_sum_type**
**Error**: no such table: `result`.

**LLM Output**:
```rust
#[table(name = result)]
pub struct ResultRow {
    #[primary_key]
    id: i32,
    value: Shape,
}
```

**Expected**:
```rust
#[table(name = result)]
pub struct ResultRow {
    #[primary_key]
    pub id: i32,
    pub value: Shape,
}
```

---

#### **2. t_016_sum_type_columns**
**Error**: no such table: `drawings`.

**LLM Output**:
```rust
#[spacetimedb::table(name = drawing)]
pub struct Drawing {
    #[primary_key]
    id: i32,
    a: Shape,
    b: Shape,
}
```

**Expected**:
```rust
#[table(name = drawing)]
pub struct Drawing {
    #[primary_key]
    pub id: i32,
    pub a: Shape,
    pub b: Shape,
}
```

---

#### **3. t_018_constraints**
**Error**: no such table: `account`.

**LLM Output**:
```rust
#[table(name = account, index(name = by_name, btree(columns = [name])))]
pub struct Account {
    #[primary_key]
    id: i32,
    #[unique]
    email: String,
    name: String,
}
```

**Expected**:
```rust
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
```

---

### **Explanation**
- **Difference**: The generated code frequently omits the `pub` visibility and fails to implement the required structure correctly.
- **Root Cause**: Documentation may not sufficiently outline the importance of proper struct and field visibility in SpacetimeDB schemas.
- **Recommendation**: Emphasize in the documentation that all fields in structs must be public and provide examples highlighting this requirement.

---

## Rust / docs Failures

### **Timeout Issues**

#### **Failure**
- t_020_ecs

#### **Analysis**
This failure indicates that the task exceeded its time limit during execution. Check if the test could be optimized for performance or if the complexity is overly high.

#### **Recommendation**
Optimize the performance of tests, ensuring they do not exceed reasonable time limits. Additionally, incorporate timeouts into the documentation to assist developers in recognizing performance expectations.

---

## C# / docs Failures 

### **1. Missing `pub` Keywords and Related Language Features**

---

#### **Failures**:
- t_014_elementary_columns
- t_017_scheduled_columns

#### 1. **t_014_elementary_columns**
**Error**: no such table: `primitive`.

**LLM Output**:
```csharp
[Table(Name = "Primitive", Public = true)]
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
```

**Expected**:
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
```

---

#### **2. t_017_scheduled_columns**
**Error**: no such table: `tick_timer`.

**LLM Output**:
```csharp
[Table(Name = "TickTimer", Scheduled = nameof(Tick), ScheduledAt = nameof(ScheduledAt))]
public partial struct TickTimer
{
    [PrimaryKey, AutoInc]
    public ulong ScheduledId;
    public ScheduleAt ScheduledAt;
}
```

**Expected**:
```csharp
[Table(Name = "TickTimer", Scheduled = nameof(Tick), ScheduledAt = nameof(ScheduledAt))]
public partial struct TickTimer
{
    [PrimaryKey, AutoInc] public ulong ScheduledId;
    public ScheduleAt ScheduledAt;
}
```

---

### **Explanation**
- **Difference**: The generated C# code fails to keep all relevant visibility concerns in mind.
- **Root Cause**: Similar to Rust failures, the need for public visibility in C# struct fields hasn't been emphasized, leading to errors.
- **Recommendation**: Document the need for public access modifiers explicitly for struct fields in C#, especially for types used with SpacetimeDB.

---

## Conclusion

This analysis provides insights into the root causes of the failures in the SpacetimeDB benchmarks across both Rust and C#. Identifying the missing `pub` keywords, incorrect function signatures, and general visibility issues will help refine documentation and improve the quality of generated code. By adopting the recommended changes, future tests will likely yield more consistent successes.

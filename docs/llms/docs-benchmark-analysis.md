# Benchmark Failure Analysis

Generated from: `C:\Users\Tyler\Developer\SpacetimeDB\tools\xtask-llm-benchmark\../../docs/llms/docs-benchmark-details.json`

## Summary

- **Total failures analyzed**: 33

---

# Analysis of SpacetimeDB Benchmark Test Failures

## Rust / rustdoc_json Failures

### Table Naming Issues

#### t_010_connect
1. **The generated code**:
   ```rust
   #[table(name = events)]
   pub struct Event {
       #[primary_key]
       #[auto_inc]
       id: i32,
       kind: String,
   }
   ```

2. **The golden example**:
   ```rust
   #[table(name = event)]
   pub struct Event {
       #[primary_key]
       #[auto_inc]
       pub id: u64,
       pub kind: String,
   }
   ```

3. **The error**: `schema_parity: tables differ - expected ["event"], got ["events"]`

4. **Explain the difference**: The generated code incorrectly uses `events` (plural) instead of the expected `event` (singular).

5. **Root cause**: The documentation does not clearly specify naming conventions for tables. There is ambiguity around whether singular or plural forms should be used.

6. **Recommendation**: Update documentation to clearly specify that table names should be singular.

---

#### t_014_elementary_columns
1. **The generated code**:
   ```rust
   #[table(name = primitives)]
   pub struct Primitive {
       #[primary_key]
       id: i32,
       count: i32,
       total: i64,
       price: f32,
       ratio: f64,
       active: bool,
       name: String,
   }
   ```

2. **The golden example**:
   ```rust
   #[table(name = primitive)]
   pub struct Primitive {
       #[primary_key]
       pub id: i32,
       pub count: i32,
       pub total: i64,
       pub price: f32,
       pub ratio: f64,
       pub active: bool,
       pub name: String,
   }
   ```

3. **The error**: `schema_parity: tables differ - expected ["primitive"], got ["primitives"]`

4. **Explain the difference**: Similar to the previous case, this failure arises from using the plural form `primitives` instead of the singular `primitive`.

5. **Root cause**: Lack of clear specification on singular vs. plural for table names in the documentation.

6. **Recommendation**: Strengthen the documentation regarding table naming conventions to specify the use of singular names.

---

#### t_016_sum_type_columns
1. **The generated code**:
   ```rust
   #[table(name = drawings)]
   pub struct Drawing {
       #[primary_key]
       id: i32,
       a: Shape,
       b: Shape,
   }
   ```

2. **The golden example**:
   ```rust
   #[table(name = drawing)]
   pub struct Drawing {
       #[primary_key]
       pub id: i32,
       pub a: Shape,
       pub b: Shape,
   }
   ```

3. **The error**: `schema_parity: tables differ - expected ["drawing"], got ["drawings"]`

4. **Explain the difference**: The difference is again in the use of the plural `drawings` instead of the singular `drawing`.

5. **Root cause**: The documentation does not differentiate between singular and plural usage in table naming, leading to confusion.

6. **Recommendation**: Clarify in the documentation that all table names must be singular.

---

### Compile/Publish Errors

#### t_002_scheduled_table
1. **The generated code**:
   ```rust
   #[table(name = tick_timer, schedule(reducer = tick, column = scheduled_at))]
   pub struct TickTimer {
       #[primary_key]
       #[auto_inc]
       scheduled_id: u64,
       scheduled_at: ScheduleAt,
   }
   ```

2. **The golden example**:
   ```rust
   #[table(name = tick_timer, scheduled(tick))]
   pub struct TickTimer {
       #[primary_key]
       #[auto_inc]
       pub scheduled_id: u64,
       pub scheduled_at: ScheduleAt,
   }
   ```

3. **The error**: `publish_error: spacetime publish failed`

4. **Explain the difference**: The generated code defines the schedule incorrectly by including `column = scheduled_at`, whereas the golden example specifies it more succinctly as `scheduled(tick)`.

5. **Root cause**: The documentation might not have clear examples for scheduled table definitions.

6. **Recommendation**: Provide clear documentation for how to properly define scheduled tables in SpacetimeDB.

---

#### t_017_scheduled_columns
1. **The generated code**:
   ```rust
   #[table(name = tick_timer, schedule(reducer = tick, column = scheduled_at))]
   pub struct TickTimer {
       #[primary_key]
       #[auto_inc]
       scheduled_id: u64,
       scheduled_at: ScheduleAt,
   }
   ```

2. **The golden example**:
   ```rust
   #[table(name = tick_timer, scheduled(tick))]
   pub struct TickTimer {
       #[primary_key]
       #[auto_inc]
       pub scheduled_id: u64,
       pub scheduled_at: ScheduleAt,
   }
   ```

3. **The error**: `publish_error: spacetime publish failed`

4. **Explain the difference**: The same error as in the previous case arises from the incorrect definition of the scheduled attribute.

5. **Root cause**: Misunderstanding from the LLM regarding how to define schedules in tables.

6. **Recommendation**: Update the documentation to clarify the correct syntax for specifying schedules in table definitions.

---

### Other Failures

#### t_003_struct_in_table
1. **The generated code**:
   ```rust
   #[table(name = entity)]
   pub struct Entity {
       #[primary_key]
       id: i32,
       pos: Position,
   }
   ```

2. **The golden example**:
   ```rust
   #[table(name = entity)]
   pub struct Entity {
       #[primary_key]
       pub id: i32,
       pub pos: Position,
   }
   ```

3. **The error**: `schema_parity: reducers differ - expected [], got ["add_entity()"]`

4. **Explain the difference**: The generated code does not have the `pub` visibility for struct fields.

5. **Root cause**: Documentation may not sufficiently emphasize the need for visibility modifiers in struct fields for proper API access.

6. **Recommendation**: Enhance the docs to specify that all struct fields must be public to ensure compatibility with the API.

---

#### t_020_ecs
1. **The generated code**:
   ```rust
   #[table(name = position)]
   pub struct Position {
       #[primary_key]
       entity_id: i32,
       x: i32,
       y: i32,
   }
   ```

2. **The golden example**:
   ```rust
   #[table(name = position)]
   pub struct Position {
       #[primary_key]
       pub entity_id: i32,
       pub x: i32,
       pub y: i32,
   }
   ```

3. **The error**: `spacetime sql failed`

4. **Explain the difference**: The struct fields lack `pub` visibility.

5. **Root cause**: As with the previous issue, visibility modifiers are crucial but may not be adequately detailed in the documentation.

6. **Recommendation**: Clarify in the docs that all database struct fields should be public for compatibility.

---

## C# / docs Failures

### Other Failures

#### t_014_elementary_columns
1. **The generated code**:
   ```csharp
   [SpacetimeDB.Table(Name = "Primitive")]
   public partial struct Primitive {
       [SpacetimeDB.PrimaryKey]
       public int Id;
       public int Count;
       public long Total;
       public float Price;
       public double Ratio;
       public bool Active;
       public string Name;
   }
   ```

2. **The golden example**:
   ```csharp
   [Table(Name = "Primitive")]
   public partial struct Primitive {
       [PrimaryKey] public int Id;
       public int Count;
       public long Total;
       public float Price;
       public double Ratio;
       public bool Active;
       public string Name;
   }
   ```

3. **The error**: `primitives` is not a valid table

4. **Explain the difference**: The generated code does not use consistent casing for the attributes, such as `SpacetimeDB.Table` and `Table`.

5. **Root cause**: Lack of clarity in the documentation about proper casing practices for attributes.

6. **Recommendation**: Standardize attribute naming conventions in documentation to avoid confusion over casing.

---

#### t_016_sum_type_columns
1. **The generated code**:
   ```csharp
   [SpacetimeDB.Table(Name = "Drawing", Public = true)]
   public partial struct Drawing {
       [SpacetimeDB.PrimaryKey]
       public int Id;
       public Shape A;
       public Shape B;
   }
   ```

2. **The golden example**:
   ```csharp
   [Table(Name = "Drawing")]
   public partial struct Drawing {
       [PrimaryKey] public int Id;
       public Shape A;
       public Shape B;
   }
   ```

3. **The error**: `drawings` is not a valid table

4. **Explain the difference**: Similar to the last case, inconsistent use of `SpacetimeDB` leads to issues.

5. **Root cause**: Inconsistencies in attribute naming conventions.

6. **Recommendation**: Implement strict guidelines for attribute naming in documentation to ensure uniformity.

---

#### t_017_scheduled_columns
1. **The generated code**:
   ```csharp
   [Table(Name = "TickTimer", Scheduled = nameof(Tick), ScheduledAt = nameof(ScheduledAt))]
   public partial struct TickTimer {
       [PrimaryKey, AutoInc]
       public ulong ScheduledId;
       public ScheduleAt ScheduledAt;
   }
   ```

2. **The golden example**:
   ```csharp
   [Table(Name = "TickTimer", Scheduled = nameof(Tick), ScheduledAt = nameof(ScheduledAt))]
   public partial struct TickTimer {
       [PrimaryKey, AutoInc] public ulong ScheduledId;
       public ScheduleAt ScheduledAt;
   }
   ```

3. **The error**: `tick_timer` is not a valid table

4. **Explain the difference**: The naming conventions in the generated code are incorrect compared to the expected outcome.

5. **Root cause**: Poor documentation on how to correctly declare scheduled columns and their attributes.

6. **Recommendation**: Enhance documentation to clarify how to declare scheduled columns accurately.

---

#### t_020_ecs
1. **The generated code**:
   ```csharp
   [SpacetimeDB.Table(Name = "Entity")]
   public partial struct Entity { [SpacetimeDB.PrimaryKey] public int Id; }
   ```

2. **The golden example**:
   ```csharp
   [Table(Name = "Entity")] public partial struct Entity { [PrimaryKey] public int Id; }
   ```

3. **The error**: `next_positions` is not a valid table

4. **Explain the difference**: As with previous cases, inconsistent attribute casing causes issues.

5. **Root cause**: Documentation does not provide clear direction regarding consistent attribute casing.

6. **Recommendation**: Consolidate and standardize attribute naming practices in the documentation to avoid confusion in the code structure.

--- 

## Conclusion
The majority of failures stem from inconsistencies in naming conventions, visibility modifiers, and attribute casing. It is recommended to enhance the SpacetimeDB documentation to emphasize these aspects clearly to avoid further misunderstandings and mistakes. Strong and specific recommendations are essential for a robust learning experience for developers using SpacetimeDB.

---
title: Auto-Increment
slug: /tables/auto-increment
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';


Auto-increment columns automatically generate unique integer values for new rows. When you insert a row with a zero value in an auto-increment column, SpacetimeDB assigns the next value from an internal sequence.

## Defining Auto-Increment Columns

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
const post = table(
  { name: 'post', public: true },
  {
    id: t.u64().primaryKey().autoInc(),
    title: t.string(),
  }
);

const spacetimedb = schema(post);

spacetimedb.reducer('add_post', { title: t.string() }, (ctx, { title }) => {
  // Pass 0 for the auto-increment field
  const inserted = ctx.db.post.insert({ id: 0n, title });
  // inserted.id now contains the assigned value
  console.log(`Created post with id: ${inserted.id}`);
});
```

Use the `.autoInc()` method on a column builder.

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
[SpacetimeDB.Table(Name = "Post", Public = true)]
public partial struct Post
{
    [SpacetimeDB.PrimaryKey]
    [SpacetimeDB.AutoInc]
    public ulong Id;
    public string Title;
}

[SpacetimeDB.Reducer]
public static void AddPost(ReducerContext ctx, string title)
{
    // Pass 0 for the auto-increment field
    var inserted = ctx.Db.Post.Insert(new Post { Id = 0, Title = title });
    // inserted.Id now contains the assigned value
    Log.Info($"Created post with id: {inserted.Id}");
}
```

Use the `[SpacetimeDB.AutoInc]` attribute.

</TabItem>
<TabItem value="rust" label="Rust">

```rust
use spacetimedb::{ReducerContext, Table};

#[spacetimedb::table(name = post, public)]
pub struct Post {
    #[primary_key]
    #[auto_inc]
    id: u64,
    title: String,
}

#[spacetimedb::reducer]
fn add_post(ctx: &ReducerContext, title: String) -> Result<(), String> {
    // Pass 0 for the auto-increment field
    let inserted = ctx.db.post().insert(Post { id: 0, title });
    // inserted.id now contains the assigned value
    log::info!("Created post with id: {}", inserted.id);
    Ok(())
}
```

Use the `#[auto_inc]` attribute.

</TabItem>
</Tabs>

Auto-increment columns must be integer types (`u8`, `u16`, `u32`, `u64`, `i8`, `i16`, `i32`, `i64`, etc.).

## Trigger Value

The auto-increment mechanism activates when you insert a row with a **zero value** in the auto-increment column. If you insert a non-zero value, SpacetimeDB uses that value directly without generating a new one.

```rust
// Triggers auto-increment: id will be assigned automatically
ctx.db.post().insert(Post { id: 0, title: "Hello".into() })?;

// Does NOT trigger auto-increment: id will be 42
ctx.db.post().insert(Post { id: 42, title: "World".into() })?;
```

This behavior allows you to migrate existing data with known IDs while still using auto-increment for new rows.

## Sequences

SpacetimeDB implements auto-increment using **sequences**, a mechanism loosely modeled after PostgreSQL sequences. A sequence is an internal counter that generates a series of integer values according to configurable parameters.

### Sequence Parameters

Each sequence has the following parameters:

| Parameter | Description |
|-----------|-------------|
| `start` | The first value the sequence generates |
| `min_value` | The minimum value in the sequence range |
| `max_value` | The maximum value in the sequence range |
| `increment` | The step between consecutive values (can be negative) |

For auto-increment columns, SpacetimeDB creates a sequence with sensible defaults based on the column type. For example, a `u64` column gets a sequence starting at 1 with a maximum of 2^64 - 1.

### Wrapping Behavior

When a sequence reaches its maximum value, it wraps around to the minimum value and continues. For a sequence with `min_value = 1`, `max_value = 10`, and `increment = 1`, the values cycle as: 1, 2, 3, ..., 9, 10, 1, 2, 3, ...

Sequences with negative increments wrap in the opposite direction. A sequence with `min_value = 1`, `max_value = 10`, and `increment = -1` starting at 5 produces: 5, 4, 3, 2, 1, 10, 9, 8, ...

### Crash Recovery

Sequences implement a crash recovery mechanism to ensure values are never reused after a database restart. Rather than persisting the current value after every increment, sequences allocate values in batches.

When a sequence needs a new value and has exhausted its current allocation, it:

1. Calculates the next batch of values
2. Persists the allocation boundary to disk
3. Returns values from the allocated range

If the database crashes, it restarts from the persisted allocation boundary. This may skip some values that were allocated but never used, but guarantees that no value is ever assigned twice.

For example, if a sequence allocates values in batches of 10:

1. First insert triggers allocation of values 1-10
2. Values 1, 2, 3 are used
3. Database crashes
4. On restart, the sequence resumes from value 1 (the allocation boundary)
5. The sequence allocates values 1-10 again, but now starts fresh

This design trades potential gaps in the sequence for durability and performance. The batch size balances the cost of persistence against the size of potential gaps.

### Uniqueness Considerations

Sequences generate values in a deterministic order, but wrapping means the same value can appear multiple times over the lifetime of a sequence. If your auto-increment column is also a primary key or has a unique constraint, inserting a duplicate value will fail.

For most applications, the range of a 64-bit integer is large enough that wrapping never occurs in practice. However, if you use a smaller type like `u8` or `u16`, or if your application has very high insert volume, plan for the possibility of sequence exhaustion.

### Concurrency and Gaps

Sequences do not guarantee sequential ordering. Gaps can appear in auto-increment values for several reasons:

1. **Crash recovery**: The batch allocation mechanism may skip values that were allocated but never used before a crash.

2. **Concurrent transactions**: SpacetimeDB currently executes transactions serially, but reserves the right to execute them concurrently in future versions. With concurrent execution, two transactions inserting into the same table may receive interleaved sequence values.

Even within a single reducer, you should not assume that consecutive inserts produce consecutive values. For example:

```rust
let a = ctx.db.post().insert(Post { id: 0, title: "First".into() })?;
let b = ctx.db.post().insert(Post { id: 0, title: "Second".into() })?;
// a.id might be 1 and b.id might be 3, not necessarily 1 and 2
```

If your application requires strictly sequential numbering without gaps, maintain that counter explicitly in a separate table rather than relying on auto-increment:

```rust
use spacetimedb::{ReducerContext, Table};

#[derive(Clone)]
#[spacetimedb::table(name = counter, public)]
pub struct Counter {
    #[primary_key]
    name: String,
    value: u64,
}

#[spacetimedb::table(name = invoice, public)]
pub struct Invoice {
    #[primary_key]
    invoice_number: u64,
    amount: u64,
}

#[spacetimedb::reducer]
fn create_invoice(ctx: &ReducerContext, amount: u64) -> Result<(), String> {
    // Get or create the counter
    let mut counter = ctx.db.counter().name().find(&"invoice".to_string())
        .unwrap_or(Counter { name: "invoice".to_string(), value: 0 });

    // Increment and update
    counter.value += 1;
    ctx.db.counter().name().update(counter.clone());

    // Use the counter value as the invoice number
    ctx.db.invoice().insert(Invoice {
        invoice_number: counter.value,
        amount,
    });

    Ok(())
}
```

This pattern guarantees sequential values because the counter update and row insert occur within the same transaction.

## Combining with Other Attributes

Auto-increment columns are commonly combined with primary keys:

```rust
#[spacetimedb::table(name = post, public)]
pub struct Post {
    #[primary_key]
    #[auto_inc]
    id: u64,
    // ...
}
```

Auto-increment columns can also be combined with unique constraints:

```rust
#[spacetimedb::table(name = item, public)]
pub struct Item {
    #[primary_key]
    name: String,
    #[unique]
    #[auto_inc]
    item_number: u32,
}
```

Auto-increment **cannot** be combined with default values, since both attempt to populate the column automatically.

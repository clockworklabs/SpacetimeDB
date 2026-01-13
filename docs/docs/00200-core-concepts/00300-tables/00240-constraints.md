---
title: Constraints
slug: /tables/constraints
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';


Constraints enforce data integrity rules on your columns. SpacetimeDB supports unique constraints, auto-increment, and default values.

## Unique Columns

Mark columns as unique to ensure no two rows can have the same value for that column.

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
const user = table(
  { name: 'user', public: true },
  {
    id: t.u32().primaryKey(),
    email: t.string().unique(),
    username: t.string().unique(),
  }
);
```

Use the `.unique()` method on a column builder.

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
[SpacetimeDB.Table(Name = "user", Public = true)]
public partial struct User
{
    [SpacetimeDB.PrimaryKey]
    public uint Id;

    [SpacetimeDB.Unique]
    public string Email;

    [SpacetimeDB.Unique]
    public string Username;
}
```

Use the `[SpacetimeDB.Unique]` attribute.

</TabItem>
<TabItem value="rust" label="Rust">

```rust
#[spacetimedb::table(name = user, public)]
pub struct User {
    #[primary_key]
    id: u32,
    #[unique]
    email: String,
    #[unique]
    username: String,
}
```

Use the `#[unique]` attribute.

</TabItem>
</Tabs>

Unlike primary keys, you can have multiple unique columns on a single table. Unique columns also create an index that enables efficient lookups.

## Auto-Increment

Use auto-increment for automatically increasing integer identifiers. When you insert a row with a zero value in an auto-increment column, the database assigns a new unique value.

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
  const inserted = ctx.db.post.insert({ id: 0, title });
  // inserted.id now contains the assigned value
  console.log(`Created post with id: ${inserted.id}`);
});
```

Use the `.autoInc()` method on a column builder.

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
[SpacetimeDB.Table(Name = "post", Public = true)]
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
    let inserted = ctx.db.post().insert(Post { id: 0, title })?;
    // inserted.id now contains the assigned value
    log::info!("Created post with id: {}", inserted.id);
    Ok(())
}
```

Use the `#[auto_inc]` attribute.

</TabItem>
</Tabs>

Auto-increment columns must be integer types (`u8`, `u16`, `u32`, `u64`, etc.).

## Default Values

Default values allow you to add new columns to existing tables during [automatic migrations](/databases/automatic-migrations). When you republish a module with a new column that has a default value, existing rows are automatically populated with that default.

:::note
New columns with default values must be added at the **end** of the table definition. Adding columns in the middle of a table is not supported.
:::

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
const player = table(
  { name: 'player', public: true },
  {
    id: t.u64().primaryKey().autoInc(),
    name: t.string(),
    // New columns added with defaults
    score: t.u32().default(0),
    isActive: t.bool().default(true),
    bio: t.string().default(''),
  }
);
```

The `.default(value)` method can be chained on any column type builder. The value must match the column's type.

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
[SpacetimeDB.Table(Name = "player", Public = true)]
public partial struct Player
{
    [SpacetimeDB.PrimaryKey]
    [SpacetimeDB.AutoInc]
    public ulong Id;

    public string Name;

    // New columns added with defaults
    [SpacetimeDB.Default(0u)]
    public uint Score;

    [SpacetimeDB.Default(true)]
    public bool IsActive;

    [SpacetimeDB.Default("")]
    public string Bio;
}
```

The `[SpacetimeDB.Default(value)]` attribute specifies the default value. The value is serialized to match the column's type.

</TabItem>
<TabItem value="rust" label="Rust">

```rust
#[spacetimedb::table(name = player, public)]
pub struct Player {
    #[primary_key]
    #[auto_inc]
    id: u64,
    name: String,
    // New columns added with defaults
    #[default(0)]
    score: u32,
    #[default(true)]
    is_active: bool,
}
```

The `#[default(value)]` attribute specifies the default value. The expression must be const-evaluable (usable in a `const` context).

:::note Rust Limitation
Default values in Rust must be const-evaluable. This means you **cannot** use `String` defaults like `#[default("".to_string())]` because `.to_string()` is not a const fn. Only primitive types, enums, and other const-constructible types can have defaults.
:::

</TabItem>
</Tabs>

### Restrictions

Default values **cannot** be combined with:
- Primary keys
- Unique constraints
- Auto-increment

This restriction exists because these constraints require the database to manage the column values, which conflicts with providing a static default.

:::warning TypeScript: Unintuitive Error Message

In TypeScript, this constraint is enforced at compile time. If you violate it, you'll see an **unintuitive error message**:

```
Expected 3 arguments, but got 2.
```

**This error means one of your columns has an invalid combination of `.default()` with `.primaryKey()`, `.unique()`, or `.autoInc()`.**

For example, this code will produce the error:

```typescript
// ERROR: default() + primaryKey() is not allowed
const badTable = table(
  { name: 'bad' },
  { id: t.u64().default(0n).primaryKey() }  // <- Causes "Expected 3 arguments"
);
```

**How to fix:** Remove either `.default()` or the constraint (`.primaryKey()`/`.unique()`/`.autoInc()`).

:::

### Use Cases

- **Schema evolution**: Add new features to your application without losing existing data
- **Optional fields**: Provide sensible defaults for fields that may not have been tracked historically
- **Feature flags**: Add boolean columns with `default(false)` to enable new functionality gradually

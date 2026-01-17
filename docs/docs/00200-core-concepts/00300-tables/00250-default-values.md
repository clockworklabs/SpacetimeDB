---
title: Default Values
slug: /tables/default-values
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';


Default values allow you to add new columns to existing tables during [automatic migrations](/databases/automatic-migrations). When you republish a module with a new column that has a default value, existing rows are automatically populated with that default.

:::note
New columns with default values must be added at the **end** of the table definition. Adding columns in the middle of a table is not supported.
:::

## Defining Default Values

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
[SpacetimeDB.Table(Name = "Player", Public = true)]
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

## Restrictions

Default values **cannot** be combined with:
- Primary keys
- Unique constraints
- [Auto-increment](/tables/auto-increment)

This restriction exists because these attributes require the database to manage the column values, which conflicts with providing a static default.

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

## Use Cases

- **Schema evolution**: Add new features to your application without losing existing data
- **Optional fields**: Provide sensible defaults for fields that may not have been tracked historically
- **Feature flags**: Add boolean columns with `default(false)` to enable new functionality gradually

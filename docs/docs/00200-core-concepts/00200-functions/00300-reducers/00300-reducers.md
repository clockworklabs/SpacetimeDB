---
title: Overview
slug: /functions/reducers
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';


Reducers are functions that modify database state in response to client requests or system events. They are the **only** way to mutate tables in SpacetimeDB - all database changes must go through reducers.

## Defining Reducers

Reducers are defined in your module code and automatically exposed as callable functions to connected clients.

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

Use the `spacetimedb.reducer` function:

```typescript
import { schema, table, t } from 'spacetimedb/server';

spacetimedb.reducer('create_user', { name: t.string(), email: t.string() }, (ctx, { name, email }) => {
  // Validate input
  if (name === '') {
    throw new Error('Name cannot be empty');
  }
  
  // Modify tables
  ctx.db.user.insert({
    id: 0,  // auto-increment will assign
    name,
    email
  });
});
```

The first argument is the reducer name, the second defines argument types, and the third is the handler function taking `(ctx, args)`.

</TabItem>
<TabItem value="csharp" label="C#">

Use the `[SpacetimeDB.Reducer]` attribute on a static method:

```csharp
using SpacetimeDB;

public static partial class Module
{
    [SpacetimeDB.Reducer]
    public static void CreateUser(ReducerContext ctx, string name, string email)
    {
        // Validate input
        if (string.IsNullOrEmpty(name))
        {
            throw new ArgumentException("Name cannot be empty");
        }
        
        // Modify tables
        ctx.Db.User.Insert(new User
        {
            Id = 0,  // auto-increment will assign
            Name = name,
            Email = email
        });
    }
}
```

Reducers must be static methods with `ReducerContext` as the first parameter. Additional parameters must be types marked with `[SpacetimeDB.Type]`. Reducers should return `void`.

</TabItem>
<TabItem value="rust" label="Rust">

Use the `#[spacetimedb::reducer]` macro on a function:

```rust
use spacetimedb::{reducer, ReducerContext, Table};

#[reducer]
pub fn create_user(ctx: &ReducerContext, name: String, email: String) -> Result<(), String> {
    // Validate input
    if name.is_empty() {
        return Err("Name cannot be empty".to_string());
    }

    // Modify tables
    ctx.db.user().insert(User {
        id: 0, // auto-increment will assign
        name,
        email,
    });

    Ok(())
}
```

Reducers must take `&ReducerContext` as their first parameter. Additional parameters must be serializable types. Reducers can return `()`, `Result<(), String>`, or `Result<(), E>` where `E: Display`.

:::note Rust: Importing the Table Trait
Table operations like `insert`, `try_insert`, `iter`, and `count` are provided by the `Table` trait. You must import this trait for these methods to be available:

```rust
use spacetimedb::Table;
```

If you see errors like "no method named `try_insert` found", add this import.
:::

</TabItem>
</Tabs>

## Transactional Execution

Every reducer runs inside a database transaction. This provides important guarantees:

- **Isolation**: Reducers don't see changes from other concurrent reducers
- **Atomicity**: Either all changes succeed or all are rolled back
- **Consistency**: Failed reducers leave the database unchanged

If a reducer throws an exception or returns an error, all of its changes are automatically rolled back.

## Accessing Tables

Reducers have full read-write access to all tables (both public and private) through the `ReducerContext`. The examples below assume a `user` table with `id` (primary key), `name` (indexed), and `email` (unique) columns.

### Inserting Rows

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
ctx.db.user.insert({
  id: 0,  // auto-increment will assign
  name: 'Alice',
  email: 'alice@example.com'
});
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
ctx.Db.User.Insert(new User
{
    Id = 0,  // auto-increment will assign
    Name = "Alice",
    Email = "alice@example.com"
});
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
ctx.db.user().insert(User {
    id: 0,  // auto-increment will assign
    name: "Alice".to_string(),
    email: "alice@example.com".to_string(),
});
```

</TabItem>
</Tabs>

### Finding Rows by Unique Column

Use `find` on a unique or primary key column to retrieve a single row:

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
const user = ctx.db.user.id.find(123);
if (user) {
  console.log(`Found: ${user.name}`);
}

const byEmail = ctx.db.user.email.find('alice@example.com');
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
var user = ctx.Db.User.Id.Find(123);
if (user is not null)
{
    Log.Info($"Found: {user.Name}");
}

var byEmail = ctx.Db.User.Email.Find("alice@example.com");
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
if let Some(user) = ctx.db.user().id().find(123) {
    log::info!("Found: {}", user.name);
}

let by_email = ctx.db.user().email().find("alice@example.com");
```

</TabItem>
</Tabs>

### Filtering Rows by Indexed Column

Use `filter` on an indexed column to retrieve multiple matching rows:

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
for (const user of ctx.db.user.name.filter('Alice')) {
  console.log(`User ${user.id}: ${user.email}`);
}
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
foreach (var user in ctx.Db.User.Name.Filter("Alice"))
{
    Log.Info($"User {user.Id}: {user.Email}");
}
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
for user in ctx.db.user().name().filter("Alice") {
    log::info!("User {}: {}", user.id, user.email);
}
```

</TabItem>
</Tabs>

### Updating Rows

Find a row, modify it, then call `update` on the same unique column:

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
const user = ctx.db.user.id.find(123);
if (user) {
  user.name = 'Bob';
  ctx.db.user.id.update(user);
}
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
var user = ctx.Db.User.Id.Find(123);
if (user is not null)
{
    user.Name = "Bob";
    ctx.Db.User.Id.Update(user);
}
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
if let Some(mut user) = ctx.db.user().id().find(123) {
    user.name = "Bob".to_string();
    ctx.db.user().id().update(user);
}
```

</TabItem>
</Tabs>

### Deleting Rows

Delete by unique column value or by indexed column value:

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
// Delete by primary key
ctx.db.user.id.delete(123);

// Delete all matching an indexed column
const deleted = ctx.db.user.name.delete('Alice');
console.log(`Deleted ${deleted} row(s)`);
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
// Delete by primary key
ctx.Db.User.Id.Delete(123);

// Delete all matching an indexed column
var deleted = ctx.Db.User.Name.Delete("Alice");
Log.Info($"Deleted {deleted} row(s)");
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
// Delete by primary key
ctx.db.user().id().delete(123);

// Delete all matching an indexed column
let deleted = ctx.db.user().name().delete("Alice");
log::info!("Deleted {} row(s)", deleted);
```

</TabItem>
</Tabs>

### Iterating All Rows

Use `iter` to iterate over all rows in a table:

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
for (const user of ctx.db.user.iter()) {
  console.log(`${user.id}: ${user.name}`);
}
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
foreach (var user in ctx.Db.User.Iter())
{
    Log.Info($"{user.Id}: {user.Name}");
}
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
for user in ctx.db.user().iter() {
    log::info!("{}: {}", user.id, user.name);
}
```

</TabItem>
</Tabs>

### Counting Rows

Use `count` to get the number of rows in a table:

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
const total = ctx.db.user.count();
console.log(`Total users: ${total}`);
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
var total = ctx.Db.User.Count();
Log.Info($"Total users: {total}");
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
let total = ctx.db.user().count();
log::info!("Total users: {}", total);
```

</TabItem>
</Tabs>

For more details on querying with indexes, including range queries and multi-column indexes, see [Indexes](/tables/indexes).

## Reducer Isolation

Reducers run in an isolated environment and **cannot** interact with the outside world:

- ❌ No network requests
- ❌ No file system access  
- ❌ No system calls
- ✅ Only database operations

If you need to interact with external systems, use [Procedures](/functions/procedures) instead. Procedures can make network calls and perform other side effects, but they have different execution semantics and limitations.

## Next Steps

- Learn about [Tables](/tables) to understand data storage
- Explore [Procedures](/functions/procedures) for side effects beyond the database
- Review [Subscriptions](/subscriptions) for real-time client updates

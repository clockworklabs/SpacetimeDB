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

</TabItem>
</Tabs>

## Transactional Execution

Every reducer runs inside a database transaction. This provides important guarantees:

- **Isolation**: Reducers don't see changes from other concurrent reducers
- **Atomicity**: Either all changes succeed or all are rolled back
- **Consistency**: Failed reducers leave the database unchanged

If a reducer throws an exception or returns an error, all of its changes are automatically rolled back.

## Accessing Tables

Reducers can query and modify tables through the `ReducerContext`:

- Inserting rows
- Updating rows by unique columns
- Deleting rows
- Querying with indexes
- Iterating all rows
- Counting rows

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

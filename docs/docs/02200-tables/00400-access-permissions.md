---
title: Table Access Permissions
slug: /tables/access-permissions
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';

# Access Permissions

SpacetimeDB enforces different levels of table access depending on the context. All contexts access tables through `ctx.db`, but the available operations differ based on whether the context is read-write or read-only.

## Reducers - Read-Write Access

Reducers receive a `ReducerContext` which provides full read-write access to tables. They can perform all CRUD operations: insert, read, update, and delete.

<Tabs groupId="server-language" queryString>
<TabItem value="rust" label="Rust">

```rust
#[spacetimedb::reducer]
fn example(ctx: &ReducerContext) -> Result<(), String> {
    // Insert
    ctx.db.user().insert(User {
        id: 0,
        name: "Alice".to_string(),
        email: "alice@example.com".to_string(),
    });

    // Read: iterate all rows
    for user in ctx.db.user().iter() {
        log::info!("User: {}", user.name);
    }

    // Read: find by unique column
    if let Some(mut user) = ctx.db.user().id().find(123) {
        // Update
        user.name = "Bob".to_string();
        ctx.db.user().id().update(user);
    }

    // Delete
    ctx.db.user().id().delete(456);

    Ok(())
}
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
[SpacetimeDB.Reducer]
public static void Example(ReducerContext ctx)
{
    // Insert
    ctx.Db.User.Insert(new User { Id = 0, Name = "Alice", Email = "alice@example.com" });

    // Read: iterate all rows
    foreach (var user in ctx.Db.User.Iter())
    {
        Log.Info($"User: {user.Name}");
    }

    // Read: find by unique column
    if (ctx.Db.User.Id.Find(123) is User foundUser)
    {
        // Update
        foundUser.Name = "Bob";
        ctx.Db.User.Id.Update(foundUser);
    }

    // Delete
    ctx.Db.User.Id.Delete(456);
}
```

</TabItem>
<TabItem value="typescript" label="TypeScript">

```typescript
spacetimedb.reducer('example', {}, (ctx) => {
  // Insert
  ctx.db.user.insert({ id: 0, name: 'Alice', email: 'alice@example.com' });

  // Read: iterate all rows
  for (const user of ctx.db.user.iter()) {
    console.log(user.name);
  }

  // Read: find by unique column
  const foundUser = ctx.db.user.id.find(123);
  if (foundUser) {
    // Update
    foundUser.name = 'Bob';
    ctx.db.user.id.update(foundUser);
  }

  // Delete
  ctx.db.user.id.delete(456);
});
```

</TabItem>
</Tabs>

## Views - Read-Only Access

Views receive a `ViewContext` or `AnonymousViewContext` which provides read-only access to tables. They can query and iterate tables, but cannot insert, update, or delete rows.

<Tabs groupId="server-language" queryString>
<TabItem value="rust" label="Rust">

```rust
#[spacetimedb::view]
fn find_users_by_name(ctx: &ViewContext) -> Vec<User> {
    // Can read and filter
    ctx.db.user().name().filter("Alice")
    
    // Cannot insert, update, or delete
    // ctx.db.user().insert(...) // ❌ Compile error
}
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
[SpacetimeDB.View]
public static List<User> FindUsersByName(ViewContext ctx)
{
    // Can read and filter
    return ctx.Db.user.Name.Filter("Alice").ToList();
    
    // Cannot insert, update, or delete
    // ctx.Db.user.Insert(...) // ❌ Method not available
}
```

</TabItem>
<TabItem value="typescript" label="TypeScript">

```typescript
spacetimedb.view('findUsersByName', (ctx) => {
  // Can read and filter
  return Array.from(ctx.db.user.name.filter('Alice'));
  
  // Cannot insert, update, or delete
  // ctx.db.user.insert(...) // ❌ Method not available
});
```

</TabItem>
</Tabs>

## Client Access - Read-Only Access

Clients connect to databases and can access public tables through subscriptions and queries. See the [Subscriptions documentation](/subscriptions) for details on client-side table access.

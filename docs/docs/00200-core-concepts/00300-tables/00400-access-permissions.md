---
title: Table Access Permissions
slug: /tables/access-permissions
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';


SpacetimeDB controls data access through table visibility and context-based permissions. Tables can be public or private, and different execution contexts (reducers, views, clients) have different levels of access.

## Public and Private Tables

Tables are **private** by default. Private tables can only be accessed by reducers and views running on the server. Clients cannot query, subscribe to, or see private tables.

**Public** tables are exposed to clients for read access through subscriptions and queries. Clients can see public table data but can only modify it by calling reducers.

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
// Private table (default) - only accessible from server-side code
const internalConfig = table(
  { name: 'internal_config' },
  {
    key: t.string().primaryKey(),
    value: t.string(),
  }
);

// Public table - clients can subscribe and query
const player = table(
  { name: 'player', public: true },
  {
    id: t.u64().primaryKey().autoInc(),
    name: t.string(),
    score: t.u64(),
  }
);
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
// Private table (default) - only accessible from server-side code
[SpacetimeDB.Table(Name = "InternalConfig")]
public partial struct InternalConfig
{
    [SpacetimeDB.PrimaryKey]
    public string Key;
    public string Value;
}

// Public table - clients can subscribe and query
[SpacetimeDB.Table(Name = "Player", Public = true)]
public partial struct Player
{
    [SpacetimeDB.PrimaryKey]
    [SpacetimeDB.AutoInc]
    public ulong Id;
    public string Name;
    public ulong Score;
}
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
// Private table (default) - only accessible from server-side code
#[spacetimedb::table(name = internal_config)]
pub struct InternalConfig {
    #[primary_key]
    key: String,
    value: String,
}

// Public table - clients can subscribe and query
#[spacetimedb::table(name = player, public)]
pub struct Player {
    #[primary_key]
    #[auto_inc]
    id: u64,
    name: String,
    score: u64,
}
```

</TabItem>
</Tabs>

Use private tables for:
- Internal configuration or state that clients should not see
- Sensitive data like password hashes or API keys
- Intermediate computation results

Use public tables for:
- Data that clients need to display or interact with
- Game state, user profiles, or other user-facing data

## Reducers - Read-Write Access

Reducers receive a `ReducerContext` which provides full read-write access to all tables (both public and private). They can perform all CRUD operations: insert, read, update, and delete.

<Tabs groupId="server-language" queryString>
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
</Tabs>

## Procedures with Transactions - Read-Write Access

Procedures receive a `ProcedureContext` and can access tables through transactions. Unlike reducers, procedures must explicitly open a transaction to read from or modify the database.

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
spacetimedb.procedure('updateUserProcedure', { userId: t.u64(), newName: t.string() }, t.unit(), (ctx, { userId, newName }) => {
  // Must explicitly open a transaction
  ctx.withTx(ctx => {
    // Full read-write access within the transaction
    const user = ctx.db.user.id.find(userId);
    if (user) {
      user.name = newName;
      ctx.db.user.id.update(user);
    }
  });
  // Transaction is committed when the function returns
  return {};
});

```

</TabItem>
<TabItem value="csharp" label="C#">

Support for procedures in C# modules is coming soon!

</TabItem>
<TabItem value="rust" label="Rust">

```rust
#[spacetimedb::procedure]
fn update_user_procedure(ctx: &mut ProcedureContext, user_id: u64, new_name: String) {
    // Must explicitly open a transaction
    ctx.with_tx(|ctx| {
        // Full read-write access within the transaction
        if let Some(mut user) = ctx.db.user().id().find(user_id) {
            user.name = new_name.clone();
            ctx.db.user().id().update(user);
        }
    });
    // Transaction is committed when the closure returns
}
```

</TabItem>
</Tabs>

See the [Procedures documentation](/functions/procedures) for more details on using procedures, including making HTTP requests to external services.

## Views - Read-Only Access

[Views](/functions/views) receive a `ViewContext` or `AnonymousViewContext` which provides read-only access to all tables (both public and private). They can query and iterate tables, but cannot insert, update, or delete rows.

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
spacetimedb.view(
  { name: 'findUsersByName', public: true },
  t.array(user.rowType),
  (ctx) => {
    // Can read and filter
    return Array.from(ctx.db.user.name.filter('Alice'));

    // Cannot insert, update, or delete
    // ctx.db.user.insert(...) // ❌ Method not available
  });
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
[SpacetimeDB.View(Name = "FindUsersByName", Public = true)]
public static List<User> FindUsersByName(ViewContext ctx)
{
    // Can read and filter
    return ctx.Db.User.Name.Filter("Alice").ToList();

    // Cannot insert, update, or delete
    // ctx.Db.User.Insert(...) // ❌ Method not available
}
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
#[spacetimedb::view(name = find_users_by_name, public)]
fn find_users_by_name(ctx: &ViewContext) -> Vec<User> {
    // Can read and filter
    ctx.db.user().name().filter("Alice").collect()

    // Cannot insert, update, or delete
    // ctx.db.user().insert(...) // ❌ Compile error
}
```

</TabItem>
</Tabs>

See the [Views documentation](/functions/views) for more details on defining and querying views.

## Using Views for Fine-Grained Access Control

While table visibility controls whether clients can access a table at all, views provide fine-grained control over which rows and columns clients can see. Views can read from private tables and expose only the data appropriate for each client.

:::note
Views can only access table data through indexed lookups, not by scanning all rows. This restriction ensures views remain performant. See the [Views documentation](/functions/views) for details.
:::

### Filtering Rows by Caller

Use views with `ViewContext` to return only the rows that belong to the caller. The view accesses the caller's identity through `ctx.sender` and uses it to look up rows via an index.

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
import { table, t, schema } from 'spacetimedb/server';

// Private table containing all messages
const message = table(
  { name: 'message' },  // Private by default
  {
    id: t.u64().primaryKey().autoInc(),
    sender: t.identity().index('btree'),
    recipient: t.identity().index('btree'),
    content: t.string(),
    timestamp: t.timestamp(),
  }
);

const spacetimedb = schema(message);

// Public view that only returns messages the caller can see
spacetimedb.view(
  { name: 'my_messages', public: true },
  t.array(message.rowType),
  (ctx) => {
    // Look up messages by index where caller is sender or recipient
    const sent = Array.from(ctx.db.message.sender.filter(ctx.sender));
    const received = Array.from(ctx.db.message.recipient.filter(ctx.sender));
    return [...sent, ...received];
  }
);
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
using SpacetimeDB;

public partial class Module 
{
    // Private table containing all messages
    [SpacetimeDB.Table(Name = "Message")]  // Private by default
    public partial struct Message
    {
        [SpacetimeDB.PrimaryKey]
        [SpacetimeDB.AutoInc]
        public ulong Id;
        [SpacetimeDB.Index.BTree]
        public Identity Sender;
        [SpacetimeDB.Index.BTree]
        public Identity Recipient;
        public string Content;
        public Timestamp Timestamp;
    }

    // Public view that only returns messages the caller can see
    [SpacetimeDB.View(Name = "MyMessages", Public = true)]
    public static List<Message> MyMessages(ViewContext ctx)
    {
        // Look up messages by index where caller is sender or recipient
        var sent = ctx.Db.Message.Sender.Filter(ctx.Sender).ToList();
        var received = ctx.Db.Message.Recipient.Filter(ctx.Sender).ToList();
        sent.AddRange(received);
        return sent;
    }


```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
use spacetimedb::{Identity, Timestamp, ViewContext};

// Private table containing all messages
#[spacetimedb::table(name = message)]  // Private by default
pub struct Message {
    #[primary_key]
    #[auto_inc]
    id: u64,
    #[index(btree)]
    sender: Identity,
    #[index(btree)]
    recipient: Identity,
    content: String,
    timestamp: Timestamp,
}

// Public view that only returns messages the caller can see
#[spacetimedb::view(name = my_messages, public)]
fn my_messages(ctx: &ViewContext) -> Vec<Message> {
    // Look up messages by index where caller is sender or recipient
    let sent: Vec<_> = ctx.db.message().sender().filter(&ctx.sender).collect();
    let received: Vec<_> = ctx.db.message().recipient().filter(&ctx.sender).collect();
    sent.into_iter().chain(received).collect()
}
```

</TabItem>
</Tabs>

Clients querying `my_messages` will only see their own messages, even though all messages are stored in the same table.

### Hiding Sensitive Columns

Use views to return a custom type that omits sensitive columns. The view reads from a table with sensitive data and returns a projection containing only the columns clients should see.

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
import {schema, t, table} from 'spacetimedb/server';

// Private table with sensitive data
const userAccount = table(
  { name: 'user_account' },  // Private by default
  {
    id: t.u64().primaryKey().autoInc(),
    identity: t.identity().unique(),
    username: t.string(),
    email: t.string(),
    passwordHash: t.string(),  // Sensitive
    apiKey: t.string(),        // Sensitive
    createdAt: t.timestamp(),
  }
);

const spacetimedb = schema(userAccount);

// Public type without sensitive columns
const publicUserProfile = t.row('PublicUserProfile', {
  id: t.u64(),
  username: t.string(),
  createdAt: t.timestamp(),
});

// Public view that returns the caller's profile without sensitive data
spacetimedb.view(
  { name: 'my_profile', public: true },
  t.option(publicUserProfile),
  (ctx) => {
    // Look up the caller's account by their identity (unique index)
    const user = ctx.db.userAccount.identity.find(ctx.sender);
    if (!user) return null;
    return {
      id: user.id,
      username: user.username,
      createdAt: user.createdAt,
      // email, passwordHash, and apiKey are not included
    };
  }
);
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
using SpacetimeDB;

public partial class Module
{
    // Private table with sensitive data
    [SpacetimeDB.Table(Name = "UserAccount")]  // Private by default
    public partial struct UserAccount
    {
        [SpacetimeDB.PrimaryKey]
        [SpacetimeDB.AutoInc]
        public ulong Id;
        [SpacetimeDB.Unique]
        public Identity Identity;
        public string Username;
        public string Email;
        public string PasswordHash;  // Sensitive
        public string ApiKey;        // Sensitive
        public Timestamp CreatedAt;
    }

    // Public type without sensitive columns
    [SpacetimeDB.Type]
        public partial struct PublicUserProfile
    {
        public ulong Id;
        public string Username;
        public Timestamp CreatedAt;
    }

    // Public view that returns the caller's profile without sensitive data
    [SpacetimeDB.View(Name = "MyProfile", Public = true)]
    public static PublicUserProfile? MyProfile(ViewContext ctx)
    {
        // Look up the caller's account by their identity (unique index)
        if (ctx.Db.UserAccount.Identity.Find(ctx.Sender) is not UserAccount user)
        {
            return null;
        }
        return new PublicUserProfile
        {
            Id = user.Id,
            Username = user.Username,
            CreatedAt = user.CreatedAt,
            // Email, PasswordHash, and ApiKey are not included
        };
    }
}
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
use spacetimedb::{SpacetimeType, ViewContext, Timestamp, Identity};

// Private table with sensitive data
#[spacetimedb::table(name = user_account)]  // Private by default
pub struct UserAccount {
    #[primary_key]
    #[auto_inc]
    id: u64,
    #[unique]
    identity: Identity,
    username: String,
    email: String,
    password_hash: String,  // Sensitive
    api_key: String,        // Sensitive
    created_at: Timestamp,
}

// Public type without sensitive columns
#[derive(SpacetimeType)]
pub struct PublicUserProfile {
    id: u64,
    username: String,
    created_at: Timestamp,
}

// Public view that returns the caller's profile without sensitive data
#[spacetimedb::view(name = my_profile, public)]
fn my_profile(ctx: &ViewContext) -> Option<PublicUserProfile> {
    // Look up the caller's account by their identity (unique index)
    let user = ctx.db.user_account().identity().find(&ctx.sender)?;
    Some(PublicUserProfile {
        id: user.id,
        username: user.username,
        created_at: user.created_at,
        // email, password_hash, and api_key are not included
    })
}
```

</TabItem>
</Tabs>

Clients can query `my_profile` to see their username and creation date, but never see their email address, password hash, or API key.

### Combining Both Techniques

Views can combine row filtering and column projection. This example returns team members who report to the caller, with salary information hidden:

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
import { table, t, schema } from 'spacetimedb/server';

// Private table with all employee data
const employee = table(
  {
    name: 'employee',
    indexes: [
      { name: 'idx_manager_id', algorithm: 'btree', columns: ['managerId'] },
    ],
  },
  {
    id: t.u64().primaryKey(),
    identity: t.identity().unique(),
    name: t.string(),
    department: t.string(),
    salary: t.u64(),           // Sensitive
    managerId: t.option(t.u64()),
  }
);

const spacetimedb = schema(employee);

// Public type for team members (no salary)
const teamMember = t.row('TeamMember', {
  id: t.u64(),
  name: t.string(),
  department: t.string(),
});

// View that returns only the caller's team members, without salary info
spacetimedb.view(
  { name: 'my_team', public: true },
  t.array(teamMember),
  (ctx) => {
    // Find the caller's employee record by identity (unique index)
    const me = ctx.db.employee.identity.find(ctx.sender);
    if (!me) return [];

    // Look up employees who report to the caller by manager_id index
    return Array.from(ctx.db.employee.idx_manager_id.filter(me.id)).map(emp => ({
      id: emp.id,
      name: emp.name,
      department: emp.department,
      // salary is not included
    }));
  }
);
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
using SpacetimeDB;

public partial class Module
{
    // Private table with all employee data
    [SpacetimeDB.Table(Name = "Employee")]
    public partial struct Employee
    {
        [SpacetimeDB.PrimaryKey]
        public ulong Id;
        [SpacetimeDB.Unique]
        public Identity Identity;
        public string Name;
        public string Department;
        public ulong Salary;           // Sensitive
        [SpacetimeDB.Index.BTree]
        public ulong? ManagerId;
    }

    // Public type for team members (no salary)
    [SpacetimeDB.Type]
    public partial struct TeamMember
    {
        public ulong Id;
        public string Name;
        public string Department;
    }

    // View that returns only the caller's team members, without salary info
    [SpacetimeDB.View(Name = "MyTeam", Public = true)]
    public static List<TeamMember> MyTeam(ViewContext ctx)
    {
        // Find the caller's employee record by identity (unique index)
        if (ctx.Db.Employee.Identity.Find(ctx.Sender) is not Employee me)
        {
            return new List<TeamMember>();
        }

        // Look up employees who report to the caller by ManagerId index
        return ctx.Db.Employee.ManagerId.Filter(me.Id)
            .Select(emp => new TeamMember
            {
                Id = emp.Id,
                Name = emp.Name,
                Department = emp.Department,
                // Salary is not included
            })
            .ToList();
    }
}
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
use spacetimedb::{SpacetimeType, Identity, ViewContext};

// Private table with all employee data
#[spacetimedb::table(name = employee)]
pub struct Employee {
    #[primary_key]
    id: u64,
    #[unique]
    identity: Identity,
    name: String,
    department: String,
    salary: u64,           // Sensitive
    #[index(btree)]
    manager_id: Option<u64>,
}

// Public type for team members (no salary)
#[derive(SpacetimeType)]
pub struct TeamMember {
    id: u64,
    name: String,
    department: String,
}

// View that returns only the caller's team members, without salary info
#[spacetimedb::view(name = my_team, public)]
fn my_team(ctx: &ViewContext) -> Vec<TeamMember> {
    // Find the caller's employee record by identity (unique index)
    let Some(me) = ctx.db.employee().identity().find(&ctx.sender) else {
        return vec![];
    };

    // Look up employees who report to the caller by manager_id index
    ctx.db.employee().manager_id().filter(&Some(me.id))
        .map(|emp| TeamMember {
            id: emp.id,
            name: emp.name,
            department: emp.department,
            // salary is not included
        })
        .collect()
}
```

</TabItem>
</Tabs>

## Client Access - Read-Only Access

Clients connect to databases and can access public tables and views through subscriptions and queries. They cannot access private tables directly. See the [Subscriptions documentation](/subscriptions) for details on client-side table access.

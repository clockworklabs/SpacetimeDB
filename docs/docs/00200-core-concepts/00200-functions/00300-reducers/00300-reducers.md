---
title: Overview
slug: /functions/reducers
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';
import { CppModuleVersionNotice } from "@site/src/components/CppModuleVersionNotice";


Reducers are functions that modify database state in response to client requests or system events. They are the **only** way to mutate tables in SpacetimeDB - all database changes must go through reducers.

## Defining Reducers

Reducers are defined in your module code and automatically exposed as callable functions to connected clients.

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

Use the `spacetimedb.reducer` function:

```typescript
import { schema, table, t } from 'spacetimedb/server';

export const create_user = spacetimedb.reducer({ name: t.string(), email: t.string() }, (ctx, { name, email }) => {
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
<TabItem value="cpp" label="C++">

<CppModuleVersionNotice />

Use the `SPACETIMEDB_REDUCER` macro on a function:

```cpp
#include <spacetimedb.h>
using namespace SpacetimeDB;

SPACETIMEDB_REDUCER(create_user, ReducerContext ctx, std::string name, std::string email) {
    // Validate input
    if (name.empty()) {
        return Err("Name cannot be empty");
    }
    
    // Modify tables
    User user{0, name, email};  // 0 for id - auto-increment will assign
    ctx.db[user].insert(user);
    
    return Ok();
}
```

Reducers must take `ReducerContext ctx` as their first parameter. Additional parameters can be any registered types. Reducers return `ReducerResult` (which is `Outcome<void>`): use `Ok()` on success or `Err(message)` on error for convenience.

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
<TabItem value="cpp" label="C++">

```cpp
ctx.db[user].insert(User{
    0,  // auto-increment will assign
    "Alice",
    "alice@example.com"
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
<TabItem value="cpp" label="C++">

```cpp
if (auto user = ctx.db[user_id].find(123)) {
    LOG_INFO("Found: " + user->name);
}

auto by_email = ctx.db[user_email].find("alice@example.com");
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
<TabItem value="cpp" label="C++">

```cpp
for (const auto& user : ctx.db[user_name].filter("Alice")) {
    LOG_INFO("User " + std::to_string(user.id) + ": " + user.email);
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
<TabItem value="cpp" label="C++">

```cpp
if (auto user = ctx.db[user_id].find(123)) {
    user->name = "Bob";
    ctx.db[user_id].update(*user);
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
<TabItem value="cpp" label="C++">

```cpp
// Delete by primary key
ctx.db[user_id].delete_by_key(123);

// Delete all matching an indexed column
uint32_t deleted = 0;
for (const auto& user : ctx.db[user_name].filter("Alice")) {
    ctx.db[user_id].delete_by_key(user.id);
    deleted++;
}
LOG_INFO("Deleted " + std::to_string(deleted) + " row(s)");
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
<TabItem value="cpp" label="C++">

```cpp
for (const auto& user : ctx.db[user]) {
    LOG_INFO(std::to_string(user.id) + ": " + user.name);
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
<TabItem value="cpp" label="C++">

```cpp
auto total = ctx.db[user].count();
LOG_INFO("Total users: " + std::to_string(total));
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

:::warning Global and Static Variables Are Undefined Behavior
Relying on global variables, static variables, or module-level state to persist across reducer calls is **undefined behavior**. SpacetimeDB does not guarantee that values stored in these locations will be available in subsequent reducer invocations.

This is undefined for several reasons:

1. **Fresh execution environments.** SpacetimeDB may run each reducer in a fresh WASM or JS instance.
2. **Module updates.** Publishing a new module creates a fresh execution environment. This is necessary for hot-swapping modules while transactions are in flight.
3. **Concurrent execution.** SpacetimeDB reserves the right to execute multiple reducers concurrently in separate execution environments (e.g., with MVCC).
4. **Crash recovery.** Instance memory is not persisted across restarts.
5. **Non-transactional updates.** If you modify global state and then roll back the transaction, the modified value may remain for subsequent transactions.
6. **Replay safety.** If a serializability anomaly is detected, SpacetimeDB may re-execute your reducer with the same arguments, causing modifications to global state to occur multiple times.

Reducers are designed to be free of side effects. They should only modify tables. Always store state in tables to ensure correctness and durability.

```rust
// ❌ Undefined behavior: may or may not persist or correctly update across reducer calls
static mut COUNTER: u64 = 0;

// ✅ Store state in a table instead
#[spacetimedb::table(accessor = counter)]
pub struct Counter {
    #[primary_key]
    id: u32,
    value: u64,
}
```
:::

## Scheduling Procedures

Reducers cannot call procedures directly (procedures may have side effects incompatible with transactional execution). Instead, schedule a procedure to run by inserting into a [schedule table](/tables/schedule-tables):

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
import { schema, t, table, SenderError } from 'spacetimedb/server';

// Define a schedule table for the procedure
const fetchSchedule = table(
  { name: 'fetch_schedule', scheduled: (): any => fetch_external_data },
  {
    scheduled_id: t.u64().primaryKey().autoInc(),
    scheduled_at: t.scheduleAt(),
    url: t.string(),
  }
);

const spacetimedb = schema({ fetchSchedule });
export default spacetimedb;

// The procedure to be scheduled
const fetchExternalData = spacetimedb.procedure(
  'fetch_external_data',
  { arg: fetchSchedule.rowType },
  t.unit(),
  (ctx, { arg }) => {
    const response = ctx.http.fetch(arg.url);
    // Process response...
    return {};
  }
);

// From a reducer, schedule the procedure by inserting into the schedule table
const queueFetch = spacetimedb.reducer('queue_fetch', { url: t.string() }, (ctx, { url }) => {
  ctx.db.fetchSchedule.insert({
    scheduled_id: 0n,
    scheduled_at: ScheduleAt.interval(0n), // Run immediately
    url,
  });
});
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
#pragma warning disable STDB_UNSTABLE
using SpacetimeDB;

public partial class Module
{
    [SpacetimeDB.Table(Accessor = "FetchSchedule", Scheduled = "FetchExternalData", ScheduledAt = "ScheduledAt")]
    public partial struct FetchSchedule
    {
        [SpacetimeDB.PrimaryKey]
        [SpacetimeDB.AutoInc]
        public ulong ScheduledId;
        public ScheduleAt ScheduledAt;
        public string Url;
    }

    [SpacetimeDB.Procedure]
    public static void FetchExternalData(ProcedureContext ctx, FetchSchedule schedule)
    {
        var result = ctx.Http.Get(schedule.Url);
        if (result is Result<HttpResponse, HttpError>.OkR(var response))
        {
            // Process response...
        }
    }

    // From a reducer, schedule the procedure
    [SpacetimeDB.Reducer]
    public static void QueueFetch(ReducerContext ctx, string url)
    {
        ctx.Db.FetchSchedule.Insert(new FetchSchedule
        {
            ScheduledId = 0,
            ScheduledAt = new ScheduleAt.Interval(TimeSpan.Zero),
            Url = url,
        });
    }
}
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
use spacetimedb::{ScheduleAt, ReducerContext, ProcedureContext, Table};
use std::time::Duration;

#[spacetimedb::table(accessor = fetch_schedule, scheduled(fetch_external_data))]
pub struct FetchSchedule {
    #[primary_key]
    #[auto_inc]
    scheduled_id: u64,
    scheduled_at: ScheduleAt,
    url: String,
}

#[spacetimedb::procedure]
fn fetch_external_data(ctx: &mut ProcedureContext, schedule: FetchSchedule) {
    if let Ok(response) = ctx.http.get(&schedule.url) {
        // Process response...
    }
}

// From a reducer, schedule the procedure
#[spacetimedb::reducer]
fn queue_fetch(ctx: &ReducerContext, url: String) {
    ctx.db.fetch_schedule().insert(FetchSchedule {
        scheduled_id: 0,
        scheduled_at: ScheduleAt::Interval(Duration::ZERO.into()),
        url,
    });
}
```

</TabItem>
<TabItem value="cpp" label="C++">

```cpp
#define SPACETIMEDB_UNSTABLE_FEATURES
#include <spacetimedb.h>
using namespace SpacetimeDB;

// Define a table to store scheduled tasks
struct FetchSchedule {
    uint64_t scheduled_id;
    ScheduleAt scheduled_at;
    std::string url;
};
SPACETIMEDB_STRUCT(FetchSchedule, scheduled_id, scheduled_at, url);
SPACETIMEDB_TABLE(FetchSchedule, fetch_schedule, Private);
FIELD_PrimaryKeyAutoInc(fetch_schedule, scheduled_id);

// Register the table for scheduling (column 1 = scheduled_at field, 0-based index)
SPACETIMEDB_SCHEDULE(fetch_schedule, 1, fetch_external_data);

// The procedure to be scheduled - called automatically when the time arrives
SPACETIMEDB_PROCEDURE(uint32_t, fetch_external_data, ProcedureContext ctx, FetchSchedule schedule) {
    LOG_INFO("Fetching data from: " + schedule.url);
    // Process response...
    return 0;  // Success
}

// From a reducer, schedule the procedure by inserting into the schedule table
SPACETIMEDB_REDUCER(queue_fetch, ReducerContext ctx, std::string url) {
    auto scheduled_at = ScheduleAt(TimeDuration::from_seconds(0));  // Run immediately
    FetchSchedule fetch_task{
        0,                // scheduled_id - auto-increment will assign
        scheduled_at,     // When to execute
        url
    };
    ctx.db[fetch_schedule].insert(fetch_task);
    LOG_INFO("Fetch scheduled for URL: " + url);
    return Ok();
}
```

</TabItem>
</Tabs>

See [Schedule Tables](/tables/schedule-tables) for more scheduling options.

## Next Steps

- Learn about [Tables](/tables) to understand data storage
- Explore [Procedures](/functions/procedures) for side effects beyond the database
- Review [Subscriptions](/subscriptions) for real-time client updates

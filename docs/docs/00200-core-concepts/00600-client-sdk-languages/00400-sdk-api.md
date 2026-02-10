---
title: SDK API Overview
slug: /sdks/api
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';


The SpacetimeDB client SDKs provide a comprehensive API for interacting with your [database](/databases). After [generating client bindings](/sdks/codegen) and [establishing a connection](/sdks/connection), you can query data, invoke server functions, and observe real-time changes.

This page describes the core concepts and patterns that apply across all client SDKs. For language-specific details and complete API documentation, see the reference pages for [Rust](/sdks/rust), [C#](/sdks/c-sharp), [TypeScript](/sdks/typescript), or [Unreal Engine](/sdks/unreal).

## Prerequisites

Before using the SDK API, you must:

1. [Generate client bindings](/sdks/codegen) using `spacetime generate`
2. [Create a connection](/sdks/connection) to your database

## Subscriptions

Subscriptions replicate a subset of the database to your client, maintaining a local cache that automatically updates as the server state changes. Clients should subscribe to the data they need, then query the local cache.

### Creating Subscriptions

Subscribe to tables or queries using SQL:

<Tabs groupId="client-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
// Subscribe with callbacks
conn
  .subscriptionBuilder()
  .onApplied(ctx => {
    console.log(`Subscription ready with ${ctx.db.User.count()} users`);
  })
  .onError((ctx, error) => {
    console.error(`Subscription failed: ${error}`);
  })
  .subscribe(['SELECT * FROM user']);
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
// Subscribe with callbacks
conn.SubscriptionBuilder()
    .OnApplied(ctx =>
    {
        Console.WriteLine($"Subscription ready with {ctx.Db.User.Count()} users");
    })
    .OnError((ctx, error) =>
    {
        Console.WriteLine($"Subscription failed: {error}");
    })
    .Subscribe("SELECT * FROM user");
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
// Subscribe with callbacks
conn.subscription_builder()
    .on_applied(|ctx| {
        println!("Subscription ready with {} users", ctx.db().user().count());
    })
    .on_error(|ctx, error| {
        eprintln!("Subscription failed: {}", error);
    })
    .subscribe(["SELECT * FROM user"]);
```

</TabItem>
<TabItem value="unreal" label="Unreal">

```cpp
// Create and bind delegates
FOnSubscriptionApplied AppliedDelegate;
AppliedDelegate.BindDynamic(this, &AMyActor::OnSubscriptionApplied);

FOnSubscriptionError ErrorDelegate;
ErrorDelegate.BindDynamic(this, &AMyActor::OnSubscriptionError);

// Subscribe with callbacks
TArray<FString> Queries = { TEXT("SELECT * FROM user") };
Conn->SubscriptionBuilder()
    ->OnApplied(AppliedDelegate)
    ->OnError(ErrorDelegate)
    ->Subscribe(Queries);

// Callback functions (must be UFUNCTION)
UFUNCTION()
void OnSubscriptionApplied(const FSubscriptionEventContext& Ctx)
{
    int32 UserCount = Ctx.Db->User->Count();
    UE_LOG(LogTemp, Log, TEXT("Subscription ready with %d users"), UserCount);
}

UFUNCTION()
void OnSubscriptionError(const FErrorContext& Ctx)
{
    UE_LOG(LogTemp, Error, TEXT("Subscription failed: %s"), *Ctx.Error);
}
```

</TabItem>
</Tabs>

See the [Subscriptions documentation](/subscriptions) for detailed information on subscription queries and semantics. Subscribe to [tables](/tables) for row data, or to [views](/functions/views) for computed query results.

### Querying the Local Cache

Once subscribed, query the local cache without network round-trips:

<Tabs groupId="client-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
// Iterate all cached rows
for (const user of conn.db.user.iter()) {
  console.log(`${user.id}: ${user.name}`);
}

// Count cached rows
const userCount = conn.db.user.count();

// Find by unique column (if indexed)
const user = conn.db.user.name.find('Alice');
if (user) {
  console.log(`Found: ${user.email}`);
}

// Filter cached rows
const adminUsers = [...conn.db.user.iter()].filter(u => u.isAdmin);
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
// Iterate all cached rows
foreach (var user in conn.Db.User.Iter())
{
    Console.WriteLine($"{user.Id}: {user.Name}");
}

// Count cached rows
var userCount = conn.Db.User.Count;

// Find by unique column (if indexed)
var user = conn.Db.User.Name.Find("Alice");
if (user != null)
{
    Console.WriteLine($"Found: {user.Email}");
}

// Filter cached rows (using LINQ)
var adminUsers = conn.Db.User.Iter()
    .Where(u => u.IsAdmin)
    .ToList();
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
// Iterate all cached rows
for user in conn.db().user().iter() {
    println!("{}: {}", user.id, user.name);
}

// Count cached rows
let user_count = conn.db().user().count();

// Find by unique column (if indexed)
if let Some(user) = conn.db().user().name().find("Alice") {
    println!("Found: {}", user.email);
}

// Filter cached rows
let admin_users: Vec<_> = conn.db().user()
    .iter()
    .filter(|u| u.is_admin)
    .collect();
```

</TabItem>
<TabItem value="unreal" label="Unreal">

```cpp
// Iterate all cached rows
TArray<FUserType> Users = Conn->Db->User->Iter();
for (const FUserType& User : Users)
{
    UE_LOG(LogTemp, Log, TEXT("%lld: %s"), User.Id, *User.Name);
}

// Count cached rows
int32 UserCount = Conn->Db->User->Count();

// Find by unique column (if indexed)
FUserType User = Conn->Db->User->Name->Find(TEXT("Alice"));
if (!User.Name.IsEmpty())
{
    UE_LOG(LogTemp, Log, TEXT("Found: %s"), *User.Email);
}

// Filter cached rows
TArray<FUserType> AllUsers = Conn->Db->User->Iter();
TArray<FUserType> AdminUsers;
for (const FUserType& User : AllUsers)
{
    if (User.IsAdmin)
    {
        AdminUsers.Add(User);
    }
}
```

</TabItem>
</Tabs>

### Row Update Callbacks

Register callbacks to observe insertions, updates, and deletions in the local cache:

<Tabs groupId="client-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
// Called when a row is inserted
conn.db.User.onInsert((ctx, user) => {
  console.log(`User inserted: ${user.name}`);
});

// Called when a row is updated
conn.db.User.onUpdate((ctx, oldUser, newUser) => {
  console.log(`User ${newUser.id} updated: ${oldUser.name} -> ${newUser.name}`);
});

// Called when a row is deleted
conn.db.User.onDelete((ctx, user) => {
  console.log(`User deleted: ${user.name}`);
});
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
// Called when a row is inserted
conn.Db.User.OnInsert += (ctx, user) =>
{
    Console.WriteLine($"User inserted: {user.Name}");
};

// Called when a row is updated
conn.Db.User.OnUpdate += (ctx, oldUser, newUser) =>
{
    Console.WriteLine($"User {newUser.Id} updated: {oldUser.Name} -> {newUser.Name}");
};

// Called when a row is deleted
conn.Db.User.OnDelete += (ctx, user) =>
{
    Console.WriteLine($"User deleted: {user.Name}");
};
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
// Called when a row is inserted
conn.db().user().on_insert(|ctx, user| {
    println!("User inserted: {}", user.name);
});

// Called when a row is updated
conn.db().user().on_update(|ctx, old_user, new_user| {
    println!("User {} updated: {} -> {}", 
        new_user.id, old_user.name, new_user.name);
});

// Called when a row is deleted
conn.db().user().on_delete(|ctx, user| {
    println!("User deleted: {}", user.name);
});
```

</TabItem>
<TabItem value="unreal" label="Unreal">

```cpp
// Called when a row is inserted
Conn->Db->User->OnInsert.AddDynamic(this, &AMyActor::OnUserInsert);

// Called when a row is updated
Conn->Db->User->OnUpdate.AddDynamic(this, &AMyActor::OnUserUpdate);

// Called when a row is deleted
Conn->Db->User->OnDelete.AddDynamic(this, &AMyActor::OnUserDelete);

// Callback functions (must be UFUNCTION)
UFUNCTION()
void OnUserInsert(const FEventContext& Context, const FUserType& User)
{
    UE_LOG(LogTemp, Log, TEXT("User inserted: %s"), *User.Name);
}

UFUNCTION()
void OnUserUpdate(const FEventContext& Context, const FUserType& OldUser, const FUserType& NewUser)
{
    UE_LOG(LogTemp, Log, TEXT("User %lld updated: %s -> %s"),
        NewUser.Id, *OldUser.Name, *NewUser.Name);
}

UFUNCTION()
void OnUserDelete(const FEventContext& Context, const FUserType& User)
{
    UE_LOG(LogTemp, Log, TEXT("User deleted: %s"), *User.Name);
}
```

</TabItem>
</Tabs>

These callbacks fire whenever the local cache changes due to subscription updates, providing real-time reactivity.

## Complete Examples

For complete working examples, see the language-specific reference pages:

- [Rust SDK Reference](/sdks/rust) - Comprehensive Rust API documentation
- [C# SDK Reference](/sdks/c-sharp) - C# and Unity-specific patterns
- [TypeScript SDK Reference](/sdks/typescript) - Browser and Node.js examples
- [Unreal SDK Reference](/sdks/unreal) - Unreal Engine C++ and Blueprint patterns

## Related Documentation

- [Generating Client Bindings](/sdks/codegen) - How to generate type-safe bindings
- [Connecting to SpacetimeDB](/sdks/connection) - Connection setup and lifecycle
- [Subscriptions](/subscriptions) - Detailed subscription semantics
- [Reducers](/functions/reducers) - Server-side transactional functions
- [Procedures](/functions/procedures) - Server-side functions with external capabilities
- [Tables](/tables) - Database schema and storage

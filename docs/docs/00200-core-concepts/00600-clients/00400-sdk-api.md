---
title: SDK API Overview
slug: /clients/api
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';


The SpacetimeDB client SDKs provide a comprehensive API for interacting with your [database](../00100-databases.md). After [generating client bindings](./00200-codegen.md) and [establishing a connection](./00300-connection.md), you can query data, invoke server functions, and observe real-time changes.

This page describes the core concepts and patterns that apply across all client SDKs. For language-specific details and complete API documentation, see the reference pages for [Rust](./00500-rust-reference.md), [C#](./00600-csharp-reference.md), [TypeScript](./00700-typescript-reference.md), or [Unreal Engine](./00800-unreal-reference.md).

## Prerequisites

Before using the SDK API, you must:

1. [Generate client bindings](./00200-codegen.md) using `spacetime generate`
2. [Create a connection](./00300-connection.md) to your database

## Subscriptions

Subscriptions replicate a subset of the database to your client, maintaining a local cache that automatically updates as the server state changes. Clients should subscribe to the data they need, then query the local cache.

Typical flow:

1. Create a subscription with the SDK builder API
2. Wait for `onApplied`/`OnApplied` to know initial rows are present
3. Read from the local cache and register callbacks
4. Unsubscribe when the data is no longer needed

For lifecycle guarantees and semantics, see [Subscriptions](../00400-subscriptions.md) and [Subscription Semantics](../00400-subscriptions/00200-subscription-semantics.md).

### Example

<Tabs groupId="client-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
import { tables } from './module_bindings';

const handle = conn
  .subscriptionBuilder()
  .onApplied(ctx => {
    console.log(`Ready with ${ctx.db.user.count()} users`);
  })
  .onError((ctx, error) => {
    console.error(`Subscription failed: ${error}`);
  })
  .subscribe([tables.user.where(r => r.online.eq(true))]);
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
var handle = conn
    .SubscriptionBuilder()
    .OnApplied(ctx =>
    {
        Console.WriteLine($"Ready with {ctx.Db.User.Count} users");
    })
    .OnError((ctx, error) =>
    {
        Console.WriteLine($"Subscription failed: {error}");
    })
    .AddQuery(q => q.From.User())
    .Subscribe();
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
let handle = conn
    .subscription_builder()
    .on_applied(|ctx| {
        println!("Ready with {} users", ctx.db().user().count());
    })
    .on_error(|_ctx, error| {
        eprintln!("Subscription failed: {}", error);
    })
    .add_query(|q| q.from.user())
    .subscribe();
```

</TabItem>
<TabItem value="unreal" label="Unreal">

```cpp
TArray<FString> Queries = { TEXT("SELECT * FROM user") };

USubscriptionHandle* Handle = Conn->SubscriptionBuilder()
    ->OnApplied(AppliedDelegate)
    ->OnError(ErrorDelegate)
    ->Subscribe(Queries);
```

</TabItem>
</Tabs>

## Querying the Local Cache

After a subscription is applied, reads are local and do not require network round-trips.

<Tabs groupId="client-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
const userCount = conn.db.user.count();
const user = conn.db.user.name.find('Alice');
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
var userCount = conn.Db.User.Count;
var user = conn.Db.User.Name.Find("Alice");
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
let user_count = conn.db().user().count();
let user = conn.db().user().name().find("Alice");
```

</TabItem>
<TabItem value="unreal" label="Unreal">

```cpp
int32 UserCount = Conn->Db->User->Count();
FUserType User = Conn->Db->User->Name->Find(TEXT("Alice"));
```

</TabItem>
</Tabs>

## Reacting to Cache Changes

Use row callbacks to react when subscribed rows are inserted, updated, or deleted.

<Tabs groupId="client-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
conn.db.user.onInsert((ctx, row) => {});
conn.db.user.onUpdate((ctx, oldRow, newRow) => {});
conn.db.user.onDelete((ctx, row) => {});
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
conn.Db.User.OnInsert += (ctx, row) => {};
conn.Db.User.OnUpdate += (ctx, oldRow, newRow) => {};
conn.Db.User.OnDelete += (ctx, row) => {};
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
conn.db().user().on_insert(|ctx, row| {});
conn.db().user().on_update(|ctx, old_row, new_row| {});
conn.db().user().on_delete(|ctx, row| {});
```

</TabItem>
<TabItem value="unreal" label="Unreal">

```cpp
Conn->Db->User->OnInsert.AddDynamic(this, &AMyActor::OnUserInsert);
Conn->Db->User->OnUpdate.AddDynamic(this, &AMyActor::OnUserUpdate);
Conn->Db->User->OnDelete.AddDynamic(this, &AMyActor::OnUserDelete);
```

</TabItem>
</Tabs>

## Canonical API References

- [Subscriptions](../00400-subscriptions.md) - Lifecycle, usage patterns, and semantics
- [Subscription Semantics](../00400-subscriptions/00200-subscription-semantics.md) - Detailed consistency and ordering behavior
- [TypeScript Reference](./00700-typescript-reference.md#subscribe-to-queries) - `SubscriptionBuilder`, `SubscriptionHandle`, query builder API
- [C# Reference](./00600-csharp-reference.md#subscribe-to-queries) - `SubscriptionBuilder`, `SubscriptionHandle`
- [C# Query Builder API](./00600-csharp-reference.md#query-builder-api) - Typed subscription query builder
- [Rust Reference](./00500-rust-reference.md#subscribe-to-queries) - `SubscriptionBuilder`, `SubscriptionHandle`
- [Rust Query Builder API](./00500-rust-reference.md#query-builder-api) - Typed subscription query builder
- [Unreal Reference](./00800-unreal-reference.md#subscriptions) - Unreal subscription APIs

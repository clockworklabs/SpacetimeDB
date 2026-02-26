---
title: Error Handling
slug: /functions/reducers/error-handling
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';
import { CppModuleVersionNotice } from "@site/src/components/CppModuleVersionNotice";


## Error Handling

Reducers distinguish between two types of errors:

### Sender Errors

Errors caused by invalid client input. These are expected and should be handled gracefully.

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

Throw a `SenderError`:

```typescript
import { SenderError } from 'spacetimedb/server';

export const transfer_credits = spacetimedb.reducer(
  { to_user: t.u64(), amount: t.u32() },
  (ctx, { to_user, amount }) => {
    const fromUser = ctx.db.users.id.find(ctx.sender);
    if (!fromUser) {
      throw new SenderError('User not found');
    }
    
    if (fromUser.credits < amount) {
      throw new SenderError('Insufficient credits');
    }
    
    // ... perform transfer
  }
);

// Alternative: return error object
export const transfer_credits = spacetimedb.reducer(
  { to_user: t.u64(), amount: t.u32() },
  (ctx, { to_user, amount }) => {
    // ...validation...
    if (error) {
      return { tag: 'err', value: 'Insufficient credits' };
    }
    // ...
  }
);
```

</TabItem>
<TabItem value="csharp" label="C#">

Throw an exception:

```csharp
[SpacetimeDB.Reducer]
public static void TransferCredits(ReducerContext ctx, ulong toUser, uint amount)
{
    var fromUser = ctx.Db.User.Id.Find(ctx.Sender);
    if (fromUser == null)
    {
        throw new InvalidOperationException("User not found");
    }
    
    if (fromUser.Value.Credits < amount)
    {
        throw new InvalidOperationException("Insufficient credits");
    }
    
    // ... perform transfer
}
```

</TabItem>
<TabItem value="rust" label="Rust">

Return an error:

```rust
#[reducer]
pub fn transfer_credits(
    ctx: &ReducerContext,
    to_user: u64,
    amount: u32
) -> Result<(), String> {
    let from_balance = ctx.db.users().id().find(ctx.sender.identity)
        .ok_or("User not found");
    
    if from_balance.credits < amount {
        return Err("Insufficient credits".to_string());
    }
    
    // ... perform transfer
    Ok(())
}
```

</TabItem>
  <TabItem value="cpp" label="C++">

<CppModuleVersionNotice />

  ```cpp
  #include <spacetimedb.h>
  using namespace SpacetimeDB;

  struct User {
    Identity identity;
    uint32_t credits;
  };
  SPACETIMEDB_STRUCT(User, identity, credits);
  SPACETIMEDB_TABLE(User, users, Private);
  FIELD_PrimaryKey(users, identity);

  SPACETIMEDB_REDUCER(transfer_credits, ReducerContext ctx, Identity to_user, uint32_t amount) {
    auto from_user = ctx.db[users_identity].find(ctx.sender);
    if (!from_user) {
      return Err("User not found");
    }
    
    if (from_user->credits < amount) {
      return Err("Insufficient credits");
    }
    
    // ... perform transfer
    return Ok();
  }
  ```

  </TabItem>
</Tabs>

### Programmer Errors

Unexpected errors caused by bugs in module code. These should be fixed by the developer.

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

Regular errors (not `SenderError`):

```typescript
export const process_data = spacetimedb.reducer(
  { data: t.array(t.u8()) },
  (ctx, { data }) => {
    // Regular Error indicates a bug
    if (data.length === 0) {
      throw new Error('Unexpected empty data');
    }
    
    // ...
  }
);
```

</TabItem>
<TabItem value="csharp" label="C#">

Uncaught exceptions:

```csharp
[SpacetimeDB.Reducer]
public static void ProcessData(ReducerContext ctx, byte[] data)
{
    // This indicates a bug
    Debug.Assert(data.Length > 0, "Unexpected empty data");
    
    // Uncaught exception indicates a bug
    var parsed = ParseData(data); // May throw
    
    // ...
}
```

</TabItem>
<TabItem value="rust" label="Rust">

Panics or uncaught errors:

```rust
#[reducer]
pub fn process_data(ctx: &ReducerContext, data: Vec<u8>) -> Result<(), String> {
    // This panic indicates a bug
    assert!(data.len() > 0, "Unexpected empty data");
    
    // Uncaught Result indicates a bug
    let parsed = parse_data(&data).expect("Failed to parse data");
    
    // ...
    Ok(())
}
```

</TabItem>
  <TabItem value="cpp" label="C++">

  ```cpp
  #include <spacetimedb.h>
  #include <cassert>
  using namespace SpacetimeDB;

  SPACETIMEDB_REDUCER(process_data, ReducerContext ctx, Vec<uint8_t> data) {
    // This indicates a bug
    assert(!data.empty() && "Unexpected empty data");
  
    auto parsed = parse_data(data);
    if (!parsed) {
      LOG_PANIC("Failed to parse data");
    }
  
    // ...
    return Ok();
  }
  ```

  </TabItem>
</Tabs>

Programmer errors are logged and visible in your project dashboard. Consider setting up alerting to be notified when these occur.

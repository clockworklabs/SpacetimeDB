---
title: Error Handling
slug: /functions/reducers/error-handling
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';


## Error Handling

Reducers distinguish between two types of errors:

### Sender Errors

Errors caused by invalid client input. These are expected and should be handled gracefully.

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

Throw a `SenderError`:

```typescript
import { SenderError } from 'spacetimedb/server';

spacetimedb.reducer('transfer_credits', 
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
spacetimedb.reducer('transfer_credits',
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
    var fromUser = ctx.Db.users.Id.Find(ctx.Sender);
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
</Tabs>

### Programmer Errors

Unexpected errors caused by bugs in module code. These should be fixed by the developer.

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

Regular errors (not `SenderError`):

```typescript
spacetimedb.reducer('process_data', 
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
</Tabs>

Programmer errors are logged and visible in your project dashboard. Consider setting up alerting to be notified when these occur.

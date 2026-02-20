---
title: Logging
slug: /how-to/logging
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';


SpacetimeDB provides logging capabilities for debugging and monitoring your modules. Log messages are private to the database owner and are not visible to clients.

## Writing Logs

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

Use the standard `console` API to write logs from your reducers:

```typescript
import { spacetimedb } from 'spacetimedb/server';

spacetimedb.reducer('process_data', { value: t.u32() }, (ctx, { value }) => {
  console.log(`Processing data with value: ${value}`);
  
  if (value > 100) {
    console.warn(`Value ${value} exceeds threshold`);
  }
  
  if (value === 0) {
    console.error('Invalid value: 0');
    throw new Error('Value cannot be zero');
  }
  
  console.debug(`Debug information: ctx.sender = ${ctx.sender}`);
});
```

Available console methods:
- `console.error()` - Error messages
- `console.warn()` - Warning messages
- `console.log()` - Informational messages
- `console.debug()` - Debug messages

SpacetimeDB automatically routes these standard console calls through its internal logging system.

</TabItem>
<TabItem value="csharp" label="C#">

Use the `SpacetimeDB.Log` class to write logs from your reducers:

```csharp
using SpacetimeDB;

public static partial class Module
{
    [SpacetimeDB.Reducer]
    public static void ProcessData(ReducerContext ctx, uint value)
    {
        Log.Info($"Processing data with value: {value}");
        
        if (value > 100)
        {
            Log.Warn($"Value {value} exceeds threshold");
        }
        
        if (value == 0)
        {
            Log.Error("Invalid value: 0");
            throw new ArgumentException("Value cannot be zero");
        }
        
        Log.Debug($"Debug information: ctx.Sender = {ctx.Sender}");
    }
}
```

Available log methods:
- `Log.Error()` - Error messages
- `Log.Warn()` - Warning messages
- `Log.Info()` - Informational messages
- `Log.Debug()` - Debug messages
- `Log.Trace()` - Trace messages

</TabItem>
<TabItem value="rust" label="Rust">

Use the `log` crate to write logs from your reducers:

```rust
use spacetimedb::{reducer, ReducerContext};

#[reducer]
pub fn process_data(ctx: &ReducerContext, value: u32) -> Result<(), String> {
    log::info!("Processing data with value: {}", value);
    
    if value > 100 {
        log::warn!("Value {} exceeds threshold", value);
    }
    
    if value == 0 {
        log::error!("Invalid value: 0");
        return Err("Value cannot be zero".to_string());
    }
    
    log::debug!("Debug information: ctx.sender = {:?}", ctx.sender);
    
    Ok(())
}
```

Available log levels:
- `log::error!()` - Error messages
- `log::warn!()` - Warning messages
- `log::info!()` - Informational messages
- `log::debug!()` - Debug messages
- `log::trace!()` - Trace messages

</TabItem>
</Tabs>

## Viewing Logs

To view logs from your database, use the `spacetime logs` command:

```bash
spacetime logs <DATABASE_NAME>
```

### Following Logs in Real-Time

To stream logs as they're generated (similar to `tail -f`):

```bash
spacetime logs --follow <DATABASE_NAME>
```

### Filtering Logs

You can filter logs by various criteria:

```bash
# Show only errors
spacetime logs --level error <DATABASE_NAME>

# Show warnings and errors
spacetime logs --level warn <DATABASE_NAME>

# Show logs from a specific time range
spacetime logs --since "2023-01-01 00:00:00" <DATABASE_NAME>
```

For all log viewing options, see the [`spacetime logs` CLI reference](/cli-reference#spacetime-logs).

## Best Practices

### Log Levels

Use appropriate log levels for different types of messages:

- **Error**: Use for actual errors that prevent operations from completing
- **Warn**: Use for potentially problematic situations that don't prevent execution
- **Info**: Use for important application events (user actions, state changes)
- **Debug**: Use for detailed diagnostic information useful during development
- **Trace**: Use for very detailed diagnostic information (typically disabled in production)

### Performance Considerations

- Logging has minimal overhead, but excessive logging can impact performance
- Avoid logging in tight loops or high-frequency operations
- Consider using debug/trace logs for verbose output that can be filtered in production

### Privacy and Security

- Logs are only visible to the database owner, not to clients
- Avoid logging sensitive information like passwords or authentication tokens
- Be mindful of personally identifiable information (PII) in logs

### Structured Logging

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

Include relevant context in your log messages:

```typescript
spacetimedb.reducer('transfer_credits', 
  { to_user: t.u64(), amount: t.u32() },
  (ctx, { to_user, amount }) => {
    console.log(`Credit transfer: from=${ctx.sender}, to=${to_user}, amount=${amount}`);
    
    // ... transfer logic
  }
);
```

</TabItem>
<TabItem value="csharp" label="C#">

Include relevant context in your log messages:

```csharp
[SpacetimeDB.Reducer]
public static void TransferCredits(ReducerContext ctx, ulong toUser, uint amount)
{
    Log.Info($"Credit transfer: from={ctx.Sender}, to={toUser}, amount={amount}");
    
    // ... transfer logic
}
```

</TabItem>
<TabItem value="rust" label="Rust">

Use structured logging with key-value pairs for better log analysis:

```rust
use spacetimedb::log;

#[reducer]
pub fn transfer_credits(ctx: &ReducerContext, to_user: u64, amount: u32) -> Result<(), String> {
    log::info!(
        "Credit transfer: from={:?}, to={}, amount={}", 
        ctx.sender, 
        to_user, 
        amount
    );
    
    // ... transfer logic
    
    Ok(())
}
```

</TabItem>
</Tabs>

## Next Steps

- Learn about [Error Handling](/functions/reducers/error-handling) in reducers
- Explore the [CLI Reference](/cli-reference) for more logging options
- Set up monitoring and alerting for your production databases

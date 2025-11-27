---
slug: /how-to/reject-client-connections
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';

# Reject Client Connections

SpacetimeDB provides a way to disconnect a client during a client connection attempt.

<Tabs groupId="server-language" defaultValue="rust">
<TabItem value="rust" label="Rust">
In Rust, if we returned and error (or a panic) during the `client_connected` reducer, the client will be disconnected.

Here is a simple example where the server module throws an error for all incoming client connections.

```rust
#[reducer(client_connected)]
pub fn client_connected(_ctx: &ReducerContext) -> Result<(), String> {
    let client_is_rejected = true;
    if client_is_rejected {
        Err("The client connection was rejected. With our current code logic, all clients will be rejected.".to_string())
    } else {
        Ok(())
    }
}
```

Client behavior can vary by client type. For example:

- **C# clients**: Client disconnection behavior is currently undefined and will generate an error reading:
  `Disconnected abnormally: System.Net.WebSockets.WebSocketException (0x80004005): The remote party closed the WebSocket connection without completing the close handshake.`

- **Rust clients**: Client disconnection behavior is currently undefined and will generate an error reading:
  `Unable to send subscribe message: WS sender loop has dropped its recv channel: TrySendError { kind: Disconnected }`

- **TypeScript clients**: Client will receive an `Error connecting to SpacetimeDB:` and a `CloseEvent` with a code of 1006.

Regardless of the client type, from the rust server's perspective, the client will be disconnected and the server module's logs will contain an entry reading:
`ERROR: : The client connection was rejected. With our current code logic, all clients will be rejected.`
</TabItem>
<TabItem value="csharp" label="C#">
In C#, if we throw an exception during the `ClientConnected` reducer, the client will be disconnected.

Here is a simple example where the server module throws an error for all incoming client connections.

```csharp
[Reducer(ReducerKind.ClientConnected)]
// Called when a client connects to a SpacetimeDB database server
public static void ClientConnected(ReducerContext ctx)
{
    throw new Exception("The client connection was rejected. With our current code logic, all clients will be rejected.");
}
```

Client behavior can vary by client type. For example:

- **C# clients**: Client disconnection behavior is currently undefined and will generate an error reading:
  `Disconnected abnormally: System.Net.WebSockets.WebSocketException (0x80004005): The remote party closed the WebSocket connection without completing the close handshake.`

- **Rust clients**: Client will receive an `on_disconnected` event with no error message.

- **TypeScript clients**: Client will receive an `Error connecting to SpacetimeDB:` and a `CloseEvent` with a code of 1006.

Regardless of the client type, from the C# server's perspective, the client will be disconnected and the server module's logs will contain an entry reading:
`ERROR: : System.Exception: The client connection was rejected. With our current code logic, all clients will be rejected.`
</TabItem>
</Tabs>

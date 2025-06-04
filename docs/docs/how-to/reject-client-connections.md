# Rejecting Client Connections

SpacetimeDB provides a way to disconnect a client during a client connection attempt.
:::server-rust
In Rust, if we returned and error (or a panic) during the `client_connected` reducer, the client would be disconnected.

Here is a simple example where the server module throws an error for all incoming client connections.
```rust
#[reducer(client_connected)]
// Called when a client connects to a SpacetimeDB database server
pub fn client_connected(ctx: &ReducerContext) {
    return Err("All incoming client connections are being rejected. Normally you'd want to only throw an error after your validation logic indicates the client is not authorized.".into());
}
```
:::
:::server-csharp
In C#, if we throw an exception during the `ClientConnected` reducer, the client would be disconnected.

Here is a simple example where the server module throws an error for all incoming client connections.
```csharp
[Reducer(ReducerKind.ClientConnected)]
// Called when a client connects to a SpacetimeDB database server
public static void ClientConnected(ReducerContext ctx)
{
    throw new Exception("All incoming client connections are being rejected. Normally you'd want to only throw an error after your validation logic indicates the client is not authorized.");
}
```
:::

From the client's perspective, this disconnection behavior is currently undefined, but they will be disconnected from the server's perspective.
---
title: Troubleshooting
slug: /troubleshooting
---

This is a list of common problems when using SpacetimeDB and how to fix them.

## Credentials

### CLI login not accepted by server

If your CLI operations fail with the error `Invalid Token: InvalidSignature`, it's likely because you logged in with `--server-issued-login` from a different SpacetimeDB server. It's also possible your server's signing keys have changed, most likely due to the server having been reset.

Log out to remove the invalid token, then log in again. Logging in with GitHub will prevent this happening again, as those identities are portable and valid with any SpacetimeDB server, including Maincloud.

```bash
spacetime logout
spacetime login
```

:::danger
`spacetime logout` will discard your previous server-issued token, resulting in you no longer being able to manage any databases you previously published owned by that identity. If you still need access to the server-issued token, view it with `spacetime login show --token` and save it. You can then log back in with that token using `spacetime login --token`.
:::

### Client connection rejected

If SpacetimeDB rejects connections from your application's client, it's most likely because you're supplying a token that was issued by a different SpacetimeDB server, or has expired. Clear the invalid token:

| Client SDK                       | How to clear                                                                                                                                                                                            |
|----------------------------------|---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| Rust (native)                    | Delete the credentials file for your application from within `~/.spacetimedb_client_credentials`. Check where you construct the `spacetimedb_sdk::credentials::File` to find which file to delete.      |
| C# (native)                      | Delete the `auth_token=` line from your app's saved settings, by default located in `~/.spacetime_csharp_sdk/settings.ini`. Check your call to `AuthToken.Init` to find the directory and/or file name. |
| TypeScript, Rust or C# (browser) | Clear cookies.                                                                                                                                                                                          |
| Unity                            | Clear PlayerPrefs by selecting **Edit -> Clear All PlayerPrefs**                                                                                                                                        |
| Unreal                           | Delete the file `Saved/Config/WindowsEditor/GameUserSettings.ini` in your Unreal project.                                                                                                               |

## In Modules

### Identity seen by reducers never changes

In module code, `ctx.identity()` (or `Ctx.Identity()`) reads the identity of the database, not of the connected client. Call `ctx.sender()`/`Ctx.Sender()` instead to read the identity of the client which requested the current function call.

As of SpacetimeDB 2.3.0, the `identity` method is a  deprecated alias for `ctx.database_identity()`/`ctx.databaseIdentity()`/`Ctx.DatabaseIdentity()`.

### Code changes not taking effect

If you've made changes to your module code that aren't taking effect, you likely need to publish the new version of the code to the database. Use the `spacetime publish` CLI command, or run `spacetime dev`, which will watch your project and automatically call `spacetime publish` whenever you make changes.

## In Clients

### Connection completely unresponsive

If your `DbConnection` is completely unresponsive, with function calls never receiving responses and subscriptions never being applied, you may have one of several issues:

#### Connection rejected

Your client may have failed to connect to the remote server, or may have been disconnected soon after starting. Make sure you register `on_connect_error`/`onConnectError`/`OnConnectError` and `on_disconnect`/`onDisconnect`/`OnDisconnect` callbacks when building your `DbConnection`, and check if they're being invoked with errors.

#### Connection not advancing

You may need to advance your connection by calling one of the following methods:

| Client SDK          | Method                       | Description                                                                        |
|---------------------|------------------------------|------------------------------------------------------------------------------------|
| Rust (native only)  | `conn.run_threaded()`        | Spawn a thread to continuously advance the connection.                             |
| Rust (browser only) | `conn.run_background_task()` | Spawn a task to continuously advance the connection.                               |
| Rust                | `conn.run_async()`           | A `Future` which you can `await` or poll to advance the connection.                |
| Rust                | `conn.frame_tick()`          | In single-threaded games, call this every frame to advance the connection.         |
| C#                  | `Conn.FrameTick()`           | Call this every frame to advance the connection, or call it in a loop on a thread. |
| Unreal              | `Conn.FrameTick()`           | Call this every frame to advance the connection.                                   |
| TypeScript          | N/a                          | The TypeScript client SDK advances connections automatically.                      |

### Rows never appear

If rows from a table or view never appear and row callbacks are never invoked, you may need to add a subscription, or a subscription may have failed.

#### Add a subscription

In order for rows to be visible to a client, you must subscribe to a table or view. See [Subscriptions](../../00200-core-concepts/00400-subscriptions.md).

#### Check for subscription errors

If you've subscribed to a subscription but never see any rows, your subscription may have failed. Ensure you're registering an `on_error`/`onError`/`OnError` callback with the subscription builder, and check if it is invoked.

### Insert, update or delete callback not invoked when row changes

For a table or view with a primary key, a row change may be routed to either the on-insert, on-update or on-delete callback. Each change will only cause one of these callbacks to be invoked, so make sure you've registered all three of them. 

:::note
Insert, update and delete callbacks on the client don't always correspond 1-to-1 with calls to those methods in the module code.

The client may see an update event when you call delete followed by insert within the same reducer or transaction. 

The client may see an insert or a delete event when you call update but either the old or new version of the row doesn't match the client's subscriptions.
:::

For tables without primary keys, only the on-insert and on-delete callbacks will ever be invoked.

### Row seen by update callback is out of date

If it appears that an on-update callback is observing an old version of a row, you may have mixed up the parameters to that callback. Update callbacks take three parameter: an `EventContext` for interacting with the connection, the old version of the row, and the new version of the row. Make sure the function you register to the callback has 3 arguments:`(ctx, old, new)`.

### Serialization errors

If you see errors related to serialization, including unexpected EOFs, incorrect lengths or unrecognized tags, it's likely your generated `module_bindings` are out of date. Re-run `spacetime generate` to update them, or use `spacetime dev`, which will watch your module code and automatically call `spacetime generate` whenever you make changes.

### Reducers, procedures, views not visible to client

Scheduled reducers or procedures won't show up in client codegen. That's expected; clients can't directly invoke them.

If functions that aren't scheduled aren't showing up, or views aren't visible, it's likely your generated `module_bindings` are out of date. Re-run `spacetime generate` to update them, or use `spacetime dev`, which will watch your module code and automatically call `spacetime generate` whenever you make changes.

### Tables not visible to client

If a table isn't visible in client codegen, and you've already run `spacetime generate` or are using `spacetime dev`, the table may be private. Only tables marked public will be visible to clients and are available for subscriptions. See [Defining Tables](../../00200-core-concepts/00300-tables.md#defining-tables) for how to mark a table public.

### Compilation or type errors in generated `module_bindings`

If you see errors when compiling or type checking your autogenerated `module_bindings`, it's likely that your SpacetimeDB CLI version doesn't match the client SDK you're using in your client project.

Ensure that your CLI is up to date by running `spacetime version upgrade`, then check your version with `spacetime --version`.

Update to the latest version of the CLI package in your client dependencies:

| Client SDK | Dependency file       | Package name                       |
|------------|-----------------------|------------------------------------|
| Rust       | `Cargo.toml`          | `spacetimedb-sdk`                  |
| TypeScript | `package.json`        | `"spacetimedb"`                    |
| C#         | `<project>.csproj`    | `"SpacetimeDB.ClientSDK"`          |
| Unity      | Unity Package Manager | `com.clockworklabs.spacetimedbsdk` |
| Unreal     | `<Game>.Build.cs`     | `"SpacetimeDbSdk"`                 |

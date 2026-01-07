# Migration note

We are in the process of moving from the `com.clockworklabs.spacetimedbsdk` repo to the `sdks/csharp` subdirectory of [SpacetimeDB](https://github.com/clockworklabs/SpacetimeDB). **Any new changes should be made there**. The `com.clockworklabs.spacetimedbsdk` repo will only be updated on release. Apologies in advance for any sharp edges while the migration is in progress.

# Notes for maintainers

First, see the [user-facing docs](https://spacetimedb.com/docs/sdks/c-sharp).

## Developing against a local clone of SpacetimeDB
When developing against a local clone of SpacetimeDB, you'll need to ensure that the packages here can find an up-to-date version of the BSATN.Codegen and BSATN.Runtime packages from SpacetimeDB.

To develop against a local clone of SpacetimeDB at `../SpacetimeDB`, run the following command:

```sh
dotnet pack ../SpacetimeDB/crates/bindings-csharp/BSATN.Runtime && ./tools~/write-nuget-config.sh ../SpacetimeDB
```

This will create a (`.gitignore`d) `nuget.config` file that uses the local build of the package, instead of the package on NuGet.

You'll need to rerun this command whenever you update `BSATN.Codegen` or `BSATN.Runtime`.

## Internal architecture documentation

### Code generation
The SDK uses multiple layers of code generation:

- The `SpacetimeDB.BSATN.Codegen` library, a dependency of the SDK, whose source code lives in the SpacetimeDB repo [here](https://github.com/clockworklabs/SpacetimeDB/tree/master/crates/bindings-csharp). This library provides the `[SpacetimeDB.Type]` annotation. When the C# compiler encounters this annotation, it invokes the library to create [BSATN](https://spacetimedb.com/docs/bsatn) serialization code for the annotated type. This works for any compatible C# type. It does not involve any non-C# code, and the generated code is not visible in the filesystem.
- The codegen performed by the [`spacetimedb-codegen`](https://github.com/clockworklabs/SpacetimeDB/blob/master/crates/codegen/src/csharp.rs) Rust library, which also lives in the SpacetimeDB repo. This library is used by the `spacetime generate` CLI command. It generates code that talks to a SpacetimeDB module over the network. This code is what the user actually sees in the filesystem. The linked SpacetimeDB module can be written in any language, not just C#.

The code created by `spacetime generate` imports the SpacetimeDB SDK and extends its various classes to create a SpacetimeDB client. It also imports `SpacetimeDB.BSATN.Codegen` for its serialization needs.

See [`../../templates/quickstart-chat-c-sharp/module_bindings`](../../templates/quickstart-chat-c-sharp/module_bindings/) for an example of what `spacetime generate`d code looks like.

If you need to debug `SpacetimeDB.BSATN.Codegen`, you can set `<EmitCompilerGeneratedFiles>true</EmitCompilerGeneratedFiles>` in the `<PropertyGroup.` of your `.csproj`, build the project, and then look in `obj/Debug/.../generated`. This is where the C# compiler dumps Roslyn-generated code.

A client created with this SDK is at root a `DbConnection`. This class lives in the generated code in the file `SpacetimeDBClient.g.cs`. See e.g. [`../../templates/quickstart-chat-c-sharp/module_bindings/SpacetimeDBClient.g.cs`](../../templates/quickstart-chat-c-sharp/module_bindings/SpacetimeDBClient.g.cs).
(Note that `SpacetimeDBClient` is a vestigial name that should probably be retired at some point...)

`DbConnection` in the generated code inherits from `DbConnectionBase<...>` in the SDK code, which lives in [`src/SpacetimeDBClient.cs`](./src/SpacetimeDBClient.cs). This is a general pattern. Similar inheritance patterns are used for tables and indexes: the generated code defines a class that inherits most of its behavior from a class in the SDK.

We require that **a DbConnection is only accessed from a single thread**, which should call the `DbConnection.FrameTick()` method frequently. See [threading model](#threading-model), below.

In general, the generated code tries to implement as little functionality as possible, leaving most of the behavior to the SDK. This makes updates easier, since it is generally easier to update SDK code instead of the generated code.

When SDK code needs to refer to generated types, we have two options:
- Make the SDK code generic, and instantiate the generics in the generated code. E.g. `DbConnectionBase<...>` (SDK) is generic, but `DbConnection` (generated) is not.
- Or, just move the code entirely into the generated code. This was done for e.g. `ReducerEventContext`, which no longer lives in the SDK at all.

The most important generated types are `RemoteTables` -- also known as the **client cache** -- and `RemoteReducers`. `RemoteTables` stores the local view of subscribed data from the database. For a `DbConnection conn`, `conn.Db` is an instance of `RemoteTables`. `RemoteReducers` allows calling reducers on the server, and is accessible at `conn.Reducers`. Types are also generated for all server-side types referred to by tables or modules.

### Runtime Structure

Most of the core logic of the SDK lives in [`DbConnectionBase<...>`](./src/SpacetimeDBClient.cs). This handles:
- Spinning up background threads to talk to the network and parse messages
- Receiving updates, updating the client cache, and calling callbacks.

The user creates a single `DbConnection` (which inherits from `DbConnectionBase<...>`), and from this `DbConnection` creates some number of `SubscriptionHandle`s using the `SubscriptionBuilder` class. Each subscription consists of some number of SQL queries that are tracked by the remote server. The user can also call reducers using this `DbConnection`.

The server periodically sends updates (via websocket) to the `DbConnection`. The `DbConnection` is responsible for updating its local view of the server state (`conn.Db`) using these messages, and invoking callbacks registered by the user.

Codegen also generates code for each table implementing the `ITable` interface in [`src/Table.cs`](./src/Table.cs). `DbConnection` only sees tables as `ITable`s -- it does not know anything more about the specific implementation of each table. `RemoteTableHandle<...>` in `src/Table.cs` implements the `ITable` interface in combination with the generated code. It also has a callback structure, `OnInternalInsert` and `OnInternalDelete`, used by generated code to maintain indexes.

Roughly speaking, code pertaining to specific tables should live in `Table.cs`, and code pertaining to the connection as a whole should live in `DbConnectionBase<...>`.

### Threading model

The C# SDK, unlike the [Rust SDK](https://github.com/clockworklabs/SpacetimeDB/tree/master/crates/sdk), **assumes a DbConnection is only accessed from a single thread**. This thread is referred to as the "main thread". The "main thread" is:
- Whichever thread is repeatedly calling `DbConnection.FrameTick()` in a loop.
It is **only safe to call `FrameTick()` from a single thread**. It is **only safe to access the DbConnection from this thread**. 
(Note: we should write about this in the public docs!)

While `DbConnection.FrameTick()` is running, the state of `conn.Db` is not well-defined. At all other times, `conn.Db` is guaranteed to be in a single, well-formed state, matching the state of the server at some time in the past [^1]. **This is only true when RemoteTables is accessed from the main thread**. Accessing `conn.Db` from any other thread may result in inconsistent reads or `ConcurrentModificationException`s.

In particular, a user of the SDK can never observe a "partially applied" transaction. Transaction updates are atomic, and happen all-at-once. If a transaction modifies multiple rows / tables, the user will never observe a `conn.Db` with only some of these updates applied. (As long as they don't access `conn.Db` from a background thread!)

Note that `DbConnection.FrameTick()` may invoke user callbacks. We also guarantee that `conn.Db` is in a well-formed state while any callbacks are invoked.

What we are doing is effectively using the main thread itself as a lock on `conn.Db`. This design makes it difficult to interact with the SDK in a multi-threaded way, but it provides a relatively simple mental model for users.

[^1]: Strictly speaking, we should say the "causal" past here. It is in the past light-cone of an observer interacting with the SDK; more concretely, the SDK has received a message from the server, and the state of the SDK corresponds to a state of the server at some point before that message was sent.[^2]

[^2]: Of course, defining things this way only makes sense if the server *has* a single, well-defined state. At time of writing, this is the case, since transactions on the server are totally ordered. But this may change in the future.

### Network protocol

The server and client communicate via websocket. They exchange messages encoded with [BSATN](https://spacetimedb.com/docs/bsatn). The specific messages they encode live in the `SpacetimeDB.ClientApi` namespace, which is stored in the [`src/SpacetimeDB/ClientApi`](./src/SpacetimeDB/ClientApi/) directory.

This namespace is automatically generated from a specification written in Rust. To regenerate this namespace, run the `tools~/gen-client-api.sh` or the
`tools~/gen-client-api.bat` script.

Note that messages are actually double-encoded. The `SpacetimeDB.ClientApi` messages store various `byte[]`s that must be decoded *again* to get actual table rows, reducer arguments, etc. This unfortunately involves a lot of copying.

### Overlapping subscriptions

The user may subscribe to the same row in multiple ways, e.g. `SELECT * FROM students WHERE student.age > 5` and `SELECT * FROM students WHERE student.class = 4`. If the user subscribes to both of these queries, the server will send multiple copies of all students in class 4 of age greater than 5.

We could deduplicate multiply-subscribed rows server-side, but this represents a large amount of work, so for performance reasons we deduplicate them client-side instead. We rely on the [`MultiDictionary`](src/MultiDictionary.cs) class to do this. This class is like a regular dictionary, but it can store multiple "copies" of a (key, value) pair. See the comments on that class for more information, and [`tests~/MultiDictionaryTests.cs`](./tests~/MultiDictionaryTests.cs) for randomized tests of its behavior.

There is also a class `MultiDictionaryDelta`. This represents a pre-processed batch of changes to a `MultiDictionary`. We prepare `MultiDictionaryDelta`s on a background thread and `Apply` them on the main thread. This allows us to do at least some work without blocking the main thread.

Note that if multiple subscriptions are subscribed to a row, when a server-side transaction updates that row, exactly the right number of updates will be sent over the network, in a single `ServerMessage`. `MultiDictionary` and `MultiDictionaryDelta` rely on this guarantee for correct operation, and will throw exceptions in debug mode if it is not met.


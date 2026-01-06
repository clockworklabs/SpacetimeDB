---
title: Chat App Tutorial
slug: /tutorials/chat-app
id: chat-app
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';
import { InstallCardLink } from "@site/src/components/InstallCardLink";

# Chat App Tutorial

In this tutorial, we'll implement a simple chat server as a SpacetimeDB module. You can write your module in TypeScript, C#, or Rust - use the tabs throughout this guide to see code examples in your preferred language.

A SpacetimeDB module is code that gets compiled and uploaded to SpacetimeDB. This code becomes server-side logic that interfaces directly with SpacetimeDB's relational database.

Each SpacetimeDB module defines a set of **tables** and a set of **reducers**.

<Tabs groupId="lang">
<TabItem value="typescript" label="TypeScript">

- Tables are declared with `table({ ...opts }, { ...columns })`. Each inserted object is a row; each field is a column.
- Tables are **private** by default (readable only by the owner and your module code). Set `{ public: true }` to make them readable by everyone; writes still happen only via reducers.
- A **reducer** is a function that reads/writes the database. Each reducer runs in its own transaction; its writes commit only if it completes without throwing.

:::note
SpacetimeDB runs your module inside the database host (not Node.js). There's no direct filesystem or network access from reducers.
:::

</TabItem>
<TabItem value="csharp" label="C#">

- Each table is defined as a C# `class` annotated with `[SpacetimeDB.Table]`, where an instance represents a row, and each field represents a column.
- By default, tables are **private**. This means that they are only readable by the table owner, and by server module code. The `[SpacetimeDB.Table(Public = true)]` annotation makes a table public.
- A reducer is a function which traverses and updates the database. Each reducer call runs in its own transaction, and its updates to the database are only committed if the reducer returns successfully. If an exception is thrown, the reducer call fails and the database is not updated.

</TabItem>
<TabItem value="rust" label="Rust">

- Each table is defined as a Rust struct annotated with `#[table(name = table_name)]`. An instance of the struct represents a row, and each field represents a column.
- By default, tables are **private**. The `#[table(name = table_name, public)]` macro makes a table public. **Public** tables are readable by all users but can still only be modified by your server module code.
- A reducer is a function that traverses and updates the database. Each reducer call runs in its own transaction, and its updates to the database are only committed if the reducer returns successfully. Reducers may return a `Result<()>`, with an `Err` return aborting the transaction.

</TabItem>
</Tabs>

## Install SpacetimeDB

If you haven't already, start by [installing SpacetimeDB](https://spacetimedb.com/install). This installs the `spacetime` CLI used to build, publish, and interact with your database.

<InstallCardLink />

<Tabs groupId="lang">
<TabItem value="typescript" label="TypeScript">

No additional installation needed - Node.js/npm will handle dependencies.

</TabItem>
<TabItem value="csharp" label="C#">

## Install .NET 8

Next we need to [install .NET 8 SDK](https://dotnet.microsoft.com/en-us/download/dotnet/8.0) so that we can build and publish our module.

You may already have .NET 8 installed:

```bash
dotnet --list-sdks
```

.NET 8.0 is the earliest to have the `wasi-experimental` workload that we rely on, but requires manual activation:

```bash
dotnet workload install wasi-experimental
```

</TabItem>
<TabItem value="rust" label="Rust">

## Install Rust

Next we need to [install Rust](https://www.rust-lang.org/tools/install) so that we can create our database module.

On macOS and Linux run this command to install the Rust compiler:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

If you're on Windows, go [here](https://learn.microsoft.com/en-us/windows/dev-environment/rust/setup).

</TabItem>
</Tabs>

## Project structure

Let's start by running `spacetime init` to initialize our project's directory structure:

<Tabs groupId="lang">
<TabItem value="typescript" label="TypeScript">

```bash
spacetime init --lang typescript quickstart-chat
```

</TabItem>
<TabItem value="csharp" label="C#">

```bash
spacetime init --lang csharp quickstart-chat
```

</TabItem>
<TabItem value="rust" label="Rust">

```bash
spacetime init --lang rust quickstart-chat
```

</TabItem>
</Tabs>

`spacetime init` will ask you for a project path in which to put your project. By default this will be `./quickstart-chat`. This basic project will have a few helper files like Cursor rules for SpacetimeDB and a `spacetimedb` directory which is where your SpacetimeDB module code will go.

<Tabs groupId="lang">
<TabItem value="typescript" label="TypeScript">

Inside the `spacetimedb/` directory will be a `src/index.ts` entrypoint (required for publishing).

</TabItem>
<TabItem value="csharp" label="C#">

`spacetime init` generated a few files:

1. Open `spacetimedb/StdbModule.csproj` to generate a .sln file for intellisense/validation support.
2. Open `spacetimedb/Lib.cs`, a trivial module.
3. Clear it out, so we can write a new module.

</TabItem>
<TabItem value="rust" label="Rust">

> [!IMPORTANT]
> While it is possible to use the traditional `cargo build` to build SpacetimeDB server modules, `spacetime build` makes this process easier.

```bash
cd spacetimedb
spacetime build
```

</TabItem>
</Tabs>

## Declare imports

<Tabs groupId="lang">
<TabItem value="typescript" label="TypeScript">

Open `spacetimedb/src/index.ts`. Replace its contents with the following imports:

```ts server
import { schema, t, table, SenderError } from 'spacetimedb/server';
```

From `spacetimedb/server`, we import:

- `table` to define SpacetimeDB tables.
- `t` for column/type builders.
- `schema` to compose our database schema and register reducers.
- `SenderError` to signal user-visible (transaction-aborting) errors.

</TabItem>
<TabItem value="csharp" label="C#">

To the top of `spacetimedb/Lib.cs`, add some imports we'll be using:

```csharp server
using SpacetimeDB;
```

We also need to create our static module class which all of the module code will live in:

```csharp server
public static partial class Module
{
}
```

</TabItem>
<TabItem value="rust" label="Rust">

Clear out `spacetimedb/src/lib.rs` and add these imports:

```rust server
use spacetimedb::{table, reducer, Table, ReducerContext, Identity, Timestamp};
```

From `spacetimedb`, we import:

- `table`, a macro used to define SpacetimeDB tables.
- `reducer`, a macro used to define SpacetimeDB reducers.
- `Table`, a rust trait which allows us to interact with tables.
- `ReducerContext`, a special argument passed to each reducer.
- `Identity`, a unique identifier for each user.
- `Timestamp`, a point in time.

</TabItem>
</Tabs>

## Define tables

We'll store two kinds of data: information about each user, and the messages that have been sent.

For each `User`, we'll store their `Identity` (the caller's unique identifier), an optional display name, and whether they're currently online. We'll use `Identity` as the primary key (unique and indexed).

<Tabs groupId="lang">
<TabItem value="typescript" label="TypeScript">

Add to `spacetimedb/src/index.ts`:

```ts server
const User = table(
  { name: 'user', public: true },
  {
    identity: t.identity().primaryKey(),
    name: t.string().optional(),
    online: t.bool(),
  }
);

const Message = table(
  { name: 'message', public: true },
  {
    sender: t.identity(),
    sent: t.timestamp(),
    text: t.string(),
  }
);

// Compose the schema (gives us ctx.db.user and ctx.db.message, etc.)
const spacetimedb = schema(User, Message);
```

</TabItem>
<TabItem value="csharp" label="C#">

In `spacetimedb/Lib.cs`, add the definition of the tables to the `Module` class:

```csharp server
[Table(Name = "user", Public = true)]
public partial class User
{
    [PrimaryKey]
    public Identity Identity;
    public string? Name;
    public bool Online;
}

[Table(Name = "message", Public = true)]
public partial class Message
{
    public Identity Sender;
    public Timestamp Sent;
    public string Text = "";
}
```

</TabItem>
<TabItem value="rust" label="Rust">

Add to `spacetimedb/src/lib.rs`:

```rust server
#[table(name = user, public)]
pub struct User {
    #[primary_key]
    identity: Identity,
    name: Option<String>,
    online: bool,
}

#[table(name = message, public)]
pub struct Message {
    sender: Identity,
    sent: Timestamp,
    text: String,
}
```

</TabItem>
</Tabs>

## Set users' names

We'll allow users to set a display name, since raw identities aren't user-friendly. Define a reducer that validates input, looks up the caller's `User` row by primary key, and updates it.

<Tabs groupId="lang">
<TabItem value="typescript" label="TypeScript">

Add:

```ts server
function validateName(name: string) {
  if (!name) {
    throw new SenderError('Names must not be empty');
  }
}

spacetimedb.reducer('set_name', { name: t.string() }, (ctx, { name }) => {
  validateName(name);
  const user = ctx.db.user.identity.find(ctx.sender);
  if (!user) {
    throw new SenderError('Cannot set name for unknown user');
  }
  ctx.db.user.identity.update({ ...user, name });
});
```

</TabItem>
<TabItem value="csharp" label="C#">

In `spacetimedb/Lib.cs`, add to the `Module` class:

```csharp server
[Reducer]
public static void SetName(ReducerContext ctx, string name)
{
    name = ValidateName(name);

    if (ctx.Db.user.Identity.Find(ctx.Sender) is User user)
    {
        user.Name = name;
        ctx.Db.user.Identity.Update(user);
    }
}

private static string ValidateName(string name)
{
    if (string.IsNullOrEmpty(name))
    {
        throw new Exception("Names must not be empty");
    }
    return name;
}
```

</TabItem>
<TabItem value="rust" label="Rust">

Add to `spacetimedb/src/lib.rs`:

```rust server
#[reducer]
pub fn set_name(ctx: &ReducerContext, name: String) -> Result<(), String> {
    let name = validate_name(name)?;
    if let Some(user) = ctx.db.user().identity().find(ctx.sender) {
        ctx.db.user().identity().update(User { name: Some(name), ..user });
        Ok(())
    } else {
        Err("Cannot set name for unknown user".to_string())
    }
}

fn validate_name(name: String) -> Result<String, String> {
    if name.is_empty() {
        Err("Names must not be empty".to_string())
    } else {
        Ok(name)
    }
}
```

</TabItem>
</Tabs>

You can extend validation with moderation checks, Unicode normalization, max length checks, or duplicate-name rejection.

## Send messages

Define a reducer to insert a new `Message` with the caller's identity and the call timestamp.

<Tabs groupId="lang">
<TabItem value="typescript" label="TypeScript">

Add:

```ts server
function validateMessage(text: string) {
  if (!text) {
    throw new SenderError('Messages must not be empty');
  }
}

spacetimedb.reducer('send_message', { text: t.string() }, (ctx, { text }) => {
  validateMessage(text);
  console.info(`User ${ctx.sender}: ${text}`);
  ctx.db.message.insert({
    sender: ctx.sender,
    text,
    sent: ctx.timestamp,
  });
});
```

</TabItem>
<TabItem value="csharp" label="C#">

In `spacetimedb/Lib.cs`, add to the `Module` class:

```csharp server
[Reducer]
public static void SendMessage(ReducerContext ctx, string text)
{
    text = ValidateMessage(text);
    Log.Info(text);
    ctx.Db.message.Insert(
        new Message
        {
            Sender = ctx.Sender,
            Text = text,
            Sent = ctx.Timestamp,
        }
    );
}

private static string ValidateMessage(string text)
{
    if (string.IsNullOrEmpty(text))
    {
        throw new ArgumentException("Messages must not be empty");
    }
    return text;
}
```

</TabItem>
<TabItem value="rust" label="Rust">

Add to `spacetimedb/src/lib.rs`:

```rust server
#[reducer]
pub fn send_message(ctx: &ReducerContext, text: String) -> Result<(), String> {
    let text = validate_message(text)?;
    log::info!("{}", text);
    ctx.db.message().insert(Message {
        sender: ctx.sender,
        text,
        sent: ctx.timestamp,
    });
    Ok(())
}

fn validate_message(text: String) -> Result<String, String> {
    if text.is_empty() {
        Err("Messages must not be empty".to_string())
    } else {
        Ok(text)
    }
}
```

</TabItem>
</Tabs>

## Set users' online status

SpacetimeDB can invoke lifecycle reducers when clients connect/disconnect. We'll create or update a `User` row to mark the caller online on connect, and mark them offline on disconnect.

<Tabs groupId="lang">
<TabItem value="typescript" label="TypeScript">

Add:

```ts server
spacetimedb.init(_ctx => {});

spacetimedb.clientConnected(ctx => {
  const user = ctx.db.user.identity.find(ctx.sender);
  if (user) {
    ctx.db.user.identity.update({ ...user, online: true });
  } else {
    ctx.db.user.insert({
      identity: ctx.sender,
      name: undefined,
      online: true,
    });
  }
});

spacetimedb.clientDisconnected(ctx => {
  const user = ctx.db.user.identity.find(ctx.sender);
  if (user) {
    ctx.db.user.identity.update({ ...user, online: false });
  } else {
    console.warn(
      `Disconnect event for unknown user with identity ${ctx.sender}`
    );
  }
});
```

</TabItem>
<TabItem value="csharp" label="C#">

In `spacetimedb/Lib.cs`, add to the `Module` class:

```csharp server
[Reducer(ReducerKind.ClientConnected)]
public static void ClientConnected(ReducerContext ctx)
{
    Log.Info($"Connect {ctx.Sender}");

    if (ctx.Db.user.Identity.Find(ctx.Sender) is User user)
    {
        user.Online = true;
        ctx.Db.user.Identity.Update(user);
    }
    else
    {
        ctx.Db.user.Insert(
            new User
            {
                Name = null,
                Identity = ctx.Sender,
                Online = true,
            }
        );
    }
}

[Reducer(ReducerKind.ClientDisconnected)]
public static void ClientDisconnected(ReducerContext ctx)
{
    if (ctx.Db.user.Identity.Find(ctx.Sender) is User user)
    {
        user.Online = false;
        ctx.Db.user.Identity.Update(user);
    }
    else
    {
        Log.Warn("Warning: No user found for disconnected client.");
    }
}
```

</TabItem>
<TabItem value="rust" label="Rust">

Add to `spacetimedb/src/lib.rs`:

```rust server
#[reducer(client_connected)]
pub fn client_connected(ctx: &ReducerContext) {
    if let Some(user) = ctx.db.user().identity().find(ctx.sender) {
        ctx.db.user().identity().update(User { online: true, ..user });
    } else {
        ctx.db.user().insert(User {
            name: None,
            identity: ctx.sender,
            online: true,
        });
    }
}

#[reducer(client_disconnected)]
pub fn identity_disconnected(ctx: &ReducerContext) {
    if let Some(user) = ctx.db.user().identity().find(ctx.sender) {
        ctx.db.user().identity().update(User { online: false, ..user });
    } else {
        log::warn!("Disconnect event for unknown user with identity {:?}", ctx.sender);
    }
}
```

</TabItem>
</Tabs>

## Start the server

If you haven't already started the SpacetimeDB host, run this in a **separate terminal** and leave it running:

```bash
spacetime start
```

## Publish the module

From the `quickstart-chat` directory:

<Tabs groupId="lang">
<TabItem value="typescript" label="TypeScript">

```bash
spacetime publish --server local --project-path spacetimedb quickstart-chat
```

</TabItem>
<TabItem value="csharp" label="C#">

```bash
spacetime publish --server local --project-path spacetimedb quickstart-chat
```

</TabItem>
<TabItem value="rust" label="Rust">

```bash
spacetime publish --server local --project-path spacetimedb quickstart-chat
```

</TabItem>
</Tabs>

You can choose any unique, URL-safe database name in place of `quickstart-chat`.

## Call reducers

Use the CLI to call reducers. Arguments are passed as JSON:

<Tabs groupId="lang">
<TabItem value="typescript" label="TypeScript">

```bash
spacetime call --server local quickstart-chat send_message "Hello, World!"
```

</TabItem>
<TabItem value="csharp" label="C#">

```bash
spacetime call --server local quickstart-chat SendMessage "Hello, World!"
```

</TabItem>
<TabItem value="rust" label="Rust">

```bash
spacetime call --server local quickstart-chat send_message "Hello, World!"
```

</TabItem>
</Tabs>

Check that it ran by viewing logs:

```bash
spacetime logs --server local quickstart-chat
```

## SQL queries

SpacetimeDB supports a subset of SQL so you can query your data:

```bash
spacetime sql --server local quickstart-chat "SELECT * FROM message"
```

Output will resemble:

```text
 sender                                                             | sent                             | text
--------------------------------------------------------------------+----------------------------------+-----------------
 0x93dda09db9a56d8fa6c024d843e805d8262191db3b4ba84c5efcd1ad451fed4e | 2025-04-08T15:47:46.935402+00:00 | "Hello, World!"
```

You've just set up your first SpacetimeDB module! You can find the full code for this module:
- [TypeScript server module](https://github.com/clockworklabs/SpacetimeDB/tree/master/modules/quickstart-chat-ts)
- [C# server module](https://github.com/clockworklabs/SpacetimeDB/tree/master/sdks/csharp/examples~/quickstart-chat/server)
- [Rust server module](https://github.com/clockworklabs/SpacetimeDB/tree/master/modules/quickstart-chat)

---

# Creating the Client

Next, you'll learn how to create a SpacetimeDB client application. Choose your preferred client language below.

<Tabs groupId="client-lang">
<TabItem value="typescript-react" label="TypeScript (React)">

## Project structure

Make sure you're in the `quickstart-chat` directory:

```bash
cd quickstart-chat
```

Create a React app:

```bash
pnpm create vite@latest client -- --template react-ts
cd client
pnpm install
pnpm install spacetimedb
```

## Generate module types

```bash
mkdir -p client/src/module_bindings
spacetime generate --lang typescript --out-dir client/src/module_bindings --project-path spacetimedb
```

## Connect to SpacetimeDB

In `client/src/main.tsx`:

```tsx
import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import './index.css';
import App from './App.tsx';
import { Identity } from 'spacetimedb';
import { SpacetimeDBProvider } from 'spacetimedb/react';
import { DbConnection, ErrorContext } from './module_bindings/index.ts';

const onConnect = (conn: DbConnection, identity: Identity, token: string) => {
  localStorage.setItem('auth_token', token);
  console.log('Connected to SpacetimeDB with identity:', identity.toHexString());
};

const onDisconnect = () => {
  console.log('Disconnected from SpacetimeDB');
};

const onConnectError = (_ctx: ErrorContext, err: Error) => {
  console.log('Error connecting to SpacetimeDB:', err);
};

const connectionBuilder = DbConnection.builder()
  .withUri('ws://localhost:3000')
  .withModuleName('quickstart-chat')
  .withToken(localStorage.getItem('auth_token') || undefined)
  .onConnect(onConnect)
  .onDisconnect(onDisconnect)
  .onConnectError(onConnectError);

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <SpacetimeDBProvider connectionBuilder={connectionBuilder}>
      <App />
    </SpacetimeDBProvider>
  </StrictMode>
);
```

## Use React hooks

In your components, use `useTable` and `useReducer` hooks:

```tsx
import { useSpacetimeDB, useTable, where, eq, useReducer } from 'spacetimedb/react';
import { tables, reducers } from './module_bindings';

function App() {
  const { identity, isActive: connected } = useSpacetimeDB();
  const setName = useReducer(reducers.setName);
  const sendMessage = useReducer(reducers.sendMessage);

  const [messages] = useTable(tables.message);
  const [onlineUsers] = useTable(tables.user, where(eq('online', true)));

  // ... rest of your component
}
```

For the complete React client code, see the [TypeScript client example](https://github.com/clockworklabs/SpacetimeDB/tree/master/crates/bindings-typescript/examples/quickstart-chat).

</TabItem>
<TabItem value="csharp-console" label="C# (Console)">

## Project structure

From the `quickstart-chat` directory, create a console application:

```bash
dotnet new console -o client
dotnet add client package SpacetimeDB.ClientSDK
```

## Generate module types

```bash
mkdir -p client/module_bindings
spacetime generate --lang csharp --out-dir client/module_bindings --project-path spacetimedb
```

## Connect to SpacetimeDB

In `client/Program.cs`:

```csharp
using SpacetimeDB;
using SpacetimeDB.Types;
using System.Collections.Concurrent;

Identity? local_identity = null;
var input_queue = new ConcurrentQueue<(string Command, string Args)>();

const string HOST = "http://localhost:3000";
const string DB_NAME = "quickstart-chat";

void Main()
{
    AuthToken.Init(".spacetime_csharp_quickstart");
    DbConnection? conn = ConnectToDB();
    RegisterCallbacks(conn);

    var cancellationTokenSource = new CancellationTokenSource();
    var thread = new Thread(() => ProcessThread(conn, cancellationTokenSource.Token));
    thread.Start();

    InputLoop();
    cancellationTokenSource.Cancel();
    thread.Join();
}

DbConnection ConnectToDB()
{
    return DbConnection.Builder()
        .WithUri(HOST)
        .WithModuleName(DB_NAME)
        .WithToken(AuthToken.Token)
        .OnConnect(OnConnected)
        .OnConnectError(OnConnectError)
        .OnDisconnect(OnDisconnected)
        .Build();
}

void OnConnected(DbConnection conn, Identity identity, string authToken)
{
    local_identity = identity;
    AuthToken.SaveToken(authToken);
    conn.SubscriptionBuilder().OnApplied(OnSubscriptionApplied).SubscribeToAllTables();
}

Main();
```

For the complete C# client code, see the [C# client example](https://github.com/clockworklabs/SpacetimeDB/tree/master/sdks/csharp/examples~/quickstart-chat/client).

</TabItem>
<TabItem value="rust-console" label="Rust (Console)">

## Project structure

From the `quickstart-chat` directory:

```bash
cargo new client
```

Add dependencies to `client/Cargo.toml`:

```toml
[dependencies]
spacetimedb-sdk = "1.0"
hex = "0.4"
```

## Generate module types

```bash
mkdir -p client/src/module_bindings
spacetime generate --lang rust --out-dir client/src/module_bindings --project-path spacetimedb
```

## Connect to SpacetimeDB

In `client/src/main.rs`:

```rust
mod module_bindings;
use module_bindings::*;
use spacetimedb_sdk::{credentials, DbContext, Error, Event, Identity, Status, Table, TableWithPrimaryKey};

const HOST: &str = "http://localhost:3000";
const DB_NAME: &str = "quickstart-chat";

fn main() {
    let ctx = connect_to_db();
    register_callbacks(&ctx);
    subscribe_to_tables(&ctx);
    ctx.run_threaded();
    user_input_loop(&ctx);
}

fn connect_to_db() -> DbConnection {
    DbConnection::builder()
        .on_connect(on_connected)
        .on_connect_error(on_connect_error)
        .on_disconnect(on_disconnected)
        .with_token(creds_store().load().expect("Error loading credentials"))
        .with_module_name(DB_NAME)
        .with_uri(HOST)
        .build()
        .expect("Failed to connect")
}

fn creds_store() -> credentials::File {
    credentials::File::new("quickstart-chat")
}

fn on_connected(_ctx: &DbConnection, _identity: Identity, token: &str) {
    if let Err(e) = creds_store().save(token) {
        eprintln!("Failed to save credentials: {:?}", e);
    }
}
```

For the complete Rust client code, see the [Rust client example](https://github.com/clockworklabs/SpacetimeDB/tree/master/crates/sdk/examples/quickstart-chat).

</TabItem>
</Tabs>

## What's next?

Congratulations! You've built a chat app with SpacetimeDB.

- Check out the [SDK Reference documentation](/sdks) for more advanced usage
- Explore the [Unity Tutorial](/docs/tutorials/unity) or [Unreal Tutorial](/docs/tutorials/unreal) for game development
- Learn about [Procedures](/functions/procedures) for making external API calls

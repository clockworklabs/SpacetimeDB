---
title: Chat App Tutorial
slug: /tutorials/chat-app
id: chat-app
toc_max_heading_level: 2
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';
import { InstallCardLink } from "@site/src/components/InstallCardLink";


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
[Table(Name = "User", Public = true)]
public partial class User
{
    [PrimaryKey]
    public Identity Identity;
    public string? Name;
    public bool Online;
}

[Table(Name = "Message", Public = true)]
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

    if (ctx.Db.User.Identity.Find(ctx.Sender) is User user)
    {
        user.Name = name;
        ctx.Db.User.Identity.Update(user);
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
    ctx.Db.Message.Insert(
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

    if (ctx.Db.User.Identity.Find(ctx.Sender) is User user)
    {
        user.Online = true;
        ctx.Db.User.Identity.Update(user);
    }
    else
    {
        ctx.Db.User.Insert(
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
    if (ctx.Db.User.Identity.Find(ctx.Sender) is User user)
    {
        user.Online = false;
        ctx.Db.User.Identity.Update(user);
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

You can choose any unique database name in place of `quickstart-chat`. Must 
be alphanumeric with internal hyphens.

## Call reducers

Use the CLI to call reducers. Arguments are passed as JSON:

<Tabs groupId="lang">
<TabItem value="typescript" label="TypeScript">

```bash
spacetime call --server local quickstart-chat send_message 'Hello, World!'
```

</TabItem>
<TabItem value="csharp" label="C#">

```bash
spacetime call --server local quickstart-chat SendMessage 'Hello, World!'
```

</TabItem>
<TabItem value="rust" label="Rust">

```bash
spacetime call --server local quickstart-chat send_message 'Hello, World!'
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
- [TypeScript server module](https://github.com/clockworklabs/SpacetimeDB/tree/master/templates/quickstart-chat-typescript)
- [C# server module](https://github.com/clockworklabs/SpacetimeDB/tree/master/templates/quickstart-chat-c-sharp/spacetimedb)
- [Rust server module](https://github.com/clockworklabs/SpacetimeDB/tree/master/templates/quickstart-chat-rust/spacetimedb)

---

## Creating the Client

Next, you'll learn how to create a SpacetimeDB client application. Choose your preferred client language below.

<Tabs groupId="client-lang">
<TabItem value="typescript-react" label="TypeScript (React)">

Next, you'll learn how to use TypeScript to create a SpacetimeDB client application.

By the end of this introduction, you will have created a basic single page web app which connects to the `quickstart-chat` database you just created.

### Project structure

Make sure you're in the `quickstart-chat` directory you created earlier in this guide:

```bash
cd quickstart-chat
```

Initialize a React app in the current directory:

```bash
pnpm create vite@latest . -- --template react-ts
pnpm install
```

We also need to install the `spacetimedb` package:

```bash
pnpm install spacetimedb
```

:::note

If you are using another package manager like `yarn` or `npm`, the same steps should work with the appropriate commands for those tools.

:::

:::warning

The `@clockworklabs/spacetimedb-sdk` package has been deprecated in favor of the `spacetimedb` package as of SpacetimeDB version 1.4.0. If you are using the old SDK package, you will need to switch to `spacetimedb`. You will also need a SpacetimeDB CLI version of 1.4.0+ to generate bindings for the new `spacetimedb` package.

:::

You can now `pnpm run dev` to see the Vite template app running at `http://localhost:5173`.

### Basic layout

The app we're going to create is a basic chat application. We will begin by creating a layout for our app. The webpage will contain four sections:

1. A profile section, where we can set our name.
2. A message section, where we can see all the messages.
3. A system section, where we can see system messages.
4. A new message section, where we can send a new message.

Replace the entire contents of `src/App.tsx` with the following:

```tsx
import React, { useEffect, useState } from 'react';
import { Message, tables, reducers } from './module_bindings';
import { useSpacetimeDB, useTable, where, eq, useReducer } from 'spacetimedb/react';
import { Identity, Timestamp } from 'spacetimedb';
import './App.css';

export type PrettyMessage = {
  senderName: string;
  text: string;
  sent: Timestamp;
  kind: 'system' | 'user';
};

function App() {
  const [newName, setNewName] = useState('');
  const [settingName, setSettingName] = useState(false);
  const [systemMessages, setSystemMessages] = useState([] as Infer<typeof Message>[]);
  const [newMessage, setNewMessage] = useState('');

  const onlineUsers: User[] = [];
  const offlineUsers: User[] = [];
  const users = [...onlineUsers, ...offlineUsers];
  const prettyMessages: PrettyMessage[] = [];

  const name = '';

  const onSubmitNewName = (e: React.FormEvent<HTMLFormElement>) => {
    e.preventDefault();
    setSettingName(false);
    // TODO: Call `setName` reducer
  };

  const onSubmitMessage = (e: React.FormEvent<HTMLFormElement>) => {
    e.preventDefault();
    setNewMessage('');
    // TODO: Call `sendMessage` reducer
  };

  return (
    <div className="App">
      <div className="profile">
        <h1>Profile</h1>
        {!settingName ? (
          <>
            <p>{name}</p>
            <button
              onClick={() => {
                setSettingName(true);
                setNewName(name);
              }}
            >
              Edit Name
            </button>
          </>
        ) : (
          <form onSubmit={onSubmitNewName}>
            <input
              type="text"
              aria-label="username input"
              value={newName}
              onChange={e => setNewName(e.target.value)}
            />
            <button type="submit">Submit</button>
          </form>
        )}
      </div>
      <div className="message-panel">
        <h1>Messages</h1>
        {prettyMessages.length < 1 && <p>No messages</p>}
        <div className="messages">
          {prettyMessages.map((message, key) => {
            const sentDate = message.sent.toDate();
            const now = new Date();
            const isOlderThanDay =
              now.getFullYear() !== sentDate.getFullYear() ||
              now.getMonth() !== sentDate.getMonth() ||
              now.getDate() !== sentDate.getDate();

            const timeString = sentDate.toLocaleTimeString([], {
              hour: '2-digit',
              minute: '2-digit',
            });
            const dateString = isOlderThanDay
              ? sentDate.toLocaleDateString([], {
                  year: 'numeric',
                  month: 'short',
                  day: 'numeric',
                }) + ' '
              : '';

            return (
              <div
                key={key}
                className={
                  message.kind === 'system' ? 'system-message' : 'user-message'
                }
              >
                <p>
                  <b>
                    {message.kind === 'system' ? 'System' : message.senderName}
                  </b>
                  <span
                    style={{
                      fontSize: '0.8rem',
                      marginLeft: '0.5rem',
                      color: '#666',
                    }}
                  >
                    {dateString}
                    {timeString}
                  </span>
                </p>
                <p>{message.text}</p>
              </div>
            );
          })}
        </div>
      </div>
      <div className="online" style={{ whiteSpace: 'pre-wrap' }}>
        <h1>Online</h1>
        <div>
          {onlineUsers.map((user, key) => (
            <div key={key}>
              <p>{user.name || user.identity.toHexString().substring(0, 8)}</p>
            </div>
          ))}
        </div>
        {offlineUsers.length > 0 && (
          <div>
            <h1>Offline</h1>
            {offlineUsers.map((user, key) => (
              <div key={key}>
                <p>
                  {user.name || user.identity.toHexString().substring(0, 8)}
                </p>
              </div>
            ))}
          </div>
        )}
      </div>
      <div className="new-message">
        <form
          onSubmit={onSubmitMessage}
          style={{
            display: 'flex',
            flexDirection: 'column',
            width: '50%',
            margin: '0 auto',
          }}
        >
          <h3>New Message</h3>
          <textarea
            aria-label="message input"
            value={newMessage}
            onChange={e => setNewMessage(e.target.value)}
          ></textarea>
          <button type="submit">Send</button>
        </form>
      </div>
    </div>
  );
}

export default App;
```

We have configured the `onSubmitNewName` and `onSubmitMessage` callbacks to be called when the user clicks the submit button in the profile and new message sections, respectively. For now, they do nothing when called, but later we'll add some logic to call SpacetimeDB reducers when these callbacks are called.

Let's also make it pretty. Replace the contents of `src/App.css` with the following:

```css
.App {
  display: grid;
  /* 
    3 rows: 
      1) Profile
      2) Main content (left = message, right = online)
      3) New message
  */
  grid-template-rows: auto 1fr auto;
  /* 2 columns: left for chat, right for online */
  grid-template-columns: 2fr 1fr;

  height: 100vh; /* fill viewport height */
  width: clamp(300px, 100%, 1200px);
  margin: 0 auto;
}

/* ----- Profile (Row 1, spans both columns) ----- */
.profile {
  grid-column: 1 / 3;
  display: flex;
  align-items: center;
  gap: 1rem;
  padding: 1rem;
  border-bottom: 1px solid var(--theme-color);
}

.profile h1 {
  margin-right: auto; /* pushes name/edit form to the right */
}

.profile form {
  display: flex;
  flex-grow: 1;
  align-items: center;
  gap: 0.5rem;
  max-width: 300px;
}

.profile form input {
  background-color: var(--textbox-color);
}

/* ----- Chat Messages (Row 2, Col 1) ----- */
.message-panel {
  grid-row: 2 / 3;
  grid-column: 1 / 2;

  /* Ensure this section scrolls if content is long */
  overflow-y: auto;
  padding: 1rem;
  display: flex;
  flex-direction: column;
  gap: 1rem;
}

.messages {
  display: flex;
  flex-direction: column;
  gap: 0.5rem;
}

.system-message {
  background-color: var(--theme-color);
  color: var(--theme-color-contrast);
  padding: 0.5rem 1rem;
  border-radius: 0.375rem;
  font-style: italic;
}

.user-message {
  background-color: var(--textbox-color);
  padding: 0.5rem 1rem;
  border-radius: 0.375rem;
}

.message h1 {
  margin-right: 0.5rem;
}

/* ----- Online Panel (Row 2, Col 2) ----- */
.online {
  grid-row: 2 / 3;
  grid-column: 2 / 3;

  /* Also scroll independently if needed */
  overflow-y: auto;
  padding: 1rem;
  border-left: 1px solid var(--theme-color);
  white-space: pre-wrap;
  font-family: monospace;
}

/* ----- New Message (Row 3, spans columns 1-2) ----- */
.new-message {
  grid-column: 1 / 3;
  display: flex;
  justify-content: center;
  align-items: center;
  padding: 1rem;
  border-top: 1px solid var(--theme-color);
}

.new-message form {
  display: flex;
  flex-direction: column;
  gap: 0.75rem;
  width: 100%;
  max-width: 600px;
}

.new-message form h3 {
  margin-bottom: 0.25rem;
}

/* Distinct background for the textarea */
.new-message form textarea {
  font-family: monospace;
  font-weight: 400;
  font-size: 1rem;
  resize: vertical;
  min-height: 80px;
  background-color: var(--textbox-color);
  color: inherit;

  /* Subtle shadow for visibility */
  box-shadow:
    0 1px 3px rgba(0, 0, 0, 0.12),
    0 1px 2px rgba(0, 0, 0, 0.24);
}

@media (prefers-color-scheme: dark) {
  .new-message form textarea {
    box-shadow: 0 0 0 1px #17492b;
  }
}
```

Next, we need to replace the global styles in `src/index.css` as well:

```css
/* ----- CSS Reset & Global Settings ----- */
*,
*::before,
*::after {
  box-sizing: border-box;
  margin: 0;
  padding: 0;
}

/* ----- Color Variables ----- */
:root {
  --theme-color: #3dc373;
  --theme-color-contrast: #08180e;
  --textbox-color: #edfef4;
  color-scheme: light dark;
}

@media (prefers-color-scheme: dark) {
  :root {
    --theme-color: #4cf490;
    --theme-color-contrast: #132219;
    --textbox-color: #0f311d;
  }
}

/* ----- Page Setup ----- */
html,
body,
#root {
  height: 100%;
  margin: 0;
}

body {
  font-family:
    -apple-system, BlinkMacSystemFont, 'Segoe UI', 'Roboto', 'Oxygen', 'Ubuntu',
    'Cantarell', 'Fira Sans', 'Droid Sans', 'Helvetica Neue', sans-serif;
  -webkit-font-smoothing: antialiased;
  -moz-osx-font-smoothing: grayscale;
}

code {
  font-family:
    source-code-pro, Menlo, Monaco, Consolas, 'Courier New', monospace;
}

/* ----- Buttons ----- */
button {
  padding: 0.5rem 0.75rem;
  border: none;
  border-radius: 0.375rem;
  background-color: var(--theme-color);
  color: var(--theme-color-contrast);
  cursor: pointer;
  font-weight: 600;
  letter-spacing: 0.1px;
  font-family: monospace;
}

/* ----- Inputs & Textareas ----- */
input,
textarea {
  border: none;
  border-radius: 0.375rem;
  caret-color: var(--theme-color);
  font-family: monospace;
  font-weight: 600;
  letter-spacing: 0.1px;
  padding: 0.5rem 0.75rem;
}

input:focus,
textarea:focus {
  outline: none;
  box-shadow: 0 0 0 2px var(--theme-color);
}
```

### Generate your module types

Before we can run the app, we need to generate the TypeScript bindings that `App.tsx` imports. The `spacetime` CLI's `generate` command generates client-side interfaces for the tables, reducers, and types defined in your server module.

In your `quickstart-chat` directory, run:

```bash
spacetime generate --lang typescript --out-dir src/module_bindings --project-path spacetimedb
```

Take a look inside `src/module_bindings`. The CLI should have generated several files:

```
module_bindings
├── client_connected_reducer.ts
├── client_disconnected_reducer.ts
├── index.ts
├── init_reducer.ts
├── message_table.ts
├── message_type.ts
├── send_message_reducer.ts
├── set_name_reducer.ts
├── user_table.ts
└── user_type.ts
```

With `spacetime generate` we have generated TypeScript types derived from the types you specified in your module, which we can conveniently use in our client. We've placed these in the `module_bindings` folder.

Now you can run `pnpm run dev` and open `http://localhost:5173` to see your app's layout. It won't connect to SpacetimeDB yet - let's fix that next.

The main entry to the SpacetimeDB API is the `DbConnection`, a type that manages a connection to a remote database. Let's import it and a few other types into our `src/main.tsx` below our other imports:

```tsx
import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import './index.css';
import App from './App.tsx';
import { Identity } from 'spacetimedb';
import { SpacetimeDBProvider } from 'spacetimedb/react';
import { DbConnection, type ErrorContext } from './module_bindings/index.ts';
```

> Note that we are importing `DbConnection` from our `module_bindings` because it is a code generated type with all the type information about our tables and types.

We've also imported the `SpacetimeDBProvider` React component which will allow us to connect our SpacetimeDB state directly to our React state seamlessly.

### Create your SpacetimeDB client

Now that we've imported the `DbConnection` type, we can use it to connect our app to our database.

Replace the body of the `main.tsx` file with the following, just below your imports:

```tsx
const onConnect = (conn: DbConnection, identity: Identity, token: string) => {
  localStorage.setItem('auth_token', token);
  console.log(
    'Connected to SpacetimeDB with identity:',
    identity.toHexString()
  );
  conn.reducers.onSendMessage(() => {
    console.log('Message sent.');
  });
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

Here we are configuring our SpacetimeDB connection by specifying the server URI, database name, and a few callbacks including the `onConnect` callback. When `onConnect` is called after connecting, we store the connection state, our `Identity`, and our SpacetimeDB credentials in our React state. If there is an error connecting, we also print that error to the console.

We are also using `localStorage` to store our SpacetimeDB credentials. This way, we can reconnect to SpacetimeDB with the same `Identity` and token if we refresh the page. The first time we connect, we won't have any credentials stored, so we pass `undefined` to the `withToken` method. This will cause SpacetimeDB to generate new credentials for us.

If you chose a different name for your database, replace `quickstart-chat` with that name, or republish your module as `quickstart-chat`.

Our React hooks will subscribe to the data in SpacetimeDB. When we subscribe, SpacetimeDB will run our subscription queries and store the result in a local "client cache". This cache will be updated in real-time as the data in the table changes on the server.

We pass our connection configuration directly to the `SpacetimeDBProvider`, which will manage our connection to SpacetimeDB.

#### Accessing the Data

Once SpacetimeDB is connected, we can easily access the data in the client cache using SpacetimeDB's provided React hooks, `useTable` and `useSpacetimeDB`.

`useTable` is the simplest way to access your database data. `useTable` subscribes your React app to data in a SpacetimeDB table so that it updates as the data changes. It essentially acts just like `useState` in React except the data is being updated in real-time from SpacetimeDB tables.

`useSpacetimeDB` gives you direct access to the connection in case you want to check the state of the connection or access database table state. Note that `useSpacetimeDB` does not automatically subscribe your app to data in the database.

Add the following `useSpacetimeDB` hook to the top of your render function in `App.tsx`, just below your `useState` declarations.

```tsx
const { identity, isActive: connected } = useSpacetimeDB();
const setName = useReducer(reducers.setName);
const sendMessage = useReducer(reducers.sendMessage);

// Subscribe to all messages in the chat
const [messages] = useTable(tables.message);
```

Next replace `const onlineUsers: User[] = [];` with the following:

```tsx
// Subscribe to all online users in the chat
// so we can show who's online and demonstrate
// the `where` and `eq` query expressions
const [onlineUsers] = useTable(
  tables.user,
  where(eq('online', true))
);
```

Notice that we can filter users in the `user` table based on their online status by passing a query expression into the `useTable` hook as the second argument.

Let's now prettify our messages in our render function by sorting them by their `sent` timestamp, and joining the username of the sender to the message by looking up the user by their `Identity` in the `user` table. Replace `const prettyMessages: PrettyMessage[] = [];` with the following:

```tsx
const prettyMessages: PrettyMessage[] = messages
  .sort((a, b) => (a.sent.toDate() > b.sent.toDate() ? 1 : -1))
  .map(message => {
    const user = users.find(
      u => u.identity.toHexString() === message.sender.toHexString()
    );
    return {
      senderName: user?.name || message.sender.toHexString().substring(0, 8),
      text: message.text,
      sent: message.sent,
      kind: Identity.zero().isEqual(message.sender) ? 'system' : 'user',
    };
  });
```

That's all we have to do to hook up our SpacetimeDB state to our React state. SpacetimeDB ensures that any changes on the server are pushed down to our application and rerendered on screen in real-time.

Let's also update our render function to show a loading message while we're connecting to SpacetimeDB. Add this just below our `prettyMessages` declaration:

```tsx
if (!connected || !identity) {
  return (
    <div className="App">
      <h1>Connecting...</h1>
    </div>
  );
}
```

Finally, let's also compute the name of the user from the `Identity` in our `name` variable. Replace `const name = '';` with the following:

```tsx
const name = (() => {
  const user = users.find(u => u.identity.isEqual(identity));
  return user?.name || identity?.toHexString().substring(0, 8) || '';
})();
```

#### Calling Reducers

Let's hook up our callbacks so we can send some messages and see them displayed in the app after they are synchronised by SpacetimeDB. We need to update the `onSubmitNewName` and `onSubmitMessage` callbacks to send the appropriate reducer to the module.

Modify the `onSubmitNewName` callback by adding a call to the `setName` reducer:

```tsx
const onSubmitNewName = (e: React.FormEvent<HTMLFormElement>) => {
  e.preventDefault();
  setSettingName(false);
  setName({ name: newName });
};
```

Next, modify the `onSubmitMessage` callback by adding a call to the `sendMessage` reducer:

```tsx
const onSubmitMessage = (e: React.FormEvent<HTMLFormElement>) => {
  e.preventDefault();
  setNewMessage('');
  sendMessage({ text: newMessage });
};
```

SpacetimeDB generated these functions for us based on the type information provided by our module. Calling these functions will invoke our reducers in our module.

Let's try out our app to see the result of these changes.

```sh
pnpm run dev
```

:::warning

Don't forget! You may need to publish your server module if you haven't yet.

:::

Send some messages and update your username and watch it change in real-time. Note that when you update your username, it also updates immediately for all prior messages. This is because the messages store the user's `Identity` directly, instead of their username, so we can retroactively apply their username to all prior messages.

Try opening a few incognito windows to see what it's like with multiple users!

#### Notify about new users

We can also register `onInsert`, `onUpdate`, and `onDelete` callbacks to handle events, not just state. For example, we might want to show a notification any time a new user connects to the database.

Note that these callbacks can fire in two contexts:

- After a reducer runs, when the client's cache is updated about changes to subscribed rows.
- After calling `subscribe`, when the client's cache is initialized with all existing matching rows.

Our current `useTable` only filters online users, but we can print a system message anytime a user enters or leaves the room by subscribing to callbacks on the `onlineUsers` React hook.

Update your `onlineUsers` React hook to add the following callbacks:

```tsx
// Subscribe to all online users in the chat
// so we can show who's online and demonstrate
// the `where` and `eq` query expressions
const [ onlineUsers ] = useTable(
  tables.user,
  where(eq('online', true)),
  {
    onInsert: user => {
      // All users being inserted here are online
      const name = user.name || user.identity.toHexString().substring(0, 8);
      setSystemMessages(prev => [
        ...prev,
        {
          sender: Identity.zero(),
          text: `${name} has connected.`,
          sent: Timestamp.now(),
        },
      ]);
    },
    onDelete: user => {
      // All users being deleted here are offline
      const name = user.name || user.identity.toHexString().substring(0, 8);
      setSystemMessages(prev => [
        ...prev,
        {
          sender: Identity.zero(),
          text: `${name} has disconnected.`,
          sent: Timestamp.now(),
        },
      ]);
    },
  }
);
```

These callbacks will be called any time the state of the `useTable` result changes to add or remove a row, while respecting your `where` filter.

Here, we post a system message indicating that a new user has connected if the user is being added to the `user` table and they're online, or if an existing user's online status is being updated to "online".

Next, let's add the system messages to our list of `Message`s so they can be interleaved with the chat messages. Modify `prettyMessages` to concat the `systemMessages` as well:

```tsx
const prettyMessages: PrettyMessage[] = Array.from(messages)
  .concat(systemMessages)
  .sort((a, b) => (a.sent.toDate() > b.sent.toDate() ? 1 : -1))
  .map(message => {
    const user = users.find(
      u => u.identity.toHexString() === message.sender.toHexString()
    );
    return {
      senderName: user?.name || message.sender.toHexString().substring(0, 8),
      text: message.text,
      sent: message.sent,
      kind: Identity.zero().isEqual(message.sender) ? 'system' : 'user',
    };
  });
```

Finally, let's also subscribe to offline users so we can show them in the sidebar as well. Replace `const offlineUsers: User[] = [];` with:

```tsx
const [offlineUsers] = useTable(
  tables.user,
  where(eq('online', false))
);
```

### Try it out!

Now that everything is set up, let's send some messages and see SpacetimeDB in action.

1. **Send your first message**: Type a message in the input field and click Send. You should see it appear in the message list almost instantly.

2. **Set your name**: Click "Edit Name" in the profile section and enter a username. Notice how your name updates immediately - not just for new messages, but for all your previous messages too! This is because messages store your `Identity`, and we look up the current name when displaying them.

3. **Open multiple windows**: Open the app in a second browser tab or an incognito window. You'll get a new identity and appear as a different user. Send messages from both and watch them appear in real-time on both screens.

4. **Watch the online status**: Notice the "Online" sidebar showing connected users. Open and close browser tabs to see users connect and disconnect, with system messages announcing each event.

5. **Test persistence**: Close all browser windows, then reopen the app. Your messages are still there! SpacetimeDB persists all your data, and your identity token (saved in localStorage) lets you reconnect as the same user.

You've just experienced the core features of SpacetimeDB: real-time synchronization, automatic persistence, and seamless multiplayer - all without writing any backend networking code.

### Conclusion

Congratulations! You've built a simple chat app with SpacetimeDB. You can find the full source code for the client we've created in this quickstart tutorial [here](https://github.com/clockworklabs/SpacetimeDB/tree/master/templates/quickstart-chat-typescript).

At this point you've learned how to create a basic TypeScript client for your SpacetimeDB `quickstart-chat` module. You've learned how to connect to SpacetimeDB and call reducers to update data. You've learned how to subscribe to table data, and hook it up so that it updates reactively in a React application.

</TabItem>
<TabItem value="csharp-console" label="C# (Console)">

Next, we'll show you how to get up and running with a simple SpacetimeDB app with a client written in C#.

We'll implement a command-line client for the module created in our [Rust](/docs/quickstarts/rust) or [C# Module](/docs/quickstarts/c-sharp) Quickstart guides. Ensure you followed one of these guides before continuing.

### Project structure

Enter the directory `quickstart-chat` you created in the [Rust Module Quickstart](/docs/quickstarts/rust) or [C# Module Quickstart](/docs/quickstarts/c-sharp) guides:

```bash
cd quickstart-chat
```

Initialize a new C# console application project in the current directory using either Visual Studio, Rider or the .NET CLI:

```bash
dotnet new console
```

Open the project in your IDE of choice.

### Add the NuGet package for the C# SpacetimeDB SDK

Add the `SpacetimeDB.ClientSDK` [NuGet package](https://www.nuget.org/packages/SpacetimeDB.ClientSDK/) using Visual Studio or Rider _NuGet Package Manager_ or via the .NET CLI:

```bash
dotnet add package SpacetimeDB.ClientSDK
```

### Clear `Program.cs`

Clear out any data from `Program.cs` so we can write our chat client.

### Generate your module types

The `spacetime` CLI's `generate` command will generate client-side interfaces for the tables, reducers and types defined in your server module.

In your `quickstart-chat` directory, run:

```bash
spacetime generate --lang csharp --out-dir module_bindings --project-path spacetimedb
```

Take a look inside `module_bindings`. The CLI should have generated three folders and nine files:

```
module_bindings
├── Reducers
│   ├── ClientConnected.g.cs
│   ├── ClientDisconnected.g.cs
│   ├── SendMessage.g.cs
│   └── SetName.g.cs
├── Tables
│   ├── Message.g.cs
│   └── User.g.cs
├── Types
│   ├── Message.g.cs
│   └── User.g.cs
└── SpacetimeDBClient.g.cs
```

### Add imports to Program.cs

Open `Program.cs` and add the following imports:

```csharp
using SpacetimeDB;
using SpacetimeDB.Types;
using System.Collections.Concurrent;
```

We will also need to create some global variables. We'll cover the `Identity` later in the `Save credentials` section. Later we'll also be setting up a second thread for handling user input. In the `Process thread` section we'll use this in the `ConcurrentQueue` to store the commands for that thread.

To `Program.cs`, add:

```csharp
// our local client SpacetimeDB identity
Identity? local_identity = null;

// declare a thread safe queue to store commands
var input_queue = new ConcurrentQueue<(string Command, string Args)>();
```

### Define Main function

We'll work outside-in, first defining our `Main` function at a high level, then implementing each behavior it needs. We need `Main` to do several things:

1. Initialize the `AuthToken` module, which loads and stores our authentication token to/from local storage.
2. Connect to the database.
3. Register a number of callbacks to run in response to various database events.
4. Start our processing thread which connects to the SpacetimeDB database, updates the SpacetimeDB client and processes commands that come in from the input loop running in the main thread.
5. Start the input loop, which reads commands from standard input and sends them to the processing thread.
6. When the input loop exits, stop the processing thread and wait for it to exit.

To `Program.cs`, add:

```csharp
void Main()
{
    // Initialize the `AuthToken` module
    AuthToken.Init(".spacetime_csharp_quickstart");
    // Builds and connects to the database
    DbConnection? conn = null;
    conn = ConnectToDB();
    // Registers to run in response to database events.
    RegisterCallbacks(conn);
    // Declare a threadsafe cancel token to cancel the process loop
    var cancellationTokenSource = new CancellationTokenSource();
    // Spawn a thread to call process updates and process commands
    var thread = new Thread(() => ProcessThread(conn, cancellationTokenSource.Token));
    thread.Start();
    // Handles CLI input
    InputLoop();
    // This signals the ProcessThread to stop
    cancellationTokenSource.Cancel();
    thread.Join();
}
```

### Connect to database

Before we connect, we'll store the SpacetimeDB hostname and our database name in constants `HOST` and `DB_NAME`.

A connection to a SpacetimeDB database is represented by a `DbConnection`. We configure `DbConnection`s using the builder pattern, by calling `DbConnection.Builder()`, chaining method calls to set various connection parameters and register callbacks, then we cap it off with a call to `.Build()` to begin the connection.

In our case, we'll supply the following options:

1. A `WithUri` call, to specify the URI of the SpacetimeDB host where our database is running.
2. A `WithModuleName` call, to specify the name or `Identity` of our database. Make sure to pass the same name here as you supplied to `spacetime publish`.
3. A `WithToken` call, to supply a token to authenticate with.
4. An `OnConnect` callback, to run when the remote database acknowledges and accepts our connection.
5. An `OnConnectError` callback, to run if the remote database is unreachable or it rejects our connection.
6. An `OnDisconnect` callback, to run when our connection ends.

To `Program.cs`, add:

```csharp
/// The URI of the SpacetimeDB instance hosting our chat database and module.
const string HOST = "http://localhost:3000";

/// The database name we chose when we published our module.
const string DB_NAME = "quickstart-chat";

/// Load credentials from a file and connect to the database.
DbConnection ConnectToDB()
{
    DbConnection? conn = null;
    conn = DbConnection.Builder()
        .WithUri(HOST)
        .WithModuleName(DB_NAME)
        .WithToken(AuthToken.Token)
        .OnConnect(OnConnected)
        .OnConnectError(OnConnectError)
        .OnDisconnect(OnDisconnected)
        .Build();
    return conn;
}
```

#### Save credentials

SpacetimeDB will accept any [OpenID Connect](https://openid.net/developers/how-connect-works/) compliant [JSON Web Token](https://jwt.io/) and use it to compute an `Identity` for the user. More complex applications will generally authenticate their user somehow, generate or retrieve a token, and attach it to their connection via `WithToken`. In our case, though, we'll connect anonymously the first time, let SpacetimeDB generate a fresh `Identity` and corresponding JWT for us, and save that token locally to re-use the next time we connect.

Once we are connected, we'll use the `AuthToken` module to save our token to local storage, so that we can re-authenticate as the same user the next time we connect. We'll also store the identity in a global variable `local_identity` so that we can use it to check if we are the sender of a message or name change. This callback also notifies us of our client's `Address`, an opaque identifier SpacetimeDB modules can use to distinguish connections by the same `Identity`, but we won't use it in our app.

To `Program.cs`, add:

```csharp
/// Our `OnConnected` callback: save our credentials to a file.
void OnConnected(DbConnection conn, Identity identity, string authToken)
{
    local_identity = identity;
    AuthToken.SaveToken(authToken);
}
```

#### Connect Error callback

Should we get an error during connection, we'll be given an `Exception` which contains the details about the exception. To keep things simple, we'll just write the exception to the console.

To `Program.cs`, add:

```csharp
/// Our `OnConnectError` callback: print the error, then exit the process.
void OnConnectError(Exception e)
{
    Console.Write($"Error while connecting: {e}");
}
```

#### Disconnect callback

When disconnecting, the callback contains the connection details and if an error occurs, it will also contain an `Exception`. If we get an error, we'll write the error to the console, if not, we'll just write that we disconnected.

To `Program.cs`, add:

```csharp
/// Our `OnDisconnect` callback: print a note, then exit the process.
void OnDisconnected(DbConnection conn, Exception? e)
{
    if (e != null)
    {
        Console.Write($"Disconnected abnormally: {e}");
    }
    else
    {
        Console.Write($"Disconnected normally.");
    }
}
```

### Register callbacks

Now we need to handle several sorts of events with Tables and Reducers:

1. `User.OnInsert`: When a new user joins, we'll print a message introducing them.
2. `User.OnUpdate`: When a user is updated, we'll print their new name, or declare their new online status.
3. `Message.OnInsert`: When we receive a new message, we'll print it.
4. `Reducer.OnSetName`: If the server rejects our attempt to set our name, we'll print an error.
5. `Reducer.OnSendMessage`: If the server rejects a message we send, we'll print an error.

To `Program.cs`, add:

```csharp
/// Register all the callbacks our app will use to respond to database events.
void RegisterCallbacks(DbConnection conn)
{
    conn.Db.User.OnInsert += User_OnInsert;
    conn.Db.User.OnUpdate += User_OnUpdate;

    conn.Db.Message.OnInsert += Message_OnInsert;

    conn.Reducers.OnSetName += Reducer_OnSetNameEvent;
    conn.Reducers.OnSendMessage += Reducer_OnSendMessageEvent;
}
```

#### Notify about new users

For each table, we can register on-insert and on-delete callbacks to be run whenever a subscribed row is inserted or deleted. We register these callbacks using the `OnInsert` and `OnDelete` methods, which are automatically generated for each table by `spacetime generate`.

These callbacks can fire in two contexts:

- After a reducer runs, when the client's cache is updated about changes to subscribed rows.
- After calling `subscribe`, when the client's cache is initialized with all existing matching rows.

This second case means that, even though the module only ever inserts online users, the client's `User.OnInsert` callbacks may be invoked with users who are offline. We'll only notify about online users.

`OnInsert` and `OnDelete` callbacks take two arguments: an `EventContext` and the altered row. The `EventContext.Event` is an enum which describes the event that caused the row to be inserted or deleted. All SpacetimeDB callbacks accept a context argument, which you can use in place of your top-level `DbConnection`.

Whenever we want to print a user, if they have set a name, we'll use that. If they haven't set a name, we'll instead print the first 8 bytes of their identity, encoded as hexadecimal. We'll define a function `UserNameOrIdentity` to handle this.

To `Program.cs`, add:

```csharp
/// If the user has no set name, use the first 8 characters from their identity.
string UserNameOrIdentity(User user) => user.Name ?? user.Identity.ToString()[..8];

/// Our `User.OnInsert` callback: if the user is online, print a notification.
void User_OnInsert(EventContext ctx, User insertedValue)
{
    if (insertedValue.Online)
    {
        Console.WriteLine($"{UserNameOrIdentity(insertedValue)} is online");
    }
}
```

#### Notify about updated users

Because we declared a primary key column in our `User` table, we can also register on-update callbacks. These run whenever a row is replaced by a row with the same primary key, like our module's `User.Identity.Update` calls. We register these callbacks using the `OnUpdate` method, which is automatically implemented by `spacetime generate` for any table with a primary key column.

`OnUpdate` callbacks take three arguments: the old row, the new row, and a `EventContext`.

In our module, users can be updated for three reasons:

1. They've set their name using the `SetName` reducer.
2. They're an existing user re-connecting, so their `Online` has been set to `true`.
3. They've disconnected, so their `Online` has been set to `false`.

We'll print an appropriate message in each of these cases.

To `Program.cs`, add:

```csharp
/// Our `User.OnUpdate` callback:
/// print a notification about name and status changes.
void User_OnUpdate(EventContext ctx, User oldValue, User newValue)
{
    if (oldValue.Name != newValue.Name)
    {
        Console.WriteLine($"{UserNameOrIdentity(oldValue)} renamed to {newValue.Name}");
    }
    if (oldValue.Online != newValue.Online)
    {
        if (newValue.Online)
        {
            Console.WriteLine($"{UserNameOrIdentity(newValue)} connected.");
        }
        else
        {
            Console.WriteLine($"{UserNameOrIdentity(newValue)} disconnected.");
        }
    }
}
```

#### Print messages

When we receive a new message, we'll print it to standard output, along with the name of the user who sent it. Keep in mind that we only want to do this for new messages, i.e. those inserted by a `SendMessage` reducer invocation. We have to handle the backlog we receive when our subscription is initialized separately, to ensure they're printed in the correct order. To that effect, our `OnInsert` callback will check if its `ReducerEvent` argument is not `null`, and only print in that case.

To find the `User` based on the message's `Sender` identity, we'll use `User.Identity.Find`, which behaves like the same function on the server.

We'll print the user's name or identity in the same way as we did when notifying about `User` table events, but here we have to handle the case where we don't find a matching `User` row. This can happen when the module owner sends a message using the CLI's `spacetime call`. In this case, we'll print `unknown`.

To `Program.cs`, add:

```csharp
/// Our `Message.OnInsert` callback: print new messages.
void Message_OnInsert(EventContext ctx, Message insertedValue)
{
    // We are filtering out messages inserted during the subscription being applied,
    // since we will be printing those in the OnSubscriptionApplied callback,
    // where we will be able to first sort the messages before printing.
    if (ctx.Event is not Event<Reducer>.SubscribeApplied)
    {
        PrintMessage(ctx.Db, insertedValue);
    }
}

void PrintMessage(RemoteTables tables, Message message)
{
    var sender = tables.User.Identity.Find(message.Sender);
    var senderName = "unknown";
    if (sender != null)
    {
        senderName = UserNameOrIdentity(sender);
    }

    Console.WriteLine($"{senderName}: {message.Text}");
}
```

#### Warn if our name was rejected

We can also register callbacks to run each time a reducer is invoked. We register these callbacks using the `OnReducerEvent` method of the `Reducer` namespace, which is automatically implemented for each reducer by `spacetime generate`.

Each reducer callback takes one fixed argument:

The `ReducerEventContext` of the callback, which contains an `Event` that contains several fields. The ones we care about are:

1. The `CallerIdentity`, the `Identity` of the client that called the reducer.
2. The `Status` of the reducer run, one of `Committed`, `Failed` or `OutOfEnergy`.
3. If we get a `Status.Failed`, an error message is nested inside that we'll want to write to the console.

It also takes a variable amount of additional arguments that match the reducer's arguments.

These callbacks will be invoked in one of two cases:

1. If the reducer was successful and altered any of our subscribed rows.
2. If we requested an invocation which failed.

Note that a status of `Failed` or `OutOfEnergy` implies that the caller identity is our own identity.

We already handle successful `SetName` invocations using our `User.OnUpdate` callback, but if the module rejects a user's chosen name, we'd like that user's client to let them know. We define a function `Reducer_OnSetNameEvent` as a `Reducer.OnSetNameEvent` callback which checks if the reducer failed, and if it did, prints an error message including the rejected name.

We'll test both that our identity matches the sender and that the status is `Failed`, even though the latter implies the former, for demonstration purposes.

To `Program.cs`, add:

```csharp
/// Our `OnSetNameEvent` callback: print a warning if the reducer failed.
void Reducer_OnSetNameEvent(ReducerEventContext ctx, string name)
{
    var e = ctx.Event;
    if (e.CallerIdentity == local_identity && e.Status is Status.Failed(var error))
    {
        Console.Write($"Failed to change name to {name}: {error}");
    }
}
```

#### Warn if our message was rejected

We handle warnings on rejected messages the same way as rejected names, though the types and the error message are different.

To `Program.cs`, add:

```csharp
/// Our `OnSendMessageEvent` callback: print a warning if the reducer failed.
void Reducer_OnSendMessageEvent(ReducerEventContext ctx, string text)
{
    var e = ctx.Event;
    if (e.CallerIdentity == local_identity && e.Status is Status.Failed(var error))
    {
        Console.Write($"Failed to send message {text}: {error}");
    }
}
```

### Subscribe to queries

SpacetimeDB is set up so that each client subscribes via SQL queries to some subset of the database, and is notified about changes only to that subset. For complex apps with large databases, judicious subscriptions can save each client significant network bandwidth, memory and computation. For example, in [BitCraft](https://bitcraftonline.com), each player's client subscribes only to the entities in the "chunk" of the world where that player currently resides, rather than the entire game world. Our app is much simpler than BitCraft, so we'll just subscribe to the whole database using `SubscribeToAllTables`.

You can also subscribe to specific tables using SQL syntax, e.g. `SELECT * FROM my_table`. Our [SQL documentation](/reference/sql) enumerates the operations that are accepted in our SQL syntax.

When we specify our subscriptions, we can supply an `OnApplied` callback. This will run when the subscription is applied and the matching rows become available in our client cache. We'll use this opportunity to print the message backlog in proper order.

We can also provide an `OnError` callback. This will run if the subscription fails, usually due to an invalid or malformed SQL queries. We can't handle this case, so we'll just print out the error and exit the process.

In `Program.cs`, update our `OnConnected` function to include `conn.SubscriptionBuilder().OnApplied(OnSubscriptionApplied).SubscribeToAllTables();` so that it reads:

```csharp
/// Our `OnConnect` callback: save our credentials to a file.
void OnConnected(DbConnection conn, Identity identity, string authToken)
{
    local_identity = identity;
    AuthToken.SaveToken(authToken);

    conn.SubscriptionBuilder()
        .OnApplied(OnSubscriptionApplied)
        .SubscribeToAllTables();
}
```

### OnSubscriptionApplied callback

Once our subscription is applied, we'll print all the previously sent messages. We'll define a function `PrintMessagesInOrder` to do this. `PrintMessagesInOrder` calls the automatically generated `Iter` function on our `Message` table, which returns an iterator over all rows in the table. We'll use the `OrderBy` method on the iterator to sort the messages by their `Sent` timestamp.

To `Program.cs`, add:

```csharp
/// Our `OnSubscriptionApplied` callback:
/// sort all past messages and print them in timestamp order.
void OnSubscriptionApplied(SubscriptionEventContext ctx)
{
    Console.WriteLine("Connected");
    PrintMessagesInOrder(ctx.Db);
}

void PrintMessagesInOrder(RemoteTables tables)
{
    foreach (Message message in tables.Message.Iter().OrderBy(item => item.Sent))
    {
        PrintMessage(tables, message);
    }
}
```

### Process thread

Since the input loop will be blocking, we'll run our processing code in a separate thread.

This thread will loop until the thread is signaled to exit, calling the update function `FrameTick` on the `DbConnection` to process any updates received from the database, and `ProcessCommand` to process any commands received from the input loop.

Afterward, close the connection to the database.

To `Program.cs`, add:

```csharp
/// Our separate thread from main, where we can call process updates and process commands without blocking the main thread.
void ProcessThread(DbConnection conn, CancellationToken ct)
{
    try
    {
        // loop until cancellation token
        while (!ct.IsCancellationRequested)
        {
            conn.FrameTick();

            ProcessCommands(conn.Reducers);

            Thread.Sleep(100);
        }
    }
    finally
    {
        conn.Disconnect();
    }
}
```

### Handle user input

The input loop will read commands from standard input and send them to the processing thread using the input queue. The `ProcessCommands` function is called every 100ms by the processing thread to process any pending commands.

Supported Commands:

1. Send a message: `message`, send the message to the database by calling `Reducer.SendMessage` which is automatically generated by `spacetime generate`.

2. Set name: `name`, will send the new name to the database by calling `Reducer.SetName` which is automatically generated by `spacetime generate`.

To `Program.cs`, add:

```csharp
/// Read each line of standard input, and either set our name or send a message as appropriate.
void InputLoop()
{
    while (true)
    {
        var input = Console.ReadLine();
        if (input == null)
        {
            break;
        }

        if (input.StartsWith("/name "))
        {
            input_queue.Enqueue(("name", input[6..]));
            continue;
        }
        else
        {
            input_queue.Enqueue(("message", input));
        }
    }
}

void ProcessCommands(RemoteReducers reducers)
{
    // process input queue commands
    while (input_queue.TryDequeue(out var command))
    {
        switch (command.Command)
        {
            case "message":
                reducers.SendMessage(command.Args);
                break;
            case "name":
                reducers.SetName(command.Args);
                break;
        }
    }
}
```

### Run the client

Finally, we just need to add a call to `Main`.

To `Program.cs`, add:

```csharp
Main();
```

Now, we can run the client by hitting start in Visual Studio or Rider; or by running the following command in the `client` directory:

```bash
dotnet run --project client
```

</TabItem>
<TabItem value="rust-console" label="Rust (Console)">

Next, we'll show you how to get up and running with a simple SpacetimeDB app with a client written in Rust.

We'll implement a command-line client for the module created in our Rust or C# Module Quickstart guides. Make sure you follow one of these guides before you start on this one.

### Project structure

Enter the directory `quickstart-chat` you created in the [Rust Module Quickstart](/docs/quickstarts/rust) or [C# Module Quickstart](/docs/quickstarts/c-sharp) guides:

```bash
cd quickstart-chat
```

Initialize a Rust crate in the current directory for our client application:

```bash
cargo init
```

### Depend on `spacetimedb-sdk` and `hex`

`Cargo.toml` should be initialized without any dependencies. We'll need two:

- [`spacetimedb-sdk`](https://crates.io/crates/spacetimedb-sdk), which defines client-side interfaces for interacting with a remote SpacetimeDB database.
- [`hex`](https://crates.io/crates/hex), which we'll use to print unnamed users' identities as hexadecimal strings.

Below the `[dependencies]` line in `Cargo.toml`, add:

```toml
spacetimedb-sdk = "1.0"
hex = "0.4"
```

Make sure you depend on the same version of `spacetimedb-sdk` as is reported by the SpacetimeDB CLI tool's `spacetime version`!

### Clear `src/main.rs`

`src/main.rs` should be initialized with a trivial "Hello world" program. Clear it out so we can write our chat client.

In your `quickstart-chat` directory, run:

```bash
rm src/main.rs
touch src/main.rs
```

### Generate your module types

The `spacetime` CLI's `generate` command will generate client-side interfaces for the tables, reducers and types referenced by tables or reducers defined in your server module.

In your `quickstart-chat` directory, run:

```bash
spacetime generate --lang rust --out-dir src/module_bindings --project-path spacetimedb
```

Take a look inside `src/module_bindings`. The CLI should have generated a few files:

```
module_bindings/
├── client_connected_reducer.rs
├── client_disconnected_reducer.rs
├── message_table.rs
├── message_type.rs
├── mod.rs
├── send_message_reducer.rs
├── set_name_reducer.rs
├── user_table.rs
└── user_type.rs
```

To use these, we'll declare the module in our client crate and import its definitions.

To `src/main.rs`, add:

```rust
mod module_bindings;
use module_bindings::*;
```

### Add more imports

We'll need additional imports from `spacetimedb_sdk` for interacting with the database, handling credentials, and managing events.

To `src/main.rs`, add:

```rust
use spacetimedb_sdk::{credentials, DbContext, Error, Event, Identity, Status, Table, TableWithPrimaryKey};
```

### Define the main function

Our `main` function will do the following:

1. Connect to the database.
2. Register a number of callbacks to run in response to various database events.
3. Subscribe to a set of SQL queries, whose results will be replicated and automatically updated in our client.
4. Spawn a background thread where our connection will process messages and invoke callbacks.
5. Enter a loop to handle user input from the command line.

We'll see the implementation of these functions a bit later, but for now add to `src/main.rs`:

```rust
fn main() {
    // Connect to the database
    let ctx = connect_to_db();

    // Register callbacks to run in response to database events.
    register_callbacks(&ctx);

    // Subscribe to SQL queries in order to construct a local partial replica of the database.
    subscribe_to_tables(&ctx);

    // Spawn a thread, where the connection will process messages and invoke callbacks.
    ctx.run_threaded();

    // Handle CLI input
    user_input_loop(&ctx);
}
```

### Connect to the database

A connection to a SpacetimeDB database is represented by a `DbConnection`. We configure `DbConnection`s using the builder pattern, by calling `DbConnection::builder()`, chaining method calls to set various connection parameters and register callbacks, then we cap it off with a call to `.build()` to begin the connection.

In our case, we'll supply the following options:

1. An `on_connect` callback, to run when the remote database acknowledges and accepts our connection.
2. An `on_connect_error` callback, to run if the remote database is unreachable or it rejects our connection.
3. An `on_disconnect` callback, to run when our connection ends.
4. A `with_token` call, to supply a token to authenticate with.
5. A `with_module_name` call, to specify the name or `Identity` of our database. Make sure to pass the same name here as you supplied to `spacetime publish`.
6. A `with_uri` call, to specify the URI of the SpacetimeDB host where our database is running.

To `src/main.rs`, add:

```rust
/// The URI of the SpacetimeDB instance hosting our chat database and module.
const HOST: &str = "http://localhost:3000";

/// The database name we chose when we published our module.
const DB_NAME: &str = "quickstart-chat";

/// Load credentials from a file and connect to the database.
fn connect_to_db() -> DbConnection {
    DbConnection::builder()
        // Register our `on_connect` callback, which will save our auth token.
        .on_connect(on_connected)
        // Register our `on_connect_error` callback, which will print a message, then exit the process.
        .on_connect_error(on_connect_error)
        // Our `on_disconnect` callback, which will print a message, then exit the process.
        .on_disconnect(on_disconnected)
        // If the user has previously connected, we'll have saved a token in the `on_connect` callback.
        // In that case, we'll load it and pass it to `with_token`,
        // so we can re-authenticate as the same `Identity`.
        .with_token(creds_store().load().expect("Error loading credentials"))
        // Set the database name we chose when we called `spacetime publish`.
        .with_module_name(DB_NAME)
        // Set the URI of the SpacetimeDB host that's running our database.
        .with_uri(HOST)
        // Finalize configuration and connect!
        .build()
        .expect("Failed to connect")
}
```

#### Save credentials

SpacetimeDB will accept any [OpenID Connect](https://openid.net/developers/how-connect-works/) compliant [JSON Web Token](https://jwt.io/) and use it to compute an `Identity` for the user. More complex applications will generally authenticate their user somehow, generate or retrieve a token, and attach it to their connection via `with_token`. In our case, though, we'll connect anonymously the first time, let SpacetimeDB generate a fresh `Identity` and corresponding JWT for us, and save that token locally to re-use the next time we connect.

The Rust SDK provides a pair of functions in `File`, `save` and `load`, for saving and storing these credentials in a file. By default the `save` and `load` will look for credentials in the `$HOME/.spacetimedb_client_credentials/` directory, which should be unintrusive. If saving our credentials fails, we'll print a message to standard error, but otherwise continue; even though the user won't be able to reconnect with the same identity, they can still chat normally.

To `src/main.rs`, add:

```rust
fn creds_store() -> credentials::File {
    credentials::File::new("quickstart-chat")
}

/// Our `on_connect` callback: save our credentials to a file.
fn on_connected(_ctx: &DbConnection, _identity: Identity, token: &str) {
    if let Err(e) = creds_store().save(token) {
        eprintln!("Failed to save credentials: {:?}", e);
    }
}
```

#### Handle errors and disconnections

We need to handle connection errors and disconnections by printing appropriate messages and exiting the program. These callbacks take an `ErrorContext`, a `DbConnection` that's been augmented with information about the error that occured.

To `src/main.rs`, add:

```rust
/// Our `on_connect_error` callback: print the error, then exit the process.
fn on_connect_error(_ctx: &ErrorContext, err: Error) {
    eprintln!("Connection error: {:?}", err);
    std::process::exit(1);
}

/// Our `on_disconnect` callback: print a note, then exit the process.
fn on_disconnected(_ctx: &ErrorContext, err: Option<Error>) {
    if let Some(err) = err {
        eprintln!("Disconnected: {}", err);
        std::process::exit(1);
    } else {
        println!("Disconnected.");
        std::process::exit(0);
    }
}
```

### Register callbacks

We need to handle several sorts of events:

1. When a new user joins, we'll print a message introducing them.
2. When a user is updated, we'll print their new name, or declare their new online status.
3. When we receive a new message, we'll print it.
4. If the server rejects our attempt to set our name, we'll print an error.
5. If the server rejects a message we send, we'll print an error.

To `src/main.rs`, add:

```rust
/// Register all the callbacks our app will use to respond to database events.
fn register_callbacks(ctx: &DbConnection) {
    // When a new user joins, print a notification.
    ctx.db.user().on_insert(on_user_inserted);

    // When a user's status changes, print a notification.
    ctx.db.user().on_update(on_user_updated);

    // When a new message is received, print it.
    ctx.db.message().on_insert(on_message_inserted);

    // When we fail to set our name, print a warning.
    ctx.reducers.on_set_name(on_name_set);

    // When we fail to send a message, print a warning.
    ctx.reducers.on_send_message(on_message_sent);
}
```

#### Notify about new users

For each table, we can register on-insert and on-delete callbacks to be run whenever a subscribed row is inserted or deleted. We register these callbacks using the `on_insert` and `on_delete`, which is automatically implemented for each table by `spacetime generate`.

These callbacks can fire in several contexts, of which we care about two:

- After a reducer runs, when the client's cache is updated about changes to subscribed rows.
- After calling `subscribe`, when the client's cache is initialized with all existing matching rows.

This second case means that, even though the module only ever inserts online users, the client's `conn.db.user().on_insert(..)` callbacks may be invoked with users who are offline. We'll only notify about online users.

`on_insert` and `on_delete` callbacks take two arguments: an `&EventContext` and the modified row. Like the `ErrorContext` above, `EventContext` is a `DbConnection` that's been augmented with information about the event that caused the row to be modified. You can determine whether the insert/delete operation was caused by a reducer, a newly-applied subscription, or some other event by pattern-matching on `ctx.event`.

Whenever we want to print a user, if they have set a name, we'll use that. If they haven't set a name, we'll instead print the first 8 bytes of their identity, encoded as hexadecimal. We'll define functions `user_name_or_identity` and `identity_leading_hex` to handle this.

To `src/main.rs`, add:

```rust
/// Our `User::on_insert` callback:
/// if the user is online, print a notification.
fn on_user_inserted(_ctx: &EventContext, user: &User) {
    if user.online {
        println!("User {} connected.", user_name_or_identity(user));
    }
}

fn user_name_or_identity(user: &User) -> String {
    user.name
        .clone()
        .unwrap_or_else(|| user.identity.to_hex().to_string())
}
```

#### Notify about updated users

Because we declared a `#[primary_key]` column in our `User` table, we can also register on-update callbacks. These run whenever a row is replaced by a row with the same primary key, like our module's `ctx.db.user().identity().update(..)` calls. We register these callbacks using the `on_update` method of the trait `TableWithPrimaryKey`, which is automatically implemented by `spacetime generate` for any table with a `#[primary_key]` column.

`on_update` callbacks take three arguments: the `&EventContext`, the old row, and the new row.

In our module, users can be updated for three reasons:

1. They've set their name using the `set_name` reducer.
2. They're an existing user re-connecting, so their `online` has been set to `true`.
3. They've disconnected, so their `online` has been set to `false`.

We'll print an appropriate message in each of these cases.

To `src/main.rs`, add:

```rust
/// Our `User::on_update` callback:
/// print a notification about name and status changes.
fn on_user_updated(_ctx: &EventContext, old: &User, new: &User) {
    if old.name != new.name {
        println!(
            "User {} renamed to {}.",
            user_name_or_identity(old),
            user_name_or_identity(new)
        );
    }
    if old.online && !new.online {
        println!("User {} disconnected.", user_name_or_identity(new));
    }
    if !old.online && new.online {
        println!("User {} connected.", user_name_or_identity(new));
    }
}
```

#### Print messages

When we receive a new message, we'll print it to standard output, along with the name of the user who sent it. Keep in mind that we only want to do this for new messages, i.e. those inserted by a `send_message` reducer invocation. We have to handle the backlog we receive when our subscription is initialized separately, to ensure they're printed in the correct order. To that effect, our `on_message_inserted` callback will check if the ctx.event type is an `Event::Reducer`, and only print in that case.

To find the `User` based on the message's `sender` identity, we'll use `ctx.db.user().identity().find(..)`, which behaves like the same function on the server.

We'll print the user's name or identity in the same way as we did when notifying about `User` table events, but here we have to handle the case where we don't find a matching `User` row. This can happen when the module owner sends a message using the CLI's `spacetime call`. In this case, we'll print `unknown`.

Notice that our `print_message` function takes an `&impl RemoteDbContext` as an argument. This is a trait, defined in our `module_bindings` by `spacetime generate`, which is implemented by `DbConnection`, `EventContext`, `ErrorContext` and a few other similar types. (`RemoteDbContext` is actually a shorthand for `DbContext`, which applies to connections to _any_ module, with its associated types locked to module-specific ones.) Later on, we're going to call `print_message` with a `ReducerEventContext`, so we need to be more generic than just accepting `EventContext`.

To `src/main.rs`, add:

```rust
/// Our `Message::on_insert` callback: print new messages.
fn on_message_inserted(ctx: &EventContext, message: &Message) {
    if let Event::Reducer(_) = ctx.event {
        print_message(ctx, message)
    }
}

fn print_message(ctx: &impl RemoteDbContext, message: &Message) {
    let sender = ctx
        .db()
        .user()
        .identity()
        .find(&message.sender.clone())
        .map(|u| user_name_or_identity(&u))
        .unwrap_or_else(|| "unknown".to_string());
    println!("{}: {}", sender, message.text);
}
```

#### Handle reducer failures

We can also register callbacks to run each time a reducer is invoked. We register these callbacks using the `on_reducer` method of the `Reducer` trait, which is automatically implemented for each reducer by `spacetime generate`.

Each reducer callback first takes a `&ReducerEventContext` which contains metadata about the reducer call, including the identity of the caller and whether or not the reducer call suceeded.

These callbacks will be invoked in one of two cases:

1. If the reducer was successful and altered any of our subscribed rows.
2. If we requested an invocation which failed.

Note that a status of `Failed` or `OutOfEnergy` implies that the caller identity is our own identity.

We already handle successful `set_name` invocations using our `ctx.db.user().on_update(..)` callback, but if the module rejects a user's chosen name, we'd like that user's client to let them know. We define a function `on_set_name` as a `conn.reducers.on_set_name(..)` callback which checks if the reducer failed, and if it did, prints a message including the rejected name and the error.

To `src/main.rs`, add:

```rust
/// Our `on_set_name` callback: print a warning if the reducer failed.
fn on_name_set(ctx: &ReducerEventContext, name: &String) {
    if let Status::Failed(err) = &ctx.event.status {
        eprintln!("Failed to change name to {:?}: {}", name, err);
    }
}

/// Our `on_send_message` callback: print a warning if the reducer failed.
fn on_message_sent(ctx: &ReducerEventContext, text: &String) {
    if let Status::Failed(err) = &ctx.event.status {
        eprintln!("Failed to send message {:?}: {}", text, err);
    }
}
```

### Subscribe to queries

SpacetimeDB is set up so that each client subscribes via SQL queries to some subset of the database, and is notified about changes only to that subset. For complex apps with large databases, judicious subscriptions can save each client significant network bandwidth, memory and computation. For example, in [BitCraft](https://bitcraftonline.com), each player's client subscribes only to the entities in the "chunk" of the world where that player currently resides, rather than the entire game world. Our app is much simpler than BitCraft, so we'll just subscribe to the whole database.

When we specify our subscriptions, we can supply an `on_applied` callback. This will run when the subscription is applied and the matching rows become available in our client cache. We'll use this opportunity to print the message backlog in proper order.

We'll also provide an `on_error` callback. This will run if the subscription fails, usually due to an invalid or malformed SQL queries. We can't handle this case, so we'll just print out the error and exit the process.

To `src/main.rs`, add:

```rust
/// Register subscriptions for all rows of both tables.
fn subscribe_to_tables(ctx: &DbConnection) {
    ctx.subscription_builder()
        .on_applied(on_sub_applied)
        .on_error(on_sub_error)
        .subscribe(["SELECT * FROM user", "SELECT * FROM message"]);
}
```

#### Print past messages in order

Messages we receive live will come in order, but when we connect, we'll receive all the past messages at once. We can't just print these in the order we receive them; the logs would be all shuffled around, and would make no sense. Instead, when we receive the log of past messages, we'll sort them by their sent timestamps and print them in order.

We'll handle this in our function `print_messages_in_order`, which we registered as an `on_applied` callback. `print_messages_in_order` iterates over all the `Message`s we've received, sorts them, and then prints them. `ctx.db.message().iter()` is defined on the trait `Table`, and returns an iterator over all the messages in the client cache. Rust iterators can't be sorted in-place, so we'll collect it to a `Vec`, then use the `sort_by_key` method to sort by timestamp.

To `src/main.rs`, add:

```rust
/// Our `on_subscription_applied` callback:
/// sort all past messages and print them in timestamp order.
fn on_sub_applied(ctx: &SubscriptionEventContext) {
    let mut messages = ctx.db.message().iter().collect::<Vec<_>>();
    messages.sort_by_key(|m| m.sent);
    for message in messages {
        print_message(ctx, &message);
    }
    println!("Fully connected and all subscriptions applied.");
    println!("Use /name to set your name, or type a message!");
}
```

#### Notify about failed subscriptions

It's possible for SpacetimeDB to reject subscriptions. This happens most often because of a typo in the SQL queries, but can be due to use of SQL features that SpacetimeDB doesn't support. See [SQL Support: Subscriptions](/reference/sql#subscriptions) for more information about what subscription queries SpacetimeDB supports.

In our case, we're pretty confident that our queries are valid, but if SpacetimeDB rejects them, we want to know about it. Our callback will print the error, then exit the process.

```rust
/// Or `on_error` callback:
/// print the error, then exit the process.
fn on_sub_error(_ctx: &ErrorContext, err: Error) {
    eprintln!("Subscription failed: {}", err);
    std::process::exit(1);
}
```

### Handle user input

Our app should allow the user to interact by typing lines into their terminal. If the line starts with `/name`, we'll change the user's name. Any other line will send a message.

For each reducer defined by our module, `ctx.reducers` has a method to request an invocation. In our case, we pass `set_name` and `send_message` a `String`, which gets sent to the server to execute the corresponding reducer.

To `src/main.rs`, add:

```rust
/// Read each line of standard input, and either set our name or send a message as appropriate.
fn user_input_loop(ctx: &DbConnection) {
    for line in std::io::stdin().lines() {
        let Ok(line) = line else {
            panic!("Failed to read from stdin.");
        };
        if let Some(name) = line.strip_prefix("/name ") {
            ctx.reducers.set_name(name.to_string()).unwrap();
        } else {
            ctx.reducers.send_message(line).unwrap();
        }
    }
}
```

### Run it

After setting everything up, compile and run the client. From the `quickstart-chat` directory, run:

```bash
cargo run
```

You should see something like:

```
User d9e25c51996dea2f connected.
```

Now try sending a message by typing `Hello, world!` and pressing enter. You should see:

```
d9e25c51996dea2f: Hello, world!
```

Next, set your name by typing `/name <my-name>`, replacing `<my-name>` with your desired username. You should see:

```
User d9e25c51996dea2f renamed to <my-name>.
```

Then, send another message:

```
<my-name>: Hello after naming myself.
```

Now, close the app by hitting `Ctrl+C`, and start it again with `cargo run`. You'll see yourself connecting, and your past messages will load in order:

```
User <my-name> connected.
<my-name>: Hello, world!
<my-name>: Hello after naming myself.
```

</TabItem>
</Tabs>

## What's next?

Congratulations! You've built a chat app with SpacetimeDB.

- Check out the [SDK Reference documentation](/sdks) for more advanced usage
- Explore the [Unity Tutorial](/docs/tutorials/unity) or [Unreal Tutorial](/docs/tutorials/unreal) for game development
- Learn about [Procedures](/functions/procedures) for making external API calls

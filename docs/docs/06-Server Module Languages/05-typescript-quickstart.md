---
title: TypeScript Quickstart
slug: /modules/typescript/quickstart
---

# TypeScript Module Quickstart

In this tutorial, we'll implement a simple chat server as a SpacetimeDB **TypeScript** module.

A SpacetimeDB module is code that gets bundled to a single JavaScript artifact and uploaded to SpacetimeDB. This code becomes server-side logic that interfaces directly with SpacetimeDB’s relational database.

Each SpacetimeDB module defines a set of **tables** and a set of **reducers**.

- Tables are declared with `table({ ...opts }, { ...columns })`. Each inserted object is a row; each field is a column.
- Tables are **private** by default (readable only by the owner and your module code). Set `{ public: true }` to make them readable by everyone; writes still happen only via reducers.
- A **reducer** is a function that reads/writes the database. Each reducer runs in its own transaction; its writes commit only if it completes without throwing. In TypeScript, reducers are registered with `spacetimedb.reducer(name, argTypes, handler)` and throw `new SenderError("...")` for user-visible errors.

:::note
SpacetimeDB runs your module inside the database host (not Node.js). There’s no direct filesystem or network access from reducers.
:::

## Install SpacetimeDB

If you haven’t already, start by [installing SpacetimeDB](https://spacetimedb.com/install). This installs the `spacetime` CLI used to build, publish, and interact with your database.

## Project structure

Let's start by running `spacetime init` to initialize our project's directory structure:

```bash
spacetime init --lang typescript quickstart-chat
```

`spacetime init` will ask you for a project path in which to put your project. By default this will be `./quickstart-chat`. This basic project will have a few helper files like Cursor rules for SpacetimeDB and a `spacetimedb` directory which is where your SpacetimeDB module code will go.

Inside the `spacetimedb/` directory will be a `src/index.ts` entrypoint (required for publishing).

## Declare imports

Open `spacetimedb/src/index.ts`. Replace its contents with the following imports to start building a bare-bones real-time chat server:

```ts
import { schema, t, table, SenderError } from 'spacetimedb/server';
```

From `spacetimedb/server`, we import:

- `table` to define SpacetimeDB tables.
- `t` for column/type builders.
- `schema` to compose our database schema and register reducers.
- `SenderError` to signal user-visible (transaction-aborting) errors.

## Define tables

We’ll store two kinds of data: information about each user, and the messages that have been sent.

For each `User`, we’ll store their `identity` (the caller’s unique identifier), an optional display `name`, and whether they’re currently `online`. We’ll use `identity` as the primary key (unique and indexed).

Add to `spacetimedb/src/index.ts`:

```ts
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

## Set users’ names

We’ll allow users to set a display name, since raw identities aren’t user-friendly. Define a reducer `set_name` that validates input, looks up the caller’s `User` row by primary key, and updates it. If there’s no user row (e.g., the caller invoked via CLI without a connection and hasn’t connected before), we’ll return an error.

Add:

```ts
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

You can extend `validateName` with moderation checks, Unicode normalization, printable-character filtering, max length checks, or duplicate-name rejection.

## Send messages

Define a reducer `send_message` to insert a new `Message` with the caller’s identity and the call timestamp. As with names, we’ll validate that text isn’t empty.

Add:

```ts
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

Possible extensions:

- Reject messages from users who haven’t set a name.
- Rate-limit messages per user.

## Set users’ online status

SpacetimeDB can invoke lifecycle reducers when clients connect/disconnect. We’ll create or update a `User` row to mark the caller online on connect, and mark them offline on disconnect.

Add:

```ts
// Called once when the module bundle is installed / updated.
// We'll keep it empty for this quickstart.
spacetimedb.init(_ctx => {});

spacetimedb.clientConnected(ctx => {
  const user = ctx.db.user.identity.find(ctx.sender);
  if (user) {
    // Returning user: set online=true, keep identity/name.
    ctx.db.user.identity.update({ ...user, online: true });
  } else {
    // New user: create a User row with no name yet.
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
    // Shouldn't happen (disconnect without prior connect)
    console.warn(
      `Disconnect event for unknown user with identity ${ctx.sender}`
    );
  }
});
```

## Start the server

If you haven’t already started the SpacetimeDB host on your machine, run this in a **separate terminal** and leave it running:

```bash
spacetime start
```

(If it’s already running, you can skip this step.)

## Publish the module

From the `spacetimedb/` directory you can lint/typecheck locally if you like, but to make the module live you’ll need to publish it to a database. Publishing bundles your TypeScript into a single artifact and installs it into the `quickstart-chat` database.

> [!IMPORTANT]
> TypeScript modules are built and published with the `spacetime` CLI. `spacetime publish` will transpile and bundle your server module for you starting with the `src/index.ts` entrypoint. If you bundle your js yourself, you can specify `spacetime publish --js-path <path-to-your-bundle-file>` when publishing.

From the `quickstart-chat` directory (the parent of `spacetimedb/`):

```bash
spacetime publish --server local --project-path spacetimedb quickstart-chat
```

You can choose any unique, URL-safe database name in place of `quickstart-chat`. The CLI will show the database **Identity** (a hex string) as well; you can use either the name or identity with CLI commands.

## Call reducers

Use the CLI to call reducers. Arguments are passed as JSON (strings may be given bare for single string parameters).

Send a message:

```bash
spacetime call --server local quickstart-chat send_message "Hello, World!"
```

Check that it ran by viewing logs (owner-only):

```bash
spacetime logs --server local quickstart-chat
```

You should see output similar to:

```text
<timestamp>  INFO: spacetimedb: Creating table `message`
<timestamp>  INFO: spacetimedb: Creating table `user`
<timestamp>  INFO: spacetimedb: Database initialized
<timestamp>  INFO: console: User 0x...: Hello, World!
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

## What’s next?

You can find a complete version of this module in the SpacetimeDB examples. Next, build a client that interacts with your module using your preferred SDK:

- [TypeScript client quickstart](/sdks/typescript/quickstart)
- [Rust client quickstart](/sdks/rust/quickstart)
- [C# client quickstart](/sdks/c-sharp/quickstart)

- Using Unity? Jump to the [Unity Comprehensive Tutorial](/unity/part-1).
- Using Unreal Engine? Check out the [Unreal Comprehensive Tutorial](/unreal/part-1).

You’ve just set up your first TypeScript module in SpacetimeDB—nice work!

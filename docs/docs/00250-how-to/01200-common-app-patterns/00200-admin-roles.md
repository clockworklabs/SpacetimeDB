---
title: Admin and moderator roles
slug: /how-to/app-patterns/admin-roles
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';
import { CppModuleVersionNotice } from "@site/src/components/CppModuleVersionNotice";

:::note Prerequisites
You need a module and a client connected to it. If you don't have them yet, follow the [quickstart](../../00100-intro/00100-getting-started/00100-getting-started.md) for your language first.
:::

In this guide, you'll add moderators and admins to a chat: moderators can delete any message, and admins can also clear the whole chat. You'll store each user's role in a private table, check it at the start of each reducer that needs it, and use a [view](../../00200-core-concepts/00200-functions/00500-views.md) to tell each client its own role.

## How it works

- A private `user_role` table holds the role of each user who has one: `Moderator` or `Admin`. Users without a row are regular users.
- Each reducer that needs a role checks the caller's role first, and returns an error if it's missing. Admins can do everything moderators can.
- The `set_role` and `remove_role` reducers give and take away roles.
- A `my_role` view returns the caller's role, so the client can show moderation buttons only to the users who can use them.

The checks in the reducers are what keep regular users out. Hiding a button on the client only changes what the user sees: anyone can call your reducers directly, from a modified client or the CLI.

## Define the tables

Add a `Role` type with two values, `Moderator` and `Admin`, and two private tables:

- `user_role` has one row per user with a role. Its primary key is the user's identity.
- `message` holds the chat messages, so moderators have something to moderate.

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
import { schema, table, t, SenderError, type ReducerCtx } from 'spacetimedb/server';

const role = t.enum('Role', ['Moderator', 'Admin']);

// The role of each user who has one. Users without a row are regular users.
const userRole = table(
  { name: 'user_role' },
  {
    identity: t.identity().primaryKey(),
    role,
  }
);

const message = table(
  { name: 'message' },
  {
    id: t.u64().primaryKey().autoInc(),
    sender: t.identity(),
    text: t.string(),
    sentAt: t.timestamp(),
  }
);

const spacetimedb = schema({ userRole, message });
export default spacetimedb;
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
using SpacetimeDB;

public static partial class Module
{
    [SpacetimeDB.Type]
    public enum Role
    {
        Moderator,
        Admin,
    }

    // The role of each user who has one. Users without a row are regular users.
    [SpacetimeDB.Table(Accessor = "UserRole")]
    public partial struct UserRole
    {
        [SpacetimeDB.PrimaryKey]
        public Identity Identity;
        public Role Role;
    }

    [SpacetimeDB.Table(Accessor = "Message")]
    public partial struct Message
    {
        [SpacetimeDB.PrimaryKey]
        [SpacetimeDB.AutoInc]
        public ulong Id;
        public Identity Sender;
        public string Text;
        public Timestamp SentAt;
    }
}
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
use spacetimedb::{
    reducer, table, view, AnonymousViewContext, Identity, Query, ReducerContext, SpacetimeType, Table, Timestamp,
    ViewContext,
};

#[derive(SpacetimeType, Clone, Copy, PartialEq)]
pub enum Role {
    Moderator,
    Admin,
}

// The role of each user who has one. Users without a row are regular users.
#[table(accessor = user_role)]
pub struct UserRole {
    #[primary_key]
    identity: Identity,
    role: Role,
}

#[table(accessor = message)]
pub struct Message {
    #[primary_key]
    #[auto_inc]
    id: u64,
    sender: Identity,
    text: String,
    sent_at: Timestamp,
}
```

</TabItem>
<TabItem value="cpp" label="C++">

<CppModuleVersionNotice />

```cpp
#include <spacetimedb.h>
using namespace SpacetimeDB;

SPACETIMEDB_ENUM(Role, Moderator, Admin)

// The role of each user who has one. Users without a row are regular users.
struct UserRole {
    Identity identity;
    Role role;
};
SPACETIMEDB_STRUCT(UserRole, identity, role)
SPACETIMEDB_TABLE(UserRole, user_role, Private)
FIELD_PrimaryKey(user_role, identity)

struct Message {
    uint64_t id;
    Identity sender;
    std::string text;
    Timestamp sent_at;
};
SPACETIMEDB_STRUCT(Message, id, sender, text, sent_at)
SPACETIMEDB_TABLE(Message, message, Private)
FIELD_PrimaryKeyAutoInc(message, id)
```

</TabItem>
</Tabs>

## Check the caller's role

Add two helpers that reducers call before doing anything else:

- `require_moderator` returns an error unless the caller is a moderator or an admin.
- `require_admin` returns an error unless the caller is an admin.

Then add the chat's reducers:

- Anyone can call `send_message`.
- `delete_message` starts with `require_moderator`, so only moderators and admins can delete a message.
- `clear_messages` starts with `require_admin`, so only admins can delete every message at once.

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
function requireModerator(ctx: ReducerCtx<typeof spacetimedb.schemaType>) {
  const row = ctx.db.userRole.identity.find(ctx.sender);
  if (!row) {
    throw new SenderError('Only moderators can do this');
  }
}

function requireAdmin(ctx: ReducerCtx<typeof spacetimedb.schemaType>) {
  const row = ctx.db.userRole.identity.find(ctx.sender);
  if (row?.role.tag !== 'Admin') {
    throw new SenderError('Only admins can do this');
  }
}

export const sendMessage = spacetimedb.reducer({ text: t.string() }, (ctx, { text }) => {
  if (text === '') {
    throw new SenderError("A message can't be empty");
  }
  ctx.db.message.insert({ id: 0n, sender: ctx.sender, text, sentAt: ctx.timestamp });
});

export const deleteMessage = spacetimedb.reducer({ id: t.u64() }, (ctx, { id }) => {
  requireModerator(ctx);
  if (!ctx.db.message.id.delete(id)) {
    throw new SenderError('No such message');
  }
});

export const clearMessages = spacetimedb.reducer(ctx => {
  requireAdmin(ctx);
  ctx.db.message.clear();
});
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
public static partial class Module
{
    static void RequireModerator(ReducerContext ctx)
    {
        if (ctx.Db.UserRole.Identity.Find(ctx.Sender) is null)
        {
            throw new Exception("Only moderators can do this");
        }
    }

    static void RequireAdmin(ReducerContext ctx)
    {
        if (ctx.Db.UserRole.Identity.Find(ctx.Sender) is not { Role: Role.Admin })
        {
            throw new Exception("Only admins can do this");
        }
    }

    [SpacetimeDB.Reducer]
    public static void SendMessage(ReducerContext ctx, string text)
    {
        if (text == "")
        {
            throw new Exception("A message can't be empty");
        }
        ctx.Db.Message.Insert(new Message { Id = 0, Sender = ctx.Sender, Text = text, SentAt = ctx.Timestamp });
    }

    [SpacetimeDB.Reducer]
    public static void DeleteMessage(ReducerContext ctx, ulong id)
    {
        RequireModerator(ctx);
        if (!ctx.Db.Message.Id.Delete(id))
        {
            throw new Exception("No such message");
        }
    }

    [SpacetimeDB.Reducer]
    public static void ClearMessages(ReducerContext ctx)
    {
        RequireAdmin(ctx);
        ctx.Db.Message.Clear();
    }
}
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
fn require_moderator(ctx: &ReducerContext) -> Result<(), String> {
    match ctx.db.user_role().identity().find(ctx.sender()) {
        Some(_) => Ok(()),
        None => Err("only moderators can do this".to_string()),
    }
}

fn require_admin(ctx: &ReducerContext) -> Result<(), String> {
    match ctx.db.user_role().identity().find(ctx.sender()) {
        Some(row) if row.role == Role::Admin => Ok(()),
        _ => Err("only admins can do this".to_string()),
    }
}

#[reducer]
pub fn send_message(ctx: &ReducerContext, text: String) -> Result<(), String> {
    if text.is_empty() {
        return Err("a message can't be empty".to_string());
    }
    ctx.db.message().insert(Message {
        id: 0,
        sender: ctx.sender(),
        text,
        sent_at: ctx.timestamp,
    });
    Ok(())
}

#[reducer]
pub fn delete_message(ctx: &ReducerContext, id: u64) -> Result<(), String> {
    require_moderator(ctx)?;
    if !ctx.db.message().id().delete(id) {
        return Err("no such message".to_string());
    }
    Ok(())
}

#[reducer]
pub fn clear_messages(ctx: &ReducerContext) -> Result<(), String> {
    require_admin(ctx)?;
    ctx.db.message().clear();
    Ok(())
}
```

</TabItem>
<TabItem value="cpp" label="C++">

```cpp
Outcome<void> require_moderator(ReducerContext& ctx) {
    if (!ctx.db[user_role_identity].find(ctx.sender())) {
        return Err("only moderators can do this");
    }
    return Ok();
}

Outcome<void> require_admin(ReducerContext& ctx) {
    auto row = ctx.db[user_role_identity].find(ctx.sender());
    if (!row || row->role != Role::Admin) {
        return Err("only admins can do this");
    }
    return Ok();
}

SPACETIMEDB_REDUCER(send_message, ReducerContext ctx, std::string text) {
    if (text.empty()) {
        return Err("a message can't be empty");
    }
    ctx.db[message].insert(Message{0, ctx.sender(), text, ctx.timestamp});
    return Ok();
}

SPACETIMEDB_REDUCER(delete_message, ReducerContext ctx, uint64_t id) {
    auto allowed = require_moderator(ctx);
    if (allowed.is_err()) {
        return allowed;
    }
    if (!ctx.db[message_id].delete_by_key(id)) {
        return Err("no such message");
    }
    return Ok();
}

SPACETIMEDB_REDUCER(clear_messages, ReducerContext ctx) {
    auto allowed = require_admin(ctx);
    if (allowed.is_err()) {
        return allowed;
    }
    ctx.db[message].clear();
    return Ok();
}
```

</TabItem>
</Tabs>

## Give and take away roles

Add two reducers:

- `set_role` gives a user a role, or changes the role they have.
- `remove_role` takes a user's role away, which makes them a regular user again.

:::warning
To keep this guide short, anyone can call `set_role` and `remove_role`, so anyone can make themselves an admin. Before you ship, decide who can give roles, and check it at the start of both reducers.
:::

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
export const setRole = spacetimedb.reducer({ identity: t.identity(), role }, (ctx, { identity, role }) => {
  if (ctx.db.userRole.identity.find(identity)) {
    ctx.db.userRole.identity.update({ identity, role });
  } else {
    ctx.db.userRole.insert({ identity, role });
  }
});

export const removeRole = spacetimedb.reducer({ identity: t.identity() }, (ctx, { identity }) => {
  ctx.db.userRole.identity.delete(identity);
});
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
public static partial class Module
{
    [SpacetimeDB.Reducer]
    public static void SetRole(ReducerContext ctx, Identity identity, Role role)
    {
        var row = new UserRole { Identity = identity, Role = role };
        if (ctx.Db.UserRole.Identity.Find(identity) is null)
        {
            ctx.Db.UserRole.Insert(row);
        }
        else
        {
            ctx.Db.UserRole.Identity.Update(row);
        }
    }

    [SpacetimeDB.Reducer]
    public static void RemoveRole(ReducerContext ctx, Identity identity)
    {
        ctx.Db.UserRole.Identity.Delete(identity);
    }
}
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
#[reducer]
pub fn set_role(ctx: &ReducerContext, identity: Identity, role: Role) {
    let row = UserRole { identity, role };
    if ctx.db.user_role().identity().find(identity).is_some() {
        ctx.db.user_role().identity().update(row);
    } else {
        ctx.db.user_role().insert(row);
    }
}

#[reducer]
pub fn remove_role(ctx: &ReducerContext, identity: Identity) {
    ctx.db.user_role().identity().delete(identity);
}
```

</TabItem>
<TabItem value="cpp" label="C++">

```cpp
SPACETIMEDB_REDUCER(set_role, ReducerContext ctx, Identity identity, Role role) {
    UserRole row{identity, role};
    if (ctx.db[user_role_identity].find(identity)) {
        ctx.db[user_role_identity].update(row);
    } else {
        ctx.db[user_role].insert(row);
    }
    return Ok();
}

SPACETIMEDB_REDUCER(remove_role, ReducerContext ctx, Identity identity) {
    ctx.db[user_role_identity].delete_by_key(identity);
    return Ok();
}
```

</TabItem>
</Tabs>

## Define the views

Add two public views:

- `my_role` returns the caller's row in `user_role`, so each client knows its own role. It depends on who's asking, so it isn't anonymous: each user gets their own result.
- `messages` returns every message. It's anonymous, so SpacetimeDB computes it once and shares it with every client.

Both return a query, which SpacetimeDB keeps up to date the same way as a subscription query: when a user's role changes, their `my_role` changes right away.

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
// The caller's role, if they have one.
export const myRole = spacetimedb.view({ name: 'my_role', public: true }, t.array(userRole.rowType), ctx =>
  ctx.from.userRole.where(r => r.identity.eq(ctx.sender))
);

export const messages = spacetimedb.anonymousView({ name: 'messages', public: true }, t.array(message.rowType), ctx =>
  ctx.from.message
);
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
public static partial class Module
{
    // The caller's role, if they have one.
    [SpacetimeDB.View(Accessor = "MyRole", Public = true)]
    public static IQuery<UserRole> MyRole(ViewContext ctx)
    {
        return ctx.From.UserRole().Where(r => r.Identity.Eq(ctx.Sender));
    }

    [SpacetimeDB.View(Accessor = "Messages", Public = true)]
    public static IQuery<Message> Messages(AnonymousViewContext ctx)
    {
        return ctx.From.Message();
    }
}
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
// The caller's role, if they have one.
#[view(accessor = my_role, public)]
fn my_role(ctx: &ViewContext) -> impl Query<UserRole> {
    let me = ctx.sender();
    ctx.from.user_role().r#where(move |r| r.identity.eq(me))
}

#[view(accessor = messages, public)]
fn messages(ctx: &AnonymousViewContext) -> impl Query<Message> {
    ctx.from.message()
}
```

</TabItem>
<TabItem value="cpp" label="C++">

```cpp
// The caller's role, if they have one.
SPACETIMEDB_VIEW(Query<UserRole>, my_role, Public, ViewContext ctx) {
    Identity me = ctx.sender();
    return ctx.from[user_role].where([me](const auto& r) {
        return r.identity.eq(me);
    });
}

SPACETIMEDB_VIEW(Query<Message>, messages, Public, AnonymousViewContext ctx) {
    return ctx.from[message];
}
```

</TabItem>
</Tabs>

## Show moderation tools on the client

On the client, subscribe to `my_role` and `messages`. Show a delete button next to each message only to moderators and admins, and a button to clear the chat only to admins. When the user's role changes, `my_role` changes too, and the buttons appear or disappear without reconnecting.

<Tabs groupId="client-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
import { DbConnection, tables } from './module_bindings';

const conn = DbConnection.builder()
  .withUri('ws://localhost:3000')
  .withDatabaseName('my-app')
  .onConnect((ctx, identity) => {
    console.log(`Connected as ${identity.toHexString()}`);
    ctx.subscriptionBuilder().subscribe([tables.myRole, tables.messages]);
  })
  .build();

// The user's role: 'Moderator', 'Admin', or undefined for a regular user.
function myRole() {
  for (const row of conn.db.myRole.iter()) {
    return row.role.tag;
  }
  return undefined;
}

// Call this each frame to draw the chat, oldest message first.
function drawChat() {
  const role = myRole();
  if (role === 'Admin') {
    console.log('[clear chat]');
  }
  const messages = Array.from(conn.db.messages.iter()).sort((a, b) => (a.id < b.id ? -1 : 1));
  for (const message of messages) {
    console.log(role ? `${message.text} [delete #${message.id}]` : message.text);
  }
}

// Call this when a moderator clicks a message's delete button.
function deleteMessage(id: bigint) {
  conn.reducers.deleteMessage({ id });
}

// Call this when an admin clicks the clear chat button.
function clearChat() {
  conn.reducers.clearMessages({});
}
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
using SpacetimeDB;
using SpacetimeDB.Types;

var conn = DbConnection.Builder()
    .WithUri("http://localhost:3000")
    .WithDatabaseName("my-app")
    .OnConnect((conn, identity, token) =>
    {
        Console.WriteLine($"Connected as {identity}");
        conn.SubscriptionBuilder()
            .AddQuery(q => q.From.MyRole())
            .AddQuery(q => q.From.Messages())
            .Subscribe();
    })
    .Build();

// The user's role, or null for a regular user.
Role? MyRole()
{
    foreach (var row in conn.Db.MyRole.Iter())
    {
        return row.Role;
    }
    return null;
}

// Call this each frame to draw the chat, oldest message first.
void DrawChat()
{
    var role = MyRole();
    if (role == Role.Admin)
    {
        Console.WriteLine("[clear chat]");
    }
    foreach (var message in conn.Db.Messages.Iter().OrderBy(m => m.Id))
    {
        Console.WriteLine(role != null ? $"{message.Text} [delete #{message.Id}]" : message.Text);
    }
}

// Call this when a moderator clicks a message's delete button.
void DeleteMessage(ulong id) => conn.Reducers.DeleteMessage(id);

// Call this when an admin clicks the clear chat button.
void ClearChat() => conn.Reducers.ClearMessages();
```

Remember to call `conn.FrameTick()` regularly, for example once per frame, so the callbacks run.

</TabItem>
<TabItem value="rust" label="Rust">

```rust
mod module_bindings;
use module_bindings::*;
use spacetimedb_sdk::{DbContext, Table};

// The user's role, or `None` for a regular user.
fn my_role(conn: &DbConnection) -> Option<Role> {
    conn.db().my_role().iter().next().map(|row| row.role)
}

// Call this each frame to draw the chat, oldest message first.
fn draw_chat(conn: &DbConnection) {
    let role = my_role(conn);
    if role == Some(Role::Admin) {
        println!("[clear chat]");
    }
    let mut messages: Vec<_> = conn.db().messages().iter().collect();
    messages.sort_by_key(|message| message.id);
    for message in messages {
        if role.is_some() {
            println!("{} [delete #{}]", message.text, message.id);
        } else {
            println!("{}", message.text);
        }
    }
}

// Call this when a moderator clicks a message's delete button.
fn delete_message(conn: &DbConnection, id: u64) {
    conn.reducers().delete_message(id).unwrap();
}

// Call this when an admin clicks the clear chat button.
fn clear_chat(conn: &DbConnection) {
    conn.reducers().clear_messages().unwrap();
}

fn main() {
    let conn = DbConnection::builder()
        .with_uri("http://localhost:3000")
        .with_database_name("my-app")
        .on_connect(|ctx, identity, _token| {
            println!("Connected as {identity}");
            ctx.subscription_builder()
                .add_query(|q| q.from.my_role())
                .add_query(|q| q.from.messages())
                .subscribe();
        })
        .build()
        .expect("failed to connect");

    // Process messages from the database on a background thread.
    conn.run_threaded();

    // Your app's main loop runs here.
}
```

</TabItem>
</Tabs>

## Give yourself a role

To try the moderation tools, give your app's identity a role from the CLI:

1. Run your app, and copy the identity it prints when it connects.
2. Call `set_role` with that identity, prefixed with `0x`. Role values are written in lowercase, `moderator` or `admin`:

```bash
spacetime call my-app set_role 0x<your app identity> '{"admin": []}'
```

Your app's `my_role` changes as soon as the call succeeds, and the moderation tools appear.

## What's next?

You now have moderators and admins in your app:

- Roles live in a private table, and each reducer that needs one checks it before doing anything else.
- A `my_role` view tells each client its own role, so the client only shows the tools the user can use.

To adapt it to your app:

- **Protect `set_role` and `remove_role`.** Decide who can give roles, for example only admins, and check it at the start of both reducers, the same way `clear_messages` does.
- **Ban users.** Add a private `banned_user` table that moderators fill, and check it at the start of `send_message`.
- **Keep a log of moderation actions.** Insert a row into a private `moderation_log` table in each moderation reducer, with who did what and when.
- **Show who the moderators are.** Add an anonymous view that returns every row of `user_role`, so clients can show a badge next to moderators' messages.

To learn more about the features used here, see [Access Permissions](../../00200-core-concepts/00300-tables/00400-access-permissions.md), [Reducers](../../00200-core-concepts/00200-functions/00300-reducers/00300-reducers.md) and [Views](../../00200-core-concepts/00200-functions/00500-views.md).

Next, you can [send an email from your module](./00400-send-email.md).

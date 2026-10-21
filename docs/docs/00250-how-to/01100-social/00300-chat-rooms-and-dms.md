---
title: Chat rooms and private DMs
slug: /how-to/social/chat-rooms-and-dms
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';
import { CppModuleVersionNotice } from "@site/src/components/CppModuleVersionNotice";

:::note Prerequisites
You need a module and a client connected to it. If you don't have them yet, follow the [quickstart](../../00100-intro/00100-getting-started/00100-getting-started.md) for your language first. This guide uses a minimal `player` table with a name, and doesn't cover how players get their row. To let players pick a unique name and block offensive ones, see [Keep usernames unique and block offensive ones](./00100-unique-usernames.md): the code here works the same with its `player` table.

For a single chat that everyone can read, see the [chat app tutorial](../../00100-intro/00300-tutorials/00100-chat-app.md) instead.
:::

In this guide, you'll let players create and join chat rooms, and send each other direct messages (DMs). Each player only receives the messages of the rooms they're in, and their own DMs. You'll store messages in private tables, and use [views](../../00200-core-concepts/00200-functions/00500-views.md) built with the query builder to give each player their own messages.

## How it works

- A public `player` table holds each player's name. How a player gets their row is up to the rest of your module.
- A public `chat_room` table lists the rooms, so that players can find rooms to join.
- A private `room_member` table has one row for each player in each room.
- A private `message` table holds the messages of every room, and a private `direct_message` table holds every DM.
- Reducers create, join and leave rooms, and send messages to a room or to a player.
- Views return the rooms the caller is in, the messages of those rooms, and the DMs the caller sent and received.
- Clients subscribe to the room list and to the views, and sort the messages themselves.

The `room_member`, `message` and `direct_message` tables are private, so clients can't read who is in which room, or anyone else's messages. They only see the rows the views return, and each view only returns the caller's own rows.

## Define the tables

Add the five tables. The four new ones each have an `id` primary key that's assigned automatically, in the order rows are created. `room_member` has an index on `room_id` and one on `member`, `message` has an index on `room_id`, and `direct_message` has an index on `sender` and one on `recipient`. These indexes let the reducers and views find each player's rows without reading the whole table.

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
import type { Identity } from 'spacetimedb';
import { schema, table, t, SenderError, type InferSchema, type ReducerCtx } from 'spacetimedb/server';

const player = table(
  { name: 'player', public: true },
  {
    identity: t.identity().primaryKey(),
    name: t.string(),
  }
);

const chatRoom = table(
  { name: 'chat_room', public: true },
  {
    id: t.u64().primaryKey().autoInc(),
    name: t.string(),
  }
);

const roomMember = table(
  { name: 'room_member' },
  {
    id: t.u64().primaryKey().autoInc(),
    roomId: t.u64().index('btree'),
    member: t.identity().index('btree'),
  }
);

const message = table(
  { name: 'message' },
  {
    id: t.u64().primaryKey().autoInc(),
    roomId: t.u64().index('btree'),
    sender: t.identity(),
    text: t.string(),
    sent: t.timestamp(),
  }
);

const directMessage = table(
  { name: 'direct_message' },
  {
    id: t.u64().primaryKey().autoInc(),
    sender: t.identity().index('btree'),
    recipient: t.identity().index('btree'),
    text: t.string(),
    sent: t.timestamp(),
  }
);

const spacetimedb = schema({ player, chatRoom, roomMember, message, directMessage });
export default spacetimedb;

type Ctx = ReducerCtx<InferSchema<typeof spacetimedb>>;
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
using SpacetimeDB;

public static partial class Module
{
    [SpacetimeDB.Table(Accessor = "Player", Public = true)]
    public partial struct Player
    {
        [SpacetimeDB.PrimaryKey]
        public Identity Identity;
        public string Name;
    }

    [SpacetimeDB.Table(Accessor = "ChatRoom", Public = true)]
    public partial struct ChatRoom
    {
        [SpacetimeDB.PrimaryKey]
        [SpacetimeDB.AutoInc]
        public ulong Id;
        public string Name;
    }

    [SpacetimeDB.Table(Accessor = "RoomMember")]
    public partial struct RoomMember
    {
        [SpacetimeDB.PrimaryKey]
        [SpacetimeDB.AutoInc]
        public ulong Id;
        [SpacetimeDB.Index.BTree]
        public ulong RoomId;
        [SpacetimeDB.Index.BTree]
        public Identity Member;
    }

    [SpacetimeDB.Table(Accessor = "Message")]
    public partial struct Message
    {
        [SpacetimeDB.PrimaryKey]
        [SpacetimeDB.AutoInc]
        public ulong Id;
        [SpacetimeDB.Index.BTree]
        public ulong RoomId;
        public Identity Sender;
        public string Text;
        public Timestamp Sent;
    }

    [SpacetimeDB.Table(Accessor = "DirectMessage")]
    public partial struct DirectMessage
    {
        [SpacetimeDB.PrimaryKey]
        [SpacetimeDB.AutoInc]
        public ulong Id;
        [SpacetimeDB.Index.BTree]
        public Identity Sender;
        [SpacetimeDB.Index.BTree]
        public Identity Recipient;
        public string Text;
        public Timestamp Sent;
    }
}
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
use spacetimedb::{reducer, table, view, Identity, Query, ReducerContext, Table, Timestamp, ViewContext};

#[table(accessor = player, public)]
pub struct Player {
    #[primary_key]
    identity: Identity,
    name: String,
}

#[table(accessor = chat_room, public)]
pub struct ChatRoom {
    #[primary_key]
    #[auto_inc]
    id: u64,
    name: String,
}

#[table(accessor = room_member)]
pub struct RoomMember {
    #[primary_key]
    #[auto_inc]
    id: u64,
    #[index(btree)]
    room_id: u64,
    #[index(btree)]
    member: Identity,
}

#[table(accessor = message)]
pub struct Message {
    #[primary_key]
    #[auto_inc]
    id: u64,
    #[index(btree)]
    room_id: u64,
    sender: Identity,
    text: String,
    sent: Timestamp,
}

#[table(accessor = direct_message)]
pub struct DirectMessage {
    #[primary_key]
    #[auto_inc]
    id: u64,
    #[index(btree)]
    sender: Identity,
    #[index(btree)]
    recipient: Identity,
    text: String,
    sent: Timestamp,
}
```

</TabItem>
<TabItem value="cpp" label="C++">

<CppModuleVersionNotice />

```cpp
#include <spacetimedb.h>
using namespace SpacetimeDB;

struct Player {
    Identity identity;
    std::string name;
};
SPACETIMEDB_STRUCT(Player, identity, name)
SPACETIMEDB_TABLE(Player, player, Public)
FIELD_PrimaryKey(player, identity)

struct ChatRoom {
    uint64_t id;
    std::string name;
};
SPACETIMEDB_STRUCT(ChatRoom, id, name)
SPACETIMEDB_TABLE(ChatRoom, chat_room, Public)
FIELD_PrimaryKeyAutoInc(chat_room, id)

struct RoomMember {
    uint64_t id;
    uint64_t room_id;
    Identity member;
};
SPACETIMEDB_STRUCT(RoomMember, id, room_id, member)
SPACETIMEDB_TABLE(RoomMember, room_member, Private)
FIELD_PrimaryKeyAutoInc(room_member, id)
FIELD_Index(room_member, room_id)
FIELD_Index(room_member, member)

struct Message {
    uint64_t id;
    uint64_t room_id;
    Identity sender;
    std::string text;
    Timestamp sent;
};
SPACETIMEDB_STRUCT(Message, id, room_id, sender, text, sent)
SPACETIMEDB_TABLE(Message, message, Private)
FIELD_PrimaryKeyAutoInc(message, id)
FIELD_Index(message, room_id)

struct DirectMessage {
    uint64_t id;
    Identity sender;
    Identity recipient;
    std::string text;
    Timestamp sent;
};
SPACETIMEDB_STRUCT(DirectMessage, id, sender, recipient, text, sent)
SPACETIMEDB_TABLE(DirectMessage, direct_message, Private)
FIELD_PrimaryKeyAutoInc(direct_message, id)
FIELD_Index(direct_message, sender)
FIELD_Index(direct_message, recipient)
```

</TabItem>
</Tabs>

## Create, join and leave rooms

Add a `require_player` function that rejects players who don't have a `player` row yet, and a `find_membership` function that returns the row recording that a player is in a room, if there is one. `find_membership` reads the player's rows through the `member` index, and looks for the room among them. The C++ module also needs a `trim` function, to remove the spaces at both ends of a string.

Then add three reducers:

- `create_room` creates a room, and adds the caller to it. It rejects players who don't have a `player` row yet, and room names that are empty or longer than 50 characters, after removing the spaces at both ends.
- `join_room` adds the caller to a room. It rejects players who don't have a `player` row yet, and rooms that don't exist. Joining a room twice changes nothing, so a client that sends the request twice doesn't get an error.
- `leave_room` removes the caller from a room.

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
const MAX_ROOM_NAME_LENGTH = 50;

// Throws unless the caller has a `player` row.
function requirePlayer(ctx: Ctx) {
  if (!ctx.db.player.identity.find(ctx.sender)) {
    throw new SenderError('Pick a name first');
  }
}

// Returns the row that records `member` being in the room, if there is one.
function findMembership(ctx: Ctx, roomId: bigint, member: Identity) {
  for (const row of ctx.db.roomMember.member.filter(member)) {
    if (row.roomId === roomId) {
      return row;
    }
  }
  return undefined;
}

export const createRoom = spacetimedb.reducer({ name: t.string() }, (ctx, { name }) => {
  requirePlayer(ctx);
  const trimmed = name.trim();
  if (trimmed.length === 0 || trimmed.length > MAX_ROOM_NAME_LENGTH) {
    throw new SenderError(`Room names must be 1 to ${MAX_ROOM_NAME_LENGTH} characters`);
  }
  const room = ctx.db.chatRoom.insert({ id: 0n, name: trimmed });
  ctx.db.roomMember.insert({ id: 0n, roomId: room.id, member: ctx.sender });
});

export const joinRoom = spacetimedb.reducer({ roomId: t.u64() }, (ctx, { roomId }) => {
  requirePlayer(ctx);
  if (!ctx.db.chatRoom.id.find(roomId)) {
    throw new SenderError('Room not found');
  }
  if (findMembership(ctx, roomId, ctx.sender)) {
    return; // Already in the room.
  }
  ctx.db.roomMember.insert({ id: 0n, roomId, member: ctx.sender });
});

export const leaveRoom = spacetimedb.reducer({ roomId: t.u64() }, (ctx, { roomId }) => {
  const row = findMembership(ctx, roomId, ctx.sender);
  if (row) {
    ctx.db.roomMember.id.delete(row.id);
  }
});
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
public static partial class Module
{
    const int MaxRoomNameLength = 50;

    // Throws unless the caller has a `player` row.
    static void RequirePlayer(ReducerContext ctx)
    {
        if (ctx.Db.Player.Identity.Find(ctx.Sender) is null)
        {
            throw new Exception("Pick a name first");
        }
    }

    // Returns the row that records `member` being in the room, if there is one.
    static RoomMember? FindMembership(ReducerContext ctx, ulong roomId, Identity member)
    {
        foreach (var row in ctx.Db.RoomMember.Member.Filter(member))
        {
            if (row.RoomId == roomId)
            {
                return row;
            }
        }
        return null;
    }

    [SpacetimeDB.Reducer]
    public static void CreateRoom(ReducerContext ctx, string name)
    {
        RequirePlayer(ctx);
        var trimmed = name.Trim();
        if (trimmed.Length == 0 || trimmed.Length > MaxRoomNameLength)
        {
            throw new Exception($"Room names must be 1 to {MaxRoomNameLength} characters");
        }
        var room = ctx.Db.ChatRoom.Insert(new ChatRoom { Name = trimmed });
        ctx.Db.RoomMember.Insert(new RoomMember { RoomId = room.Id, Member = ctx.Sender });
    }

    [SpacetimeDB.Reducer]
    public static void JoinRoom(ReducerContext ctx, ulong roomId)
    {
        RequirePlayer(ctx);
        if (ctx.Db.ChatRoom.Id.Find(roomId) is null)
        {
            throw new Exception("Room not found");
        }
        if (FindMembership(ctx, roomId, ctx.Sender) is not null)
        {
            return; // Already in the room.
        }
        ctx.Db.RoomMember.Insert(new RoomMember { RoomId = roomId, Member = ctx.Sender });
    }

    [SpacetimeDB.Reducer]
    public static void LeaveRoom(ReducerContext ctx, ulong roomId)
    {
        if (FindMembership(ctx, roomId, ctx.Sender) is RoomMember row)
        {
            ctx.Db.RoomMember.Id.Delete(row.Id);
        }
    }
}
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
const MAX_ROOM_NAME_LENGTH: usize = 50;

/// Returns an error unless the caller has a `player` row.
fn require_player(ctx: &ReducerContext) -> Result<(), String> {
    if ctx.db.player().identity().find(ctx.sender()).is_none() {
        return Err("Pick a name first".to_string());
    }
    Ok(())
}

/// Returns the row that records `member` being in the room, if there is one.
fn find_membership(ctx: &ReducerContext, room_id: u64, member: Identity) -> Option<RoomMember> {
    ctx.db
        .room_member()
        .member()
        .filter(member)
        .find(|row| row.room_id == room_id)
}

#[reducer]
pub fn create_room(ctx: &ReducerContext, name: String) -> Result<(), String> {
    require_player(ctx)?;
    let name = name.trim();
    if name.is_empty() || name.chars().count() > MAX_ROOM_NAME_LENGTH {
        return Err(format!("Room names must be 1 to {MAX_ROOM_NAME_LENGTH} characters"));
    }
    let room = ctx.db.chat_room().insert(ChatRoom {
        id: 0,
        name: name.to_string(),
    });
    ctx.db.room_member().insert(RoomMember {
        id: 0,
        room_id: room.id,
        member: ctx.sender(),
    });
    Ok(())
}

#[reducer]
pub fn join_room(ctx: &ReducerContext, room_id: u64) -> Result<(), String> {
    require_player(ctx)?;
    if ctx.db.chat_room().id().find(room_id).is_none() {
        return Err("Room not found".to_string());
    }
    if find_membership(ctx, room_id, ctx.sender()).is_some() {
        return Ok(()); // Already in the room.
    }
    ctx.db.room_member().insert(RoomMember {
        id: 0,
        room_id,
        member: ctx.sender(),
    });
    Ok(())
}

#[reducer]
pub fn leave_room(ctx: &ReducerContext, room_id: u64) {
    if let Some(row) = find_membership(ctx, room_id, ctx.sender()) {
        ctx.db.room_member().id().delete(row.id);
    }
}
```

</TabItem>
<TabItem value="cpp" label="C++">

```cpp
// Lengths are counted in bytes, so names and messages that aren't plain ASCII hold fewer characters.
constexpr size_t MAX_ROOM_NAME_LENGTH = 50;

// Removes the spaces at both ends of `text`.
std::string trim(const std::string& text) {
    auto start = text.find_first_not_of(" \t\r\n");
    if (start == std::string::npos) {
        return "";
    }
    auto end = text.find_last_not_of(" \t\r\n");
    return text.substr(start, end - start + 1);
}

// Returns the row that records `member` being in the room, if there is one.
std::optional<RoomMember> find_membership(ReducerContext& ctx, uint64_t room_id, Identity member) {
    for (const auto& row : ctx.db[room_member_member].filter(member)) {
        if (row.room_id == room_id) {
            return row;
        }
    }
    return std::nullopt;
}

SPACETIMEDB_REDUCER(create_room, ReducerContext ctx, std::string name) {
    if (!ctx.db[player_identity].find(ctx.sender())) {
        return Err("Pick a name first");
    }
    auto trimmed = trim(name);
    if (trimmed.empty() || trimmed.size() > MAX_ROOM_NAME_LENGTH) {
        return Err("Room names must be 1 to " + std::to_string(MAX_ROOM_NAME_LENGTH) + " characters");
    }
    auto room = ctx.db[chat_room].insert(ChatRoom{0, trimmed});
    ctx.db[room_member].insert(RoomMember{0, room.id, ctx.sender()});
    return Ok();
}

SPACETIMEDB_REDUCER(join_room, ReducerContext ctx, uint64_t room_id) {
    if (!ctx.db[player_identity].find(ctx.sender())) {
        return Err("Pick a name first");
    }
    if (!ctx.db[chat_room_id].find(room_id)) {
        return Err("Room not found");
    }
    if (find_membership(ctx, room_id, ctx.sender())) {
        return Ok(); // Already in the room.
    }
    ctx.db[room_member].insert(RoomMember{0, room_id, ctx.sender()});
    return Ok();
}

SPACETIMEDB_REDUCER(leave_room, ReducerContext ctx, uint64_t room_id) {
    if (auto row = find_membership(ctx, room_id, ctx.sender())) {
        ctx.db[room_member_id].delete_by_key(row->id);
    }
    return Ok();
}
```

</TabItem>
</Tabs>

## Send messages

Add a `message_text` function that removes the spaces at both ends of a message, and rejects messages that are then empty or longer than 1,000 characters. Then add two reducers:

- `send_message` sends a message to a room. It rejects players who aren't in the room.
- `send_direct_message` sends a message to another player. It rejects players who don't have a `player` row yet, messages to yourself, and messages to a player who doesn't exist.

Both reducers set the sender to `ctx.sender` and the time to `ctx.timestamp`, so clients can't send messages in another player's name, or change when a message was sent.

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
const MAX_MESSAGE_LENGTH = 1000;

// Removes the spaces at both ends of a message, and checks its length.
function messageText(text: string) {
  const trimmed = text.trim();
  if (trimmed.length === 0 || trimmed.length > MAX_MESSAGE_LENGTH) {
    throw new SenderError(`Messages must be 1 to ${MAX_MESSAGE_LENGTH} characters`);
  }
  return trimmed;
}

export const sendMessage = spacetimedb.reducer(
  { roomId: t.u64(), text: t.string() },
  (ctx, { roomId, text }) => {
    if (!findMembership(ctx, roomId, ctx.sender)) {
      throw new SenderError("You're not in this room");
    }
    ctx.db.message.insert({
      id: 0n,
      roomId,
      sender: ctx.sender,
      text: messageText(text),
      sent: ctx.timestamp,
    });
  }
);

export const sendDirectMessage = spacetimedb.reducer(
  { recipient: t.identity(), text: t.string() },
  (ctx, { recipient, text }) => {
    requirePlayer(ctx);
    if (recipient.isEqual(ctx.sender)) {
      throw new SenderError("You can't message yourself");
    }
    if (!ctx.db.player.identity.find(recipient)) {
      throw new SenderError('Player not found');
    }
    ctx.db.directMessage.insert({
      id: 0n,
      sender: ctx.sender,
      recipient,
      text: messageText(text),
      sent: ctx.timestamp,
    });
  }
);
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
public static partial class Module
{
    const int MaxMessageLength = 1000;

    // Removes the spaces at both ends of a message, and checks its length.
    static string MessageText(string text)
    {
        var trimmed = text.Trim();
        if (trimmed.Length == 0 || trimmed.Length > MaxMessageLength)
        {
            throw new Exception($"Messages must be 1 to {MaxMessageLength} characters");
        }
        return trimmed;
    }

    [SpacetimeDB.Reducer]
    public static void SendMessage(ReducerContext ctx, ulong roomId, string text)
    {
        if (FindMembership(ctx, roomId, ctx.Sender) is null)
        {
            throw new Exception("You're not in this room");
        }
        ctx.Db.Message.Insert(new Message
        {
            RoomId = roomId,
            Sender = ctx.Sender,
            Text = MessageText(text),
            Sent = ctx.Timestamp,
        });
    }

    [SpacetimeDB.Reducer]
    public static void SendDirectMessage(ReducerContext ctx, Identity recipient, string text)
    {
        RequirePlayer(ctx);
        if (recipient == ctx.Sender)
        {
            throw new Exception("You can't message yourself");
        }
        if (ctx.Db.Player.Identity.Find(recipient) is null)
        {
            throw new Exception("Player not found");
        }
        ctx.Db.DirectMessage.Insert(new DirectMessage
        {
            Sender = ctx.Sender,
            Recipient = recipient,
            Text = MessageText(text),
            Sent = ctx.Timestamp,
        });
    }
}
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
const MAX_MESSAGE_LENGTH: usize = 1000;

/// Removes the spaces at both ends of a message, and checks its length.
fn message_text(text: &str) -> Result<String, String> {
    let text = text.trim();
    if text.is_empty() || text.chars().count() > MAX_MESSAGE_LENGTH {
        return Err(format!("Messages must be 1 to {MAX_MESSAGE_LENGTH} characters"));
    }
    Ok(text.to_string())
}

#[reducer]
pub fn send_message(ctx: &ReducerContext, room_id: u64, text: String) -> Result<(), String> {
    if find_membership(ctx, room_id, ctx.sender()).is_none() {
        return Err("You're not in this room".to_string());
    }
    ctx.db.message().insert(Message {
        id: 0,
        room_id,
        sender: ctx.sender(),
        text: message_text(&text)?,
        sent: ctx.timestamp,
    });
    Ok(())
}

#[reducer]
pub fn send_direct_message(ctx: &ReducerContext, recipient: Identity, text: String) -> Result<(), String> {
    require_player(ctx)?;
    if recipient == ctx.sender() {
        return Err("You can't message yourself".to_string());
    }
    if ctx.db.player().identity().find(recipient).is_none() {
        return Err("Player not found".to_string());
    }
    ctx.db.direct_message().insert(DirectMessage {
        id: 0,
        sender: ctx.sender(),
        recipient,
        text: message_text(&text)?,
        sent: ctx.timestamp,
    });
    Ok(())
}
```

</TabItem>
<TabItem value="cpp" label="C++">

```cpp
constexpr size_t MAX_MESSAGE_LENGTH = 1000;

// Removes the spaces at both ends of a message, and checks its length.
Outcome<std::string> message_text(const std::string& text) {
    auto trimmed = trim(text);
    if (trimmed.empty() || trimmed.size() > MAX_MESSAGE_LENGTH) {
        return Err<std::string>("Messages must be 1 to " + std::to_string(MAX_MESSAGE_LENGTH) + " characters");
    }
    return Ok(trimmed);
}

SPACETIMEDB_REDUCER(send_message, ReducerContext ctx, uint64_t room_id, std::string text) {
    if (!find_membership(ctx, room_id, ctx.sender())) {
        return Err("You're not in this room");
    }
    auto clean = message_text(text);
    if (!clean.is_ok()) {
        return Err(clean.error());
    }
    ctx.db[message].insert(Message{0, room_id, ctx.sender(), clean.value(), ctx.timestamp});
    return Ok();
}

SPACETIMEDB_REDUCER(send_direct_message, ReducerContext ctx, Identity recipient, std::string text) {
    if (!ctx.db[player_identity].find(ctx.sender())) {
        return Err("Pick a name first");
    }
    if (recipient == ctx.sender()) {
        return Err("You can't message yourself");
    }
    if (!ctx.db[player_identity].find(recipient)) {
        return Err("Player not found");
    }
    auto clean = message_text(text);
    if (!clean.is_ok()) {
        return Err(clean.error());
    }
    ctx.db[direct_message].insert(DirectMessage{0, ctx.sender(), recipient, clean.value(), ctx.timestamp});
    return Ok();
}
```

</TabItem>
</Tabs>

## Show each player their messages

Add four views:

- `my_rooms` returns the rooms the caller is in.
- `my_messages` returns the messages of the rooms the caller is in.
- `my_sent_dms` returns the DMs the caller sent, and `my_received_dms` the DMs the caller received. A single view that combines both filters with `or` would also work, but SpacetimeDB can't use an index for an `or`, so it would read every DM in the database each time a player subscribes. Two views each read only the caller's DMs, through their own index.

`my_rooms` and `my_messages` start from the caller's rows in `room_member`, then use a semijoin to keep the rooms, or the messages, that match them.

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
// The rooms the caller is in.
export const myRooms = spacetimedb.view(
  { name: 'my_rooms', public: true },
  t.array(chatRoom.rowType),
  ctx =>
    ctx.from.roomMember
      .where(m => m.member.eq(ctx.sender))
      .rightSemijoin(ctx.from.chatRoom, (m, room) => m.roomId.eq(room.id))
);

// The messages of the rooms the caller is in.
export const myMessages = spacetimedb.view(
  { name: 'my_messages', public: true },
  t.array(message.rowType),
  ctx =>
    ctx.from.roomMember
      .where(m => m.member.eq(ctx.sender))
      .rightSemijoin(ctx.from.message, (m, msg) => m.roomId.eq(msg.roomId))
);

// The DMs the caller sent.
export const mySentDms = spacetimedb.view(
  { name: 'my_sent_dms', public: true },
  t.array(directMessage.rowType),
  ctx => ctx.from.directMessage.where(dm => dm.sender.eq(ctx.sender))
);

// The DMs the caller received.
export const myReceivedDms = spacetimedb.view(
  { name: 'my_received_dms', public: true },
  t.array(directMessage.rowType),
  ctx => ctx.from.directMessage.where(dm => dm.recipient.eq(ctx.sender))
);
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
public static partial class Module
{
    // The rooms the caller is in.
    [SpacetimeDB.View(Accessor = "MyRooms", Public = true)]
    public static IQuery<ChatRoom> MyRooms(ViewContext ctx)
    {
        var me = ctx.Sender;
        return ctx.From.RoomMember()
            .Where(m => m.Member.Eq(me))
            .RightSemijoin(ctx.From.ChatRoom(), (m, room) => m.RoomId.Eq(room.Id));
    }

    // The messages of the rooms the caller is in.
    [SpacetimeDB.View(Accessor = "MyMessages", Public = true)]
    public static IQuery<Message> MyMessages(ViewContext ctx)
    {
        var me = ctx.Sender;
        return ctx.From.RoomMember()
            .Where(m => m.Member.Eq(me))
            .RightSemijoin(ctx.From.Message(), (m, msg) => m.RoomId.Eq(msg.RoomId));
    }

    // The DMs the caller sent.
    [SpacetimeDB.View(Accessor = "MySentDms", Public = true)]
    public static IQuery<DirectMessage> MySentDms(ViewContext ctx)
    {
        var me = ctx.Sender;
        return ctx.From.DirectMessage().Where(dm => dm.Sender.Eq(me));
    }

    // The DMs the caller received.
    [SpacetimeDB.View(Accessor = "MyReceivedDms", Public = true)]
    public static IQuery<DirectMessage> MyReceivedDms(ViewContext ctx)
    {
        var me = ctx.Sender;
        return ctx.From.DirectMessage().Where(dm => dm.Recipient.Eq(me));
    }
}
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
/// The rooms the caller is in.
#[view(accessor = my_rooms, public)]
fn my_rooms(ctx: &ViewContext) -> impl Query<ChatRoom> {
    let me = ctx.sender();
    ctx.from
        .room_member()
        .r#where(move |m| m.member.eq(me))
        .right_semijoin(ctx.from.chat_room(), |m, room| m.room_id.eq(room.id))
}

/// The messages of the rooms the caller is in.
#[view(accessor = my_messages, public)]
fn my_messages(ctx: &ViewContext) -> impl Query<Message> {
    let me = ctx.sender();
    ctx.from
        .room_member()
        .r#where(move |m| m.member.eq(me))
        .right_semijoin(ctx.from.message(), |m, msg| m.room_id.eq(msg.room_id))
}

/// The DMs the caller sent.
#[view(accessor = my_sent_dms, public)]
fn my_sent_dms(ctx: &ViewContext) -> impl Query<DirectMessage> {
    let me = ctx.sender();
    ctx.from.direct_message().r#where(move |dm| dm.sender.eq(me))
}

/// The DMs the caller received.
#[view(accessor = my_received_dms, public)]
fn my_received_dms(ctx: &ViewContext) -> impl Query<DirectMessage> {
    let me = ctx.sender();
    ctx.from.direct_message().r#where(move |dm| dm.recipient.eq(me))
}
```

</TabItem>
<TabItem value="cpp" label="C++">

```cpp
// The rooms the caller is in.
SPACETIMEDB_VIEW(Query<ChatRoom>, my_rooms, Public, ViewContext ctx) {
    auto me = ctx.sender();
    return ctx.from[room_member]
        .where([me](const auto& m) { return m.member.eq(me); })
        .right_semijoin(ctx.from[chat_room], [](const auto& m, const auto& room) {
            return m.room_id.eq(room.id);
        });
}

// The messages of the rooms the caller is in.
SPACETIMEDB_VIEW(Query<Message>, my_messages, Public, ViewContext ctx) {
    auto me = ctx.sender();
    return ctx.from[room_member]
        .where([me](const auto& m) { return m.member.eq(me); })
        .right_semijoin(ctx.from[message], [](const auto& m, const auto& msg) {
            return m.room_id.eq(msg.room_id);
        });
}

// The DMs the caller sent.
SPACETIMEDB_VIEW(Query<DirectMessage>, my_sent_dms, Public, ViewContext ctx) {
    auto me = ctx.sender();
    return ctx.from[direct_message].where([me](const auto& dm) { return dm.sender.eq(me); });
}

// The DMs the caller received.
SPACETIMEDB_VIEW(Query<DirectMessage>, my_received_dms, Public, ViewContext ctx) {
    auto me = ctx.sender();
    return ctx.from[direct_message].where([me](const auto& dm) { return dm.recipient.eq(me); });
}
```

</TabItem>
</Tabs>

These views return a query instead of rows, so SpacetimeDB evaluates them with its query engine. When a row changes, it works out which rows of each view changed, instead of running the view again. Each view depends on who asks, so SpacetimeDB keeps one per subscribed player, but each one only covers that player's rooms and DMs.

When a player joins a room, `my_messages` adds the room's earlier messages, so the player sees the room's history. When they leave, `my_messages` removes them again.

:::note
A semijoin can only match columns that have their own single-column index. That's why `room_member` and `message` both index `room_id`.
:::

## Show the messages on the client

On the client, subscribe to `player` and `chat_room`, and to the four views. `chat_room` lists every room, so players can pick one to join, and `my_rooms` tells you which ones they're in. The code below shows the room list when the player asks for it, for example when they open a room picker. `player` gives you the names of the senders.

Views don't keep their rows in any order, so sort the messages on the client. Each message's `id` is larger than the ids of the messages sent before it, so sort by `id`. To show a conversation with another player, merge the DMs you sent to them with the ones you received from them.

Show the open room or conversation again whenever a message is added or removed. Joining a room adds many messages at once, and leaving it removes them.

<Tabs groupId="client-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
import type { Identity } from 'spacetimedb';
import { DbConnection, tables } from './module_bindings';

// Shows what's on screen: a room or a conversation.
let render = () => {};

const conn = DbConnection.builder()
  .withUri('ws://localhost:3000')
  .withDatabaseName('my-game')
  .onConnect(ctx => {
    ctx
      .subscriptionBuilder()
      .onApplied(() => render())
      .subscribe([
        tables.player,
        tables.chatRoom,
        tables.myRooms,
        tables.myMessages,
        tables.mySentDms,
        tables.myReceivedDms,
      ]);
  })
  .build();

conn.db.myMessages.onInsert(() => render());
conn.db.myMessages.onDelete(() => render());
conn.db.mySentDms.onInsert(() => render());
conn.db.myReceivedDms.onInsert(() => render());

function nameOf(identity: Identity) {
  return conn.db.player.identity.find(identity)?.name ?? 'Unknown player';
}

function listRooms() {
  const joined = Array.from(conn.db.myRooms.iter());
  for (const room of conn.db.chatRoom.iter()) {
    const isJoined = joined.some(r => r.id === room.id);
    console.log(isJoined ? `${room.name} (joined)` : room.name);
  }
}

function showRoom(roomId: bigint) {
  render = () => {
    const messages = Array.from(conn.db.myMessages.iter())
      .filter(m => m.roomId === roomId)
      .sort((a, b) => (a.id < b.id ? -1 : 1));
    for (const m of messages) {
      console.log(`${nameOf(m.sender)}: ${m.text}`);
    }
  };
  render();
}

function showConversation(other: Identity) {
  render = () => {
    const sent = Array.from(conn.db.mySentDms.iter()).filter(m => m.recipient.isEqual(other));
    const received = Array.from(conn.db.myReceivedDms.iter()).filter(m => m.sender.isEqual(other));
    const messages = [...sent, ...received].sort((a, b) => (a.id < b.id ? -1 : 1));
    for (const m of messages) {
      console.log(`${nameOf(m.sender)}: ${m.text}`);
    }
  };
  render();
}

async function createRoom(name: string) {
  try {
    await conn.reducers.createRoom({ name });
  } catch (error) {
    console.error(`Could not create the room: ${error}`);
  }
}

function joinRoom(roomId: bigint) {
  conn.reducers.joinRoom({ roomId });
}

function leaveRoom(roomId: bigint) {
  conn.reducers.leaveRoom({ roomId });
}

async function sendToRoom(roomId: bigint, text: string) {
  try {
    await conn.reducers.sendMessage({ roomId, text });
  } catch (error) {
    console.error(`Could not send the message: ${error}`);
  }
}

async function sendToPlayer(recipient: Identity, text: string) {
  try {
    await conn.reducers.sendDirectMessage({ recipient, text });
  } catch (error) {
    console.error(`Could not send the message: ${error}`);
  }
}
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
using SpacetimeDB;
using SpacetimeDB.Types;

// Shows what's on screen: a room or a conversation.
Action render = () => { };

var conn = DbConnection.Builder()
    .WithUri("http://localhost:3000")
    .WithDatabaseName("my-game")
    .OnConnect((conn, identity, token) =>
    {
        conn.SubscriptionBuilder()
            .OnApplied(ctx => render())
            .AddQuery(q => q.From.Player())
            .AddQuery(q => q.From.ChatRoom())
            .AddQuery(q => q.From.MyRooms())
            .AddQuery(q => q.From.MyMessages())
            .AddQuery(q => q.From.MySentDms())
            .AddQuery(q => q.From.MyReceivedDms())
            .Subscribe();
    })
    .Build();

conn.Db.MyMessages.OnInsert += (ctx, row) => render();
conn.Db.MyMessages.OnDelete += (ctx, row) => render();
conn.Db.MySentDms.OnInsert += (ctx, row) => render();
conn.Db.MyReceivedDms.OnInsert += (ctx, row) => render();

conn.Reducers.OnCreateRoom += (ctx, name) =>
{
    if (ctx.Event.Status is Status.Failed(var error))
    {
        Console.WriteLine($"Could not create the room: {error}");
    }
};
conn.Reducers.OnSendMessage += (ctx, roomId, text) =>
{
    if (ctx.Event.Status is Status.Failed(var error))
    {
        Console.WriteLine($"Could not send the message: {error}");
    }
};
conn.Reducers.OnSendDirectMessage += (ctx, recipient, text) =>
{
    if (ctx.Event.Status is Status.Failed(var error))
    {
        Console.WriteLine($"Could not send the message: {error}");
    }
};

string NameOf(Identity identity) => conn.Db.Player.Identity.Find(identity)?.Name ?? "Unknown player";

void ListRooms()
{
    var joined = conn.Db.MyRooms.Iter().ToList();
    foreach (var room in conn.Db.ChatRoom.Iter())
    {
        var isJoined = joined.Any(r => r.Id == room.Id);
        Console.WriteLine(isJoined ? $"{room.Name} (joined)" : room.Name);
    }
}

void ShowRoom(ulong roomId)
{
    render = () =>
    {
        var messages = conn.Db.MyMessages.Iter().Where(m => m.RoomId == roomId).OrderBy(m => m.Id);
        foreach (var m in messages)
        {
            Console.WriteLine($"{NameOf(m.Sender)}: {m.Text}");
        }
    };
    render();
}

void ShowConversation(Identity other)
{
    render = () =>
    {
        var sent = conn.Db.MySentDms.Iter().Where(m => m.Recipient == other);
        var received = conn.Db.MyReceivedDms.Iter().Where(m => m.Sender == other);
        foreach (var m in sent.Concat(received).OrderBy(m => m.Id))
        {
            Console.WriteLine($"{NameOf(m.Sender)}: {m.Text}");
        }
    };
    render();
}

void CreateRoom(string name) => conn.Reducers.CreateRoom(name);
void JoinRoom(ulong roomId) => conn.Reducers.JoinRoom(roomId);
void LeaveRoom(ulong roomId) => conn.Reducers.LeaveRoom(roomId);
void SendToRoom(ulong roomId, string text) => conn.Reducers.SendMessage(roomId, text);
void SendToPlayer(Identity recipient, string text) => conn.Reducers.SendDirectMessage(recipient, text);
```

Remember to call `conn.FrameTick()` regularly, for example once per frame, so the callbacks run.

</TabItem>
<TabItem value="rust" label="Rust">

```rust
mod module_bindings;
use module_bindings::*;
use spacetimedb_sdk::{DbContext, Identity, Table};
use std::sync::Mutex;

/// What's on screen: a room or a conversation.
enum Screen {
    Room(u64),
    Conversation(Identity),
}

static SCREEN: Mutex<Option<Screen>> = Mutex::new(None);

fn name_of(db: &RemoteTables, identity: Identity) -> String {
    match db.player().identity().find(&identity) {
        Some(player) => player.name,
        None => "Unknown player".to_string(),
    }
}

/// Shows what's on screen.
fn render(db: &RemoteTables) {
    let lines: Vec<(Identity, String)> = match *SCREEN.lock().unwrap() {
        Some(Screen::Room(room_id)) => {
            let mut messages: Vec<Message> = db.my_messages().iter().filter(|m| m.room_id == room_id).collect();
            messages.sort_by_key(|m| m.id);
            messages.into_iter().map(|m| (m.sender, m.text)).collect()
        }
        Some(Screen::Conversation(other)) => {
            let mut messages: Vec<DirectMessage> = db
                .my_sent_dms()
                .iter()
                .filter(|m| m.recipient == other)
                .chain(db.my_received_dms().iter().filter(|m| m.sender == other))
                .collect();
            messages.sort_by_key(|m| m.id);
            messages.into_iter().map(|m| (m.sender, m.text)).collect()
        }
        None => return,
    };
    for (sender, text) in lines {
        println!("{}: {text}", name_of(db, sender));
    }
}

fn main() {
    let conn = DbConnection::builder()
        .with_uri("http://localhost:3000")
        .with_database_name("my-game")
        .on_connect(|ctx, _identity, _token| {
            ctx.subscription_builder()
                .on_applied(|ctx| render(&ctx.db))
                .add_query(|q| q.from.player())
                .add_query(|q| q.from.chat_room())
                .add_query(|q| q.from.my_rooms())
                .add_query(|q| q.from.my_messages())
                .add_query(|q| q.from.my_sent_dms())
                .add_query(|q| q.from.my_received_dms())
                .subscribe();
        })
        .build()
        .expect("failed to connect");

    conn.db().my_messages().on_insert(|ctx, _| render(&ctx.db));
    conn.db().my_messages().on_delete(|ctx, _| render(&ctx.db));
    conn.db().my_sent_dms().on_insert(|ctx, _| render(&ctx.db));
    conn.db().my_received_dms().on_insert(|ctx, _| render(&ctx.db));

    // Process messages from the database on a background thread.
    conn.run_threaded();

    // Your game loop runs here.
}

fn list_rooms(conn: &DbConnection) {
    let joined: Vec<ChatRoom> = conn.db().my_rooms().iter().collect();
    for room in conn.db().chat_room().iter() {
        if joined.iter().any(|r| r.id == room.id) {
            println!("{} (joined)", room.name);
        } else {
            println!("{}", room.name);
        }
    }
}

fn show_room(conn: &DbConnection, room_id: u64) {
    *SCREEN.lock().unwrap() = Some(Screen::Room(room_id));
    render(conn.db());
}

fn show_conversation(conn: &DbConnection, other: Identity) {
    *SCREEN.lock().unwrap() = Some(Screen::Conversation(other));
    render(conn.db());
}

fn create_room(conn: &DbConnection, name: String) {
    conn.reducers()
        .create_room_then(name, |_ctx, result| {
            if let Ok(Err(error)) = result {
                eprintln!("Could not create the room: {error}");
            }
        })
        .unwrap();
}

fn join_room(conn: &DbConnection, room_id: u64) {
    conn.reducers().join_room(room_id).unwrap();
}

fn leave_room(conn: &DbConnection, room_id: u64) {
    conn.reducers().leave_room(room_id).unwrap();
}

fn send_to_room(conn: &DbConnection, room_id: u64, text: String) {
    conn.reducers()
        .send_message_then(room_id, text, |_ctx, result| {
            if let Ok(Err(error)) = result {
                eprintln!("Could not send the message: {error}");
            }
        })
        .unwrap();
}

fn send_to_player(conn: &DbConnection, recipient: Identity, text: String) {
    conn.reducers()
        .send_direct_message_then(recipient, text, |_ctx, result| {
            if let Ok(Err(error)) = result {
                eprintln!("Could not send the message: {error}");
            }
        })
        .unwrap();
}
```

</TabItem>
</Tabs>

`onInsert` and `onDelete` run once for each row that changes, so joining a room can show it many times in a row. If rendering is expensive, render at most once per frame instead.

## What's next?

You now have chat rooms and DMs that stay live on every client:

- Private tables hold the room members, the messages and the DMs.
- Reducers check who can send what, and set the sender and time of each message themselves.
- Views built with the query builder give each player the messages of their own rooms, and their own DMs, and update incrementally.
- Clients sort the messages, and merge the DMs of each conversation.

To adapt it to your app:

- **Long histories.** `my_messages` returns every message of every room the player is in, and the DM views every DM they sent or received. To keep that bounded, delete old messages with a [scheduled reducer](../../00200-core-concepts/00300-tables/00500-schedule-tables.md).
- **Invite-only rooms.** Add a column to `chat_room` that marks the room as private, and a reducer that lets members add other players. Make `join_room` reject private rooms. To keep private rooms out of the public list, make `chat_room` private, and add a view that returns only the public rooms.
- **Group DMs.** A group DM is an invite-only room that isn't listed.
- **Blocking players.** Add a private table of blocked players that `send_direct_message` checks before it sends a DM.
- **Moderation.** Room names are public, and every client downloads them. To check room names and messages for offensive words, use the same approach as [Keep usernames unique and block offensive ones](./00100-unique-usernames.md).
- **Spam.** To stop a player from creating many rooms or sending many messages in a row, store when they last did it, and reject requests that come too soon. The usernames guide uses the same kind of cooldown for name requests.

To learn more about the features used here, see [Views](../../00200-core-concepts/00200-functions/00500-views.md), [Indexes](../../00200-core-concepts/00300-tables/00300-indexes.md) and [Access Permissions](../../00200-core-concepts/00300-tables/00400-access-permissions.md).

Next, you can show who's typing with [typing indicators and other short-lived signals](./00400-typing-indicators.md).

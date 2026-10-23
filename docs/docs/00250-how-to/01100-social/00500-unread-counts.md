---
title: Unread counts and notifications
slug: /how-to/social/unread-counts
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';
import { CppModuleVersionNotice } from "@site/src/components/CppModuleVersionNotice";

:::note Prerequisites
This guide builds on [Chat rooms and private DMs](./00300-chat-rooms-and-dms.md): it uses its tables, its `find_membership` function, its views and its client. If you haven't built chat rooms yet, follow that guide first.
:::

In this guide, you'll show how many messages each room has that the player hasn't read, such as "general (3)", and let them know when a new message arrives in another room. The database remembers what each player has read, so reading a room on one device clears its count on the others.

## How it works

- A private `read_marker` table has one row for each player in each room they've read. It holds the `id` of the last message they read there. Each message's `id` is larger than the ids of the messages sent before it, so this one number tells which messages are unread: the ones with a larger `id`.
- A `mark_read` reducer moves the caller's marker forward. It never moves it back, so a late or repeated request can't make messages unread again.
- A `my_read_markers` view returns the caller's markers.
- The client already has the messages of the player's rooms, from `my_messages`. A room's unread count is the number of its messages that have an `id` larger than the marker, and that other players sent.
- When the player opens a room, and when a message arrives in the room they have open, the client marks the room as read.
- When a message arrives in another room, the client shows a notice, such as "New message from Alice in general".

## Define the table and the view

Add the `read_marker` table. It's private, so players can't read what others have read. It has an index on `member`, so the reducer and the view can find a player's markers quickly.

Then add the `my_read_markers` view, which returns the caller's markers.

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
const readMarker = table(
  { name: 'read_marker' },
  {
    id: t.u64().primaryKey().autoInc(),
    roomId: t.u64(),
    member: t.identity().index('btree'),
    lastReadId: t.u64(),
  }
);

const spacetimedb = schema({ player, chatRoom, roomMember, message, directMessage, readMarker });
export default spacetimedb;

// The caller's read markers.
export const myReadMarkers = spacetimedb.view(
  { name: 'my_read_markers', public: true },
  t.array(readMarker.rowType),
  ctx => ctx.from.readMarker.where(m => m.member.eq(ctx.sender))
);
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
public static partial class Module
{
    [SpacetimeDB.Table(Accessor = "ReadMarker")]
    public partial struct ReadMarker
    {
        [SpacetimeDB.PrimaryKey]
        [SpacetimeDB.AutoInc]
        public ulong Id;
        public ulong RoomId;
        [SpacetimeDB.Index.BTree]
        public Identity Member;
        public ulong LastReadId;
    }

    // The caller's read markers.
    [SpacetimeDB.View(Accessor = "MyReadMarkers", Public = true)]
    public static IQuery<ReadMarker> MyReadMarkers(ViewContext ctx)
    {
        var me = ctx.Sender;
        return ctx.From.ReadMarker().Where(m => m.Member.Eq(me));
    }
}
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
#[table(accessor = read_marker)]
pub struct ReadMarker {
    #[primary_key]
    #[auto_inc]
    id: u64,
    room_id: u64,
    #[index(btree)]
    member: Identity,
    last_read_id: u64,
}

/// The caller's read markers.
#[view(accessor = my_read_markers, public)]
fn my_read_markers(ctx: &ViewContext) -> impl Query<ReadMarker> {
    let me = ctx.sender();
    ctx.from.read_marker().r#where(move |m| m.member.eq(me))
}
```

</TabItem>
<TabItem value="cpp" label="C++">

<CppModuleVersionNotice />

```cpp
struct ReadMarker {
    uint64_t id;
    uint64_t room_id;
    Identity member;
    uint64_t last_read_id;
};
SPACETIMEDB_STRUCT(ReadMarker, id, room_id, member, last_read_id)
SPACETIMEDB_TABLE(ReadMarker, read_marker, Private)
FIELD_PrimaryKeyAutoInc(read_marker, id)
FIELD_Index(read_marker, member)

// The caller's read markers.
SPACETIMEDB_VIEW(Query<ReadMarker>, my_read_markers, Public, ViewContext ctx) {
    auto me = ctx.sender();
    return ctx.from[read_marker].where([me](const auto& m) { return m.member.eq(me); });
}
```

</TabItem>
</Tabs>

A new table is always a safe change to a published module: the existing tables and their rows stay as they are.

## Mark messages as read

Add a `find_read_marker` function that returns a player's marker in a room, if there is one. It works like `find_membership`: it reads the player's markers through the `member` index, and looks for the room among them.

Then add a `mark_read` reducer. It takes a room and the `id` of the last message the player read in it, and:

- Rejects players who aren't in the room.
- Rejects messages that aren't in the room. Without this check, a client could set its marker to an `id` that no message has yet, and the next messages would show as read.
- Creates the player's marker if they don't have one yet, or moves it forward. If the message is older than the marker, it changes nothing.

Finally, replace `leave_room` with a version that also deletes the player's marker, so that no marker stays behind for a room they left.

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
// Returns the read marker of `member` in the room, if there is one.
function findReadMarker(ctx: Ctx, roomId: bigint, member: Identity) {
  for (const row of ctx.db.readMarker.member.filter(member)) {
    if (row.roomId === roomId) {
      return row;
    }
  }
  return undefined;
}

export const markRead = spacetimedb.reducer(
  { roomId: t.u64(), lastReadId: t.u64() },
  (ctx, { roomId, lastReadId }) => {
    if (!findMembership(ctx, roomId, ctx.sender)) {
      throw new SenderError("You're not in this room");
    }
    const msg = ctx.db.message.id.find(lastReadId);
    if (!msg || msg.roomId !== roomId) {
      throw new SenderError('Message not found');
    }
    const marker = findReadMarker(ctx, roomId, ctx.sender);
    if (!marker) {
      ctx.db.readMarker.insert({ id: 0n, roomId, member: ctx.sender, lastReadId });
    } else if (lastReadId > marker.lastReadId) {
      ctx.db.readMarker.id.update({ ...marker, lastReadId });
    }
  }
);

export const leaveRoom = spacetimedb.reducer({ roomId: t.u64() }, (ctx, { roomId }) => {
  const row = findMembership(ctx, roomId, ctx.sender);
  if (row) {
    ctx.db.roomMember.id.delete(row.id);
  }
  const marker = findReadMarker(ctx, roomId, ctx.sender);
  if (marker) {
    ctx.db.readMarker.id.delete(marker.id);
  }
});
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
public static partial class Module
{
    // Returns the read marker of `member` in the room, if there is one.
    static ReadMarker? FindReadMarker(ReducerContext ctx, ulong roomId, Identity member)
    {
        foreach (var row in ctx.Db.ReadMarker.Member.Filter(member))
        {
            if (row.RoomId == roomId)
            {
                return row;
            }
        }
        return null;
    }

    [SpacetimeDB.Reducer]
    public static void MarkRead(ReducerContext ctx, ulong roomId, ulong lastReadId)
    {
        if (FindMembership(ctx, roomId, ctx.Sender) is null)
        {
            throw new Exception("You're not in this room");
        }
        if (ctx.Db.Message.Id.Find(lastReadId) is not Message msg || msg.RoomId != roomId)
        {
            throw new Exception("Message not found");
        }
        if (FindReadMarker(ctx, roomId, ctx.Sender) is not ReadMarker marker)
        {
            ctx.Db.ReadMarker.Insert(new ReadMarker { RoomId = roomId, Member = ctx.Sender, LastReadId = lastReadId });
        }
        else if (lastReadId > marker.LastReadId)
        {
            marker.LastReadId = lastReadId;
            ctx.Db.ReadMarker.Id.Update(marker);
        }
    }

    [SpacetimeDB.Reducer]
    public static void LeaveRoom(ReducerContext ctx, ulong roomId)
    {
        if (FindMembership(ctx, roomId, ctx.Sender) is RoomMember row)
        {
            ctx.Db.RoomMember.Id.Delete(row.Id);
        }
        if (FindReadMarker(ctx, roomId, ctx.Sender) is ReadMarker marker)
        {
            ctx.Db.ReadMarker.Id.Delete(marker.Id);
        }
    }
}
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
/// Returns the read marker of `member` in the room, if there is one.
fn find_read_marker(ctx: &ReducerContext, room_id: u64, member: Identity) -> Option<ReadMarker> {
    ctx.db
        .read_marker()
        .member()
        .filter(member)
        .find(|row| row.room_id == room_id)
}

#[reducer]
pub fn mark_read(ctx: &ReducerContext, room_id: u64, last_read_id: u64) -> Result<(), String> {
    if find_membership(ctx, room_id, ctx.sender()).is_none() {
        return Err("You're not in this room".to_string());
    }
    match ctx.db.message().id().find(last_read_id) {
        Some(msg) if msg.room_id == room_id => {}
        _ => return Err("Message not found".to_string()),
    }
    match find_read_marker(ctx, room_id, ctx.sender()) {
        None => {
            ctx.db.read_marker().insert(ReadMarker {
                id: 0,
                room_id,
                member: ctx.sender(),
                last_read_id,
            });
        }
        Some(marker) if last_read_id > marker.last_read_id => {
            ctx.db.read_marker().id().update(ReadMarker { last_read_id, ..marker });
        }
        Some(_) => {} // Already read.
    }
    Ok(())
}

#[reducer]
pub fn leave_room(ctx: &ReducerContext, room_id: u64) {
    if let Some(row) = find_membership(ctx, room_id, ctx.sender()) {
        ctx.db.room_member().id().delete(row.id);
    }
    if let Some(marker) = find_read_marker(ctx, room_id, ctx.sender()) {
        ctx.db.read_marker().id().delete(marker.id);
    }
}
```

</TabItem>
<TabItem value="cpp" label="C++">

```cpp
// Returns the read marker of `member` in the room, if there is one.
std::optional<ReadMarker> find_read_marker(ReducerContext& ctx, uint64_t room_id, Identity member) {
    for (const auto& row : ctx.db[read_marker_member].filter(member)) {
        if (row.room_id == room_id) {
            return row;
        }
    }
    return std::nullopt;
}

SPACETIMEDB_REDUCER(mark_read, ReducerContext ctx, uint64_t room_id, uint64_t last_read_id) {
    if (!find_membership(ctx, room_id, ctx.sender())) {
        return Err("You're not in this room");
    }
    auto msg = ctx.db[message_id].find(last_read_id);
    if (!msg || msg->room_id != room_id) {
        return Err("Message not found");
    }
    auto marker = find_read_marker(ctx, room_id, ctx.sender());
    if (!marker) {
        ctx.db[read_marker].insert(ReadMarker{0, room_id, ctx.sender(), last_read_id});
    } else if (last_read_id > marker->last_read_id) {
        marker->last_read_id = last_read_id;
        ctx.db[read_marker_id].update(*marker);
    }
    return Ok();
}

SPACETIMEDB_REDUCER(leave_room, ReducerContext ctx, uint64_t room_id) {
    if (auto row = find_membership(ctx, room_id, ctx.sender())) {
        ctx.db[room_member_id].delete_by_key(row->id);
    }
    if (auto marker = find_read_marker(ctx, room_id, ctx.sender())) {
        ctx.db[read_marker_id].delete_by_key(marker->id);
    }
    return Ok();
}
```

</TabItem>
</Tabs>

A player who joins a room has no marker there yet, so the room's earlier messages all count as unread. The client marks them as read as soon as the player opens the room.

## Show unread counts

On the client, add `my_read_markers` to the subscription of the chat rooms client: `tables.myReadMarkers` in TypeScript, `q => q.From.MyReadMarkers()` in C#, and `|q| q.from.my_read_markers()` in Rust. Then add the functions below:

- `last_read_id` returns the player's marker in a room, or 0 if they don't have one.
- `unread_count` counts the messages of a room that have a larger `id` than the marker, leaving out the player's own messages.
- `mark_room_read` calls `mark_read` with the newest message of a room. It doesn't call it if there's nothing new, so opening a room that's already read sends nothing.
- `show_unread_counts` shows each of the player's rooms, with its count if it has unread messages.

Show the counts again whenever a marker changes, or the player joins or leaves a room.

Then change `show_room` so that it also marks the room as read. The TypeScript and C# clients also keep track of the room on screen, in `openRoomId`, so `show_conversation` changes too. The Rust client already knows which room is on screen.

<Tabs groupId="client-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
// The room on screen, if any.
let openRoomId: bigint | undefined;

// The id of the last message the player read in the room, or 0 if they haven't read any.
function lastReadId(roomId: bigint) {
  for (const marker of conn.db.myReadMarkers.iter()) {
    if (marker.roomId === roomId) {
      return marker.lastReadId;
    }
  }
  return 0n;
}

// The number of messages that other players sent in the room after the player's last read one.
function unreadCount(roomId: bigint) {
  const lastRead = lastReadId(roomId);
  let count = 0;
  for (const m of conn.db.myMessages.iter()) {
    if (m.roomId === roomId && m.id > lastRead && !m.sender.isEqual(conn.identity!)) {
      count++;
    }
  }
  return count;
}

// Marks every message of the room as read, if there are new ones.
function markRoomRead(roomId: bigint) {
  let newest = 0n;
  for (const m of conn.db.myMessages.iter()) {
    if (m.roomId === roomId && m.id > newest) {
      newest = m.id;
    }
  }
  if (newest > lastReadId(roomId)) {
    conn.reducers.markRead({ roomId, lastReadId: newest }).catch(console.error);
  }
}

function showUnreadCounts() {
  for (const room of conn.db.myRooms.iter()) {
    const count = unreadCount(room.id);
    console.log(count > 0 ? `${room.name} (${count})` : room.name);
  }
}

conn.db.myReadMarkers.onInsert(() => showUnreadCounts());
conn.db.myReadMarkers.onUpdate(() => showUnreadCounts());
conn.db.myRooms.onInsert(() => showUnreadCounts());
conn.db.myRooms.onDelete(() => showUnreadCounts());

function showRoom(roomId: bigint) {
  openRoomId = roomId;
  markRoomRead(roomId);
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
  openRoomId = undefined;
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
```

</TabItem>
<TabItem value="csharp" label="C#">

Add this code after the callbacks of the chat rooms client, and replace its `ShowRoom` and `ShowConversation` functions with the ones below.

```csharp
// The room on screen, if any.
ulong? openRoomId = null;

// The id of the last message the player read in the room, or 0 if they haven't read any.
ulong LastReadId(ulong roomId)
{
    foreach (var marker in conn.Db.MyReadMarkers.Iter())
    {
        if (marker.RoomId == roomId)
        {
            return marker.LastReadId;
        }
    }
    return 0;
}

// The number of messages that other players sent in the room after the player's last read one.
int UnreadCount(ulong roomId)
{
    var lastRead = LastReadId(roomId);
    return conn.Db.MyMessages.Iter().Count(m => m.RoomId == roomId && m.Id > lastRead && m.Sender != conn.Identity);
}

// Marks every message of the room as read, if there are new ones.
void MarkRoomRead(ulong roomId)
{
    ulong newest = 0;
    foreach (var m in conn.Db.MyMessages.Iter())
    {
        if (m.RoomId == roomId && m.Id > newest)
        {
            newest = m.Id;
        }
    }
    if (newest > LastReadId(roomId))
    {
        conn.Reducers.MarkRead(roomId, newest);
    }
}

void ShowUnreadCounts()
{
    foreach (var room in conn.Db.MyRooms.Iter())
    {
        var count = UnreadCount(room.Id);
        Console.WriteLine(count > 0 ? $"{room.Name} ({count})" : room.Name);
    }
}

conn.Db.MyReadMarkers.OnInsert += (ctx, marker) => ShowUnreadCounts();
conn.Db.MyReadMarkers.OnUpdate += (ctx, oldMarker, newMarker) => ShowUnreadCounts();
conn.Db.MyRooms.OnInsert += (ctx, room) => ShowUnreadCounts();
conn.Db.MyRooms.OnDelete += (ctx, room) => ShowUnreadCounts();

void ShowRoom(ulong roomId)
{
    openRoomId = roomId;
    MarkRoomRead(roomId);
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
    openRoomId = null;
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
```

</TabItem>
<TabItem value="rust" label="Rust">

Call `register_unread_callbacks(&conn)` in `main`, next to the other callbacks, and replace `show_room` with the version below.

```rust
use spacetimedb_sdk::{Event, TableWithPrimaryKey};

/// The room on screen, if any.
fn open_room() -> Option<u64> {
    match *SCREEN.lock().unwrap() {
        Some(Screen::Room(room_id)) => Some(room_id),
        _ => None,
    }
}

/// The id of the last message the player read in the room, or 0 if they haven't read any.
fn last_read_id(db: &RemoteTables, room_id: u64) -> u64 {
    db.my_read_markers()
        .iter()
        .find(|marker| marker.room_id == room_id)
        .map_or(0, |marker| marker.last_read_id)
}

/// The number of messages that other players sent in the room after the player's last read one.
fn unread_count(ctx: &impl RemoteDbContext, room_id: u64) -> usize {
    let last_read = last_read_id(ctx.db(), room_id);
    let me = ctx.identity();
    ctx.db()
        .my_messages()
        .iter()
        .filter(|m| m.room_id == room_id && m.id > last_read && m.sender != me)
        .count()
}

/// Marks every message of the room as read, if there are new ones.
fn mark_room_read(ctx: &impl RemoteDbContext, room_id: u64) {
    let newest = ctx.db().my_messages().iter().filter(|m| m.room_id == room_id).map(|m| m.id).max();
    if let Some(newest) = newest {
        if newest > last_read_id(ctx.db(), room_id) {
            ctx.reducers().mark_read(room_id, newest).unwrap();
        }
    }
}

fn show_unread_counts(ctx: &impl RemoteDbContext) {
    for room in ctx.db().my_rooms().iter() {
        match unread_count(ctx, room.id) {
            0 => println!("{}", room.name),
            count => println!("{} ({count})", room.name),
        }
    }
}

fn register_unread_callbacks(conn: &DbConnection) {
    conn.db().my_read_markers().on_insert(|ctx, _| show_unread_counts(ctx));
    conn.db().my_read_markers().on_update(|ctx, _, _| show_unread_counts(ctx));
    conn.db().my_rooms().on_insert(|ctx, _| show_unread_counts(ctx));
    conn.db().my_rooms().on_delete(|ctx, _| show_unread_counts(ctx));
    on_new_message(conn);
}

fn show_room(conn: &DbConnection, room_id: u64) {
    *SCREEN.lock().unwrap() = Some(Screen::Room(room_id));
    mark_room_read(conn, room_id);
    render(conn.db());
}
```

</TabItem>
</Tabs>

## Notify about new messages

When a message arrives in the room on screen, mark the room as read, so the player's other devices clear it too. Don't show the counts yet: the room would show 1 unread message until its marker changes, and the marker's update shows them. When a message arrives in another room, show a notice, then the counts.

Only notify about messages that other players just sent. Inserts into `my_messages` also happen when the client subscribes, and when the player joins a room and gets its earlier messages. The event of each insert tells them apart:

- `SubscribeApplied`: the client just subscribed, and is getting the rows that already exist.
- `Reducer`: a reducer this client called made the change, such as `join_room` or `send_message`.
- `Transaction`: another client's reducer made the change. That's the one to notify about.

<Tabs groupId="client-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
conn.db.myMessages.onInsert((ctx, message) => {
  if (message.roomId === openRoomId) {
    markRoomRead(message.roomId);
    return;
  }
  if (ctx.event.tag === 'Transaction') {
    const room = conn.db.chatRoom.id.find(message.roomId);
    console.log(`New message from ${nameOf(message.sender)} in ${room?.name}`);
  }
  showUnreadCounts();
});
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
conn.Db.MyMessages.OnInsert += (ctx, message) =>
{
    if (message.RoomId == openRoomId)
    {
        MarkRoomRead(message.RoomId);
        return;
    }
    if (ctx.Event is Event<Reducer>.Transaction)
    {
        var room = conn.Db.ChatRoom.Id.Find(message.RoomId);
        Console.WriteLine($"New message from {NameOf(message.Sender)} in {room?.Name}");
    }
    ShowUnreadCounts();
};
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
fn on_new_message(conn: &DbConnection) {
    conn.db().my_messages().on_insert(|ctx, message| {
        if open_room() == Some(message.room_id) {
            mark_room_read(ctx, message.room_id);
            return;
        }
        if matches!(ctx.event, Event::Transaction) {
            let room = ctx.db.chat_room().id().find(&message.room_id);
            let room_name = room.map_or(String::new(), |room| room.name);
            println!("New message from {} in {room_name}", name_of(&ctx.db, message.sender));
        }
        show_unread_counts(ctx);
    });
}
```

</TabItem>
</Tabs>

The counts show again for each message that arrives, so subscribing or joining a room with a long history shows them many times in a row. As in the chat rooms guide, if rendering is expensive, render at most once per frame instead.

## What's next?

You now show unread counts that stay in sync across the player's devices, and notify them about new messages:

- A private table holds the `id` of the last message each player read in each room, and a view gives each player their own.
- A reducer only moves a marker forward, and only to a message of the room.
- Clients count the unread messages themselves, from the messages they already have.
- Clients tell new messages from other players apart from the ones they get when they subscribe or join a room, with the event of each insert.

To adapt it to your app:

- **DMs.** Add a marker for each conversation, with the other player's identity instead of a room, and count the DMs you received after it.
- **Big rooms.** The client counts unread messages from the messages it has, so it needs every message of every room the player is in. If you only send clients the latest messages, for example with [pagination](../01200-common-app-patterns/00100-pagination.md), keep an unread count in each player's marker instead: `send_message` adds one to the count of every other member of the room, and `mark_read` sets it back to zero.
- **Mentions.** To notify players only when someone mentions them, check the text of each message for their name before showing the notice.
- **Players who aren't online.** Clients only see new messages while they're connected. To reach players who aren't, send them an email from your module, as in [Send an email from your module](../01200-common-app-patterns/00400-send-email.md).

To learn more about the features used here, see [Views](../../00200-core-concepts/00200-functions/00500-views.md) and [Indexes](../../00200-core-concepts/00300-tables/00300-indexes.md).

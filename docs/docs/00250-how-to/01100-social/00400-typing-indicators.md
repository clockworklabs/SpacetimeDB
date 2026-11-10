---
title: Typing indicators and other short-lived signals
slug: /how-to/social/typing-indicators
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';
import { CppModuleVersionNotice } from "@site/src/components/CppModuleVersionNotice";

:::note Prerequisites
You need a chat app. This guide builds on the one from the [chat app tutorial](../../00100-intro/00300-tutorials/00100-chat-app.md): it uses its `user` and `message` tables, and its `send_message` reducer. If you don't have a chat app yet, follow the tutorial for your language first.
:::

In this guide, you'll show who's typing in the chat, such as "Alice is typing…". Typing signals only matter for a few seconds, so you'll send them through an [event table](../../00200-core-concepts/00300-tables/00550-event-tables.md): its rows go to the clients, then the database deletes them.

## How it works

- A public `typing_event` event table has one column: the `sender`.
- A `start_typing` reducer inserts a row, with the caller as the sender. When the reducer's transaction commits, SpacetimeDB sends the row to every subscribed client, then deletes it.
- While the user types, the client calls `start_typing`, at most once every 2 seconds.
- Each client keeps a list of who is typing. A user stays in it for 3 seconds after their last signal, or until they send a message. The client shows "Alice is typing…" from that list.

You could store who's typing in a regular table instead, but then you'd have to delete each row once the user stops typing. Typing signals don't need to be stored: a client that connects later doesn't need to know who was typing a second ago. An event table is always empty between transactions, so there's nothing to clean up.

## Define the event table

Add the `typing_event` table, with the `event` flag. It's public, so every client can subscribe to it.

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
const typingEvent = table(
  { name: 'typing_event', public: true, event: true },
  {
    sender: t.identity(),
  }
);

const spacetimedb = schema({ user, message, typingEvent });
export default spacetimedb;
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
public static partial class Module
{
    [SpacetimeDB.Table(Accessor = "TypingEvent", Public = true, Event = true)]
    public partial struct TypingEvent
    {
        public Identity Sender;
    }
}
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
#[table(accessor = typing_event, public, event)]
pub struct TypingEvent {
    sender: Identity,
}
```

</TabItem>
<TabItem value="cpp" label="C++">

<CppModuleVersionNotice />

```cpp
struct TypingEvent {
    Identity sender;
};
SPACETIMEDB_STRUCT(TypingEvent, sender)
SPACETIMEDB_TABLE(TypingEvent, typing_event, Public, true)
```

</TabItem>
</Tabs>

## Write the reducer

Add a `start_typing` reducer that inserts a row into `typing_event`. It sets the sender to `ctx.sender`, so a client can't make it look like someone else is typing.

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
export const startTyping = spacetimedb.reducer(ctx => {
  ctx.db.typingEvent.insert({ sender: ctx.sender });
});
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
public static partial class Module
{
    [SpacetimeDB.Reducer]
    public static void StartTyping(ReducerContext ctx)
    {
        ctx.Db.TypingEvent.Insert(new TypingEvent { Sender = ctx.Sender });
    }
}
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
#[reducer]
pub fn start_typing(ctx: &ReducerContext) {
    ctx.db.typing_event().insert(TypingEvent { sender: ctx.sender() });
}
```

</TabItem>
<TabItem value="cpp" label="C++">

```cpp
SPACETIMEDB_REDUCER(start_typing, ReducerContext ctx) {
    ctx.db[typing_event].insert(TypingEvent{ctx.sender()});
    return Ok();
}
```

</TabItem>
</Tabs>

## Show who's typing

On the client, subscribe to `typing_event`, along with `user` and `message`. The client cache never keeps the rows of an event table, so you can't read them back later: handle each signal in an `onInsert` callback, when it arrives.

Keep a list of who is typing, and when each of them stops showing as typing:

- When a signal arrives, set its sender to stop showing 3 seconds from now. Skip your own signals, so you don't see yourself typing.
- When a message arrives, remove its sender: they're done typing.
- A few times per second, remove the users whose 3 seconds are up.

Each client uses its own clock for these deadlines, so it doesn't matter if the clocks of the clients and the database disagree. The timeout is longer than the interval between signals, so a user who keeps typing doesn't flicker on and off when a signal arrives a little late.

<Tabs groupId="client-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
import type { Identity } from 'spacetimedb';
import { DbConnection, tables } from './module_bindings';

// How long a user shows as typing after their last signal.
const TYPING_TIMEOUT_MS = 3000;

// The users who are typing, by identity, with when they stop showing as typing.
const typingUntil = new Map<string, { identity: Identity; until: number }>();

const conn = DbConnection.builder()
  .withUri('ws://localhost:3000')
  .withDatabaseName('quickstart-chat')
  .onConnect(ctx => {
    ctx.subscriptionBuilder().subscribe([tables.user, tables.message, tables.typingEvent]);
  })
  .build();

conn.db.typingEvent.onInsert((_ctx, typing) => {
  if (typing.sender.isEqual(conn.identity!)) {
    return; // Don't show the user that they're typing.
  }
  typingUntil.set(typing.sender.toHexString(), {
    identity: typing.sender,
    until: Date.now() + TYPING_TIMEOUT_MS,
  });
  showTyping();
});

// A user who sends a message is done typing.
conn.db.message.onInsert((_ctx, message) => {
  if (typingUntil.delete(message.sender.toHexString())) {
    showTyping();
  }
});

// Remove the users whose last signal is too old.
setInterval(() => {
  const now = Date.now();
  let changed = false;
  for (const [key, entry] of typingUntil) {
    if (entry.until <= now) {
      typingUntil.delete(key);
      changed = true;
    }
  }
  if (changed) {
    showTyping();
  }
}, 250);

function nameOf(identity: Identity) {
  return conn.db.user.identity.find(identity)?.name ?? identity.toHexString().substring(0, 8);
}

// The line to show under the messages, such as "Alice is typing…".
function typingLine() {
  const names = Array.from(typingUntil.values(), entry => nameOf(entry.identity));
  if (names.length === 0) {
    return '';
  }
  if (names.length === 1) {
    return `${names[0]} is typing…`;
  }
  if (names.length === 2) {
    return `${names[0]} and ${names[1]} are typing…`;
  }
  return 'Several people are typing…';
}

function showTyping() {
  console.log(typingLine());
}
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
using System;
using System.Collections.Generic;
using System.Linq;
using SpacetimeDB;
using SpacetimeDB.Types;

// How long a user shows as typing after their last signal.
var typingTimeout = TimeSpan.FromSeconds(3);

// The users who are typing, with when they stop showing as typing.
var typingUntil = new Dictionary<Identity, DateTime>();

var conn = DbConnection.Builder()
    .WithUri("http://localhost:3000")
    .WithDatabaseName("quickstart-chat")
    .OnConnect((conn, identity, token) =>
    {
        conn.SubscriptionBuilder()
            .AddQuery(q => q.From.User())
            .AddQuery(q => q.From.Message())
            .AddQuery(q => q.From.TypingEvent())
            .Subscribe();
    })
    .Build();

conn.Db.TypingEvent.OnInsert += (ctx, typing) =>
{
    if (typing.Sender == conn.Identity)
    {
        return; // Don't show the user that they're typing.
    }
    typingUntil[typing.Sender] = DateTime.UtcNow + typingTimeout;
    ShowTyping();
};

// A user who sends a message is done typing.
conn.Db.Message.OnInsert += (ctx, message) =>
{
    if (typingUntil.Remove(message.Sender))
    {
        ShowTyping();
    }
};

// Removes the users whose last signal is too old.
void RemoveExpired()
{
    var now = DateTime.UtcNow;
    var expired = typingUntil.Where(entry => entry.Value <= now).Select(entry => entry.Key).ToList();
    foreach (var identity in expired)
    {
        typingUntil.Remove(identity);
    }
    if (expired.Count > 0)
    {
        ShowTyping();
    }
}

string NameOf(Identity identity) => conn.Db.User.Identity.Find(identity)?.Name ?? identity.ToString()[..8];

// The line to show under the messages, such as "Alice is typing…".
string TypingLine()
{
    var names = typingUntil.Keys.Select(NameOf).ToList();
    return names.Count switch
    {
        0 => "",
        1 => $"{names[0]} is typing…",
        2 => $"{names[0]} and {names[1]} are typing…",
        _ => "Several people are typing…",
    };
}

void ShowTyping() => Console.WriteLine(TypingLine());
```

Remember to call `conn.FrameTick()` regularly, for example once per frame, so the callbacks run. Call `RemoveExpired()` right after it.

</TabItem>
<TabItem value="rust" label="Rust">

```rust
mod module_bindings;
use module_bindings::*;
use spacetimedb_sdk::{DbContext, EventTable, Identity, Table};
use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

/// How long a user shows as typing after their last signal.
const TYPING_TIMEOUT: Duration = Duration::from_secs(3);

/// The users who are typing, with when they stop showing as typing.
static TYPING_UNTIL: LazyLock<Mutex<HashMap<Identity, Instant>>> = LazyLock::new(Default::default);

fn main() {
    let conn = DbConnection::builder()
        .with_uri("http://localhost:3000")
        .with_database_name("quickstart-chat")
        .on_connect(|ctx, _identity, _token| {
            ctx.subscription_builder()
                .add_query(|q| q.from.user())
                .add_query(|q| q.from.message())
                .add_query(|q| q.from.typing_event())
                .subscribe();
        })
        .build()
        .expect("failed to connect");

    conn.db().typing_event().on_insert(|ctx, typing| {
        if typing.sender == ctx.identity() {
            return; // Don't show the user that they're typing.
        }
        TYPING_UNTIL
            .lock()
            .unwrap()
            .insert(typing.sender, Instant::now() + TYPING_TIMEOUT);
        show_typing(&ctx.db);
    });

    // A user who sends a message is done typing.
    conn.db().message().on_insert(|ctx, message| {
        let removed = TYPING_UNTIL.lock().unwrap().remove(&message.sender).is_some();
        if removed {
            show_typing(&ctx.db);
        }
    });

    // Process messages from the database on a background thread.
    conn.run_threaded();

    // Your app's main loop runs here, and calls `remove_expired` regularly, for example once per frame.
}

/// Removes the users whose last signal is too old.
fn remove_expired(conn: &DbConnection) {
    let now = Instant::now();
    let changed = {
        let mut typing_until = TYPING_UNTIL.lock().unwrap();
        let before = typing_until.len();
        typing_until.retain(|_, until| *until > now);
        typing_until.len() != before
    };
    if changed {
        show_typing(conn.db());
    }
}

fn name_of(db: &RemoteTables, identity: Identity) -> String {
    match db.user().identity().find(&identity).and_then(|user| user.name) {
        Some(name) => name,
        None => identity.to_abbreviated_hex().to_string(),
    }
}

/// The line to show under the messages, such as "Alice is typing…".
fn typing_line(db: &RemoteTables) -> String {
    let identities: Vec<Identity> = TYPING_UNTIL.lock().unwrap().keys().copied().collect();
    let names: Vec<String> = identities.into_iter().map(|identity| name_of(db, identity)).collect();
    match names.as_slice() {
        [] => String::new(),
        [name] => format!("{name} is typing…"),
        [first, second] => format!("{first} and {second} are typing…"),
        _ => "Several people are typing…".to_string(),
    }
}

fn show_typing(db: &RemoteTables) {
    println!("{}", typing_line(db));
}
```

</TabItem>
</Tabs>

## Signal that the user is typing

Each time the text in the message box changes, call `start_typing`, but at most once every 2 seconds. Each call sends a row to every subscribed client, so don't send one per keystroke: one signal every 2 seconds is enough to keep the user showing as typing.

When the user sends their message, forget when you last signaled, so the next keystroke signals right away.

<Tabs groupId="client-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
// How often the client signals that the user is typing.
const SIGNAL_INTERVAL_MS = 2000;

// When this client last signaled that the user is typing.
let lastSignal = 0;

// Call this each time the text in the message box changes.
function onMessageBoxChanged(text: string) {
  const now = Date.now();
  if (text.length === 0 || now - lastSignal < SIGNAL_INTERVAL_MS) {
    return;
  }
  lastSignal = now;
  conn.reducers.startTyping({}).catch(console.error);
}

async function sendMessage(text: string) {
  lastSignal = 0;
  try {
    await conn.reducers.sendMessage({ text });
  } catch (error) {
    console.error(`Could not send the message: ${error}`);
  }
}
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
// How often the client signals that the user is typing.
var signalInterval = TimeSpan.FromSeconds(2);

// When this client last signaled that the user is typing.
var lastSignal = DateTime.MinValue;

// Call this each time the text in the message box changes.
void OnMessageBoxChanged(string text)
{
    var now = DateTime.UtcNow;
    if (text.Length == 0 || now - lastSignal < signalInterval)
    {
        return;
    }
    lastSignal = now;
    conn.Reducers.StartTyping();
}

void SendMessage(string text)
{
    lastSignal = DateTime.MinValue;
    conn.Reducers.SendMessage(text);
}
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
/// How often the client signals that the user is typing.
const SIGNAL_INTERVAL: Duration = Duration::from_secs(2);

/// When this client last signaled that the user is typing.
static LAST_SIGNAL: Mutex<Option<Instant>> = Mutex::new(None);

/// Call this each time the text in the message box changes.
fn on_message_box_changed(conn: &DbConnection, text: &str) {
    let mut last_signal = LAST_SIGNAL.lock().unwrap();
    if text.is_empty() || last_signal.is_some_and(|at| at.elapsed() < SIGNAL_INTERVAL) {
        return;
    }
    *last_signal = Some(Instant::now());
    conn.reducers().start_typing().unwrap();
}

fn send_message(conn: &DbConnection, text: String) {
    *LAST_SIGNAL.lock().unwrap() = None;
    conn.reducers()
        .send_message_then(text, |_ctx, result| {
            if let Ok(Err(error)) = result {
                eprintln!("Could not send the message: {error}");
            }
        })
        .unwrap();
}
```

</TabItem>
</Tabs>

Clearing the message box doesn't send anything: the other clients stop showing the user as typing 3 seconds after their last signal.

## What's next?

You now show who's typing in the chat:

- A public event table carries the typing signals. Its rows go to the subscribed clients, and are never stored, so there's nothing to clean up.
- A reducer sets the sender of each signal itself.
- Clients send at most one signal every 2 seconds, and show each user as typing for 3 seconds after their last one, or until they send a message.

To adapt it to your app:

- **Chat rooms.** Add a `room_id` column to `typing_event`, and a `room_id` argument to `start_typing` that it checks the caller is in. Clients then show only the signals of the room that's open. With many rooms, subscribe only to the open room's signals, with a query that filters on `room_id`.
- **Private signals.** Event tables can't be used in views, so every client that subscribes to `typing_event` can receive every signal. That's fine for a chat that everyone can read, but not for a private room or a DM. For those, store who's typing in a private table, give each user the rows they may see through a [view](../../00200-core-concepts/00200-functions/00500-views.md), as in [Chat rooms and private DMs](./00300-chat-rooms-and-dms.md), and delete old rows with a [scheduled reducer](../../00200-core-concepts/00300-tables/00500-schedule-tables.md).
- **Stop typing right away.** To hide the indicator as soon as the user clears the message box, add a `typing` column to `typing_event`, and a `stop_typing` reducer that inserts a row with `typing` set to false. Clients remove the sender when they receive it.
- **Spam.** A client can call `start_typing` much more often than every 2 seconds. To stop that, store when each user last signaled in a private table, and ignore signals that come too soon. The [usernames guide](./00100-unique-usernames.md) uses the same kind of cooldown for name requests.
- **Other short-lived signals.** The same pattern works for anything clients only need to see as it happens: emoji reactions, "Alice joined the chat" notices, pings or sound effects. Give each kind of signal its own event table, with the columns it needs.

To learn more about the features used here, see [Event Tables](../../00200-core-concepts/00300-tables/00550-event-tables.md) and [Reducers](../../00200-core-concepts/00200-functions/00300-reducers/00300-reducers.md).

Next, you can show each user how many messages they haven't read with [unread counts and notifications](./00500-unread-counts.md).

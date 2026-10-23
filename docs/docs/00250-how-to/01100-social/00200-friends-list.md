---
title: Build a friends or follow list
slug: /how-to/social/friends-list
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';
import { CppModuleVersionNotice } from "@site/src/components/CppModuleVersionNotice";

:::note Prerequisites
You need a module and a client connected to it. If you don't have them yet, follow the [quickstart](../../00100-intro/00100-getting-started/00100-getting-started.md) for your language first. This guide uses a minimal `player` table with a name, and doesn't cover how players get their row. To let players pick a unique name and block offensive ones, see [Keep usernames unique and block offensive ones](./00100-unique-usernames.md): the code here works the same with its `player` table.
:::

In this guide, you'll let players follow each other, and show each player, live, who they follow and who follows them. Players who follow each other are mutual follows. Nobody else can see a player's lists. You'll store follows in a private table, and use [views](../../00200-core-concepts/00200-functions/00500-views.md) built with the query builder to give each player their own lists.

## How it works

- A public `player` table holds each player's name. How a player gets their row is up to the rest of your module.
- A private `follow` table has one row for each follow: `follower` follows `followed`. Both columns have an [index](../../00200-core-concepts/00300-tables/00300-indexes.md).
- `follow_player` and `unfollow_player` reducers add and remove the caller's follows. A `remove_follower` reducer lets players remove someone who follows them.
- Two views, `my_following` and `my_followers`, return the `player` rows of the players the caller follows, and of the players who follow the caller.
- Clients subscribe to both views. A player who is in both lists is a mutual follow.

The `follow` table is private, so clients can't read who follows whom. They only see the rows the views return, and each view only returns the caller's own lists.

## Define the tables

Add the `player` and `follow` tables. `follow` has an `id` primary key, so a row can be deleted by its `id`, and an index on each of the other columns: `follower` to list who a player follows, and `followed` to list a player's followers.

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

const follow = table(
  { name: 'follow' },
  {
    id: t.u64().primaryKey().autoInc(),
    follower: t.identity().index('btree'),
    followed: t.identity().index('btree'),
  }
);

const spacetimedb = schema({ player, follow });
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

    [SpacetimeDB.Table(Accessor = "Follow")]
    public partial struct Follow
    {
        [SpacetimeDB.PrimaryKey]
        [SpacetimeDB.AutoInc]
        public ulong Id;
        [SpacetimeDB.Index.BTree]
        public Identity Follower;
        [SpacetimeDB.Index.BTree]
        public Identity Followed;
    }
}
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
use spacetimedb::{reducer, table, view, Identity, Query, ReducerContext, Table, ViewContext};

#[table(accessor = player, public)]
pub struct Player {
    #[primary_key]
    identity: Identity,
    name: String,
}

#[table(accessor = follow)]
pub struct Follow {
    #[primary_key]
    #[auto_inc]
    id: u64,
    #[index(btree)]
    follower: Identity,
    #[index(btree)]
    followed: Identity,
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

struct Follow {
    uint64_t id;
    Identity follower;
    Identity followed;
};
SPACETIMEDB_STRUCT(Follow, id, follower, followed)
SPACETIMEDB_TABLE(Follow, follow, Private)
FIELD_PrimaryKeyAutoInc(follow, id)
FIELD_Index(follow, follower)
FIELD_Index(follow, followed)
```

</TabItem>
</Tabs>

## Follow and unfollow players

Add a `find_follow` function that returns the row recording that one player follows another, if there is one. It reads the follower's follows through the `follower` index, and looks for the followed player among them.

Then add three reducers:

- `follow_player` makes the caller follow `target`. It rejects players who don't have a `player` row yet, following yourself, and following a player who doesn't exist. Following someone twice changes nothing, so a client that sends the request twice doesn't get an error. It also limits each player to 1,000 follows, so that no one can follow every player in your game, and so that `find_follow` never reads more than 1,000 rows.
- `unfollow_player` makes the caller stop following `target`.
- `remove_follower` makes `follower` stop following the caller.

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
const MAX_FOLLOWING = 1000;

// Returns the row that records `follower` following `followed`, if there is one.
function findFollow(ctx: Ctx, follower: Identity, followed: Identity) {
  for (const row of ctx.db.follow.follower.filter(follower)) {
    if (row.followed.isEqual(followed)) {
      return row;
    }
  }
  return undefined;
}

export const followPlayer = spacetimedb.reducer({ target: t.identity() }, (ctx, { target }) => {
  if (!ctx.db.player.identity.find(ctx.sender)) {
    throw new SenderError('Pick a name first');
  }
  if (target.isEqual(ctx.sender)) {
    throw new SenderError("You can't follow yourself");
  }
  if (!ctx.db.player.identity.find(target)) {
    throw new SenderError('Player not found');
  }
  if (findFollow(ctx, ctx.sender, target)) {
    return; // Already following.
  }
  if (Array.from(ctx.db.follow.follower.filter(ctx.sender)).length >= MAX_FOLLOWING) {
    throw new SenderError(`You can't follow more than ${MAX_FOLLOWING} players`);
  }
  ctx.db.follow.insert({ id: 0n, follower: ctx.sender, followed: target });
});

export const unfollowPlayer = spacetimedb.reducer({ target: t.identity() }, (ctx, { target }) => {
  const row = findFollow(ctx, ctx.sender, target);
  if (row) {
    ctx.db.follow.id.delete(row.id);
  }
});

export const removeFollower = spacetimedb.reducer({ follower: t.identity() }, (ctx, { follower }) => {
  const row = findFollow(ctx, follower, ctx.sender);
  if (row) {
    ctx.db.follow.id.delete(row.id);
  }
});
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
public static partial class Module
{
    const int MaxFollowing = 1000;

    // Returns the row that records `follower` following `followed`, if there is one.
    static Follow? FindFollow(ReducerContext ctx, Identity follower, Identity followed)
    {
        foreach (var row in ctx.Db.Follow.Follower.Filter(follower))
        {
            if (row.Followed == followed)
            {
                return row;
            }
        }
        return null;
    }

    [SpacetimeDB.Reducer]
    public static void FollowPlayer(ReducerContext ctx, Identity target)
    {
        if (ctx.Db.Player.Identity.Find(ctx.Sender) is null)
        {
            throw new Exception("Pick a name first");
        }
        if (target == ctx.Sender)
        {
            throw new Exception("You can't follow yourself");
        }
        if (ctx.Db.Player.Identity.Find(target) is null)
        {
            throw new Exception("Player not found");
        }
        if (FindFollow(ctx, ctx.Sender, target) is not null)
        {
            return; // Already following.
        }
        if (ctx.Db.Follow.Follower.Filter(ctx.Sender).Count() >= MaxFollowing)
        {
            throw new Exception($"You can't follow more than {MaxFollowing} players");
        }
        ctx.Db.Follow.Insert(new Follow { Follower = ctx.Sender, Followed = target });
    }

    [SpacetimeDB.Reducer]
    public static void UnfollowPlayer(ReducerContext ctx, Identity target)
    {
        if (FindFollow(ctx, ctx.Sender, target) is Follow row)
        {
            ctx.Db.Follow.Id.Delete(row.Id);
        }
    }

    [SpacetimeDB.Reducer]
    public static void RemoveFollower(ReducerContext ctx, Identity follower)
    {
        if (FindFollow(ctx, follower, ctx.Sender) is Follow row)
        {
            ctx.Db.Follow.Id.Delete(row.Id);
        }
    }
}
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
const MAX_FOLLOWING: usize = 1000;

/// Returns the row that records `follower` following `followed`, if there is one.
fn find_follow(ctx: &ReducerContext, follower: Identity, followed: Identity) -> Option<Follow> {
    ctx.db
        .follow()
        .follower()
        .filter(follower)
        .find(|row| row.followed == followed)
}

#[reducer]
pub fn follow_player(ctx: &ReducerContext, target: Identity) -> Result<(), String> {
    if ctx.db.player().identity().find(ctx.sender()).is_none() {
        return Err("Pick a name first".to_string());
    }
    if target == ctx.sender() {
        return Err("You can't follow yourself".to_string());
    }
    if ctx.db.player().identity().find(target).is_none() {
        return Err("Player not found".to_string());
    }
    if find_follow(ctx, ctx.sender(), target).is_some() {
        return Ok(()); // Already following.
    }
    if ctx.db.follow().follower().filter(ctx.sender()).count() >= MAX_FOLLOWING {
        return Err(format!("You can't follow more than {MAX_FOLLOWING} players"));
    }
    ctx.db.follow().insert(Follow {
        id: 0,
        follower: ctx.sender(),
        followed: target,
    });
    Ok(())
}

#[reducer]
pub fn unfollow_player(ctx: &ReducerContext, target: Identity) {
    if let Some(row) = find_follow(ctx, ctx.sender(), target) {
        ctx.db.follow().id().delete(row.id);
    }
}

#[reducer]
pub fn remove_follower(ctx: &ReducerContext, follower: Identity) {
    if let Some(row) = find_follow(ctx, follower, ctx.sender()) {
        ctx.db.follow().id().delete(row.id);
    }
}
```

</TabItem>
<TabItem value="cpp" label="C++">

```cpp
constexpr size_t MAX_FOLLOWING = 1000;

// Returns the row that records `follower` following `followed`, if there is one.
std::optional<Follow> find_follow(ReducerContext& ctx, Identity follower, Identity followed) {
    for (const auto& row : ctx.db[follow_follower].filter(follower)) {
        if (row.followed == followed) {
            return row;
        }
    }
    return std::nullopt;
}

SPACETIMEDB_REDUCER(follow_player, ReducerContext ctx, Identity target) {
    if (!ctx.db[player_identity].find(ctx.sender())) {
        return Err("Pick a name first");
    }
    if (target == ctx.sender()) {
        return Err("You can't follow yourself");
    }
    if (!ctx.db[player_identity].find(target)) {
        return Err("Player not found");
    }
    if (find_follow(ctx, ctx.sender(), target)) {
        return Ok(); // Already following.
    }
    if (ctx.db[follow_follower].filter(ctx.sender()).collect().size() >= MAX_FOLLOWING) {
        return Err("You can't follow more than " + std::to_string(MAX_FOLLOWING) + " players");
    }
    ctx.db[follow].insert(Follow{0, ctx.sender(), target});
    return Ok();
}

SPACETIMEDB_REDUCER(unfollow_player, ReducerContext ctx, Identity target) {
    if (auto row = find_follow(ctx, ctx.sender(), target)) {
        ctx.db[follow_id].delete_by_key(row->id);
    }
    return Ok();
}

SPACETIMEDB_REDUCER(remove_follower, ReducerContext ctx, Identity follower) {
    if (auto row = find_follow(ctx, follower, ctx.sender())) {
        ctx.db[follow_id].delete_by_key(row->id);
    }
    return Ok();
}
```

</TabItem>
</Tabs>

## Show each player their lists

Add two views. `my_following` returns the players the caller follows, and `my_followers` returns the players who follow the caller. Each view starts from the caller's rows in `follow`, then uses a semijoin to keep the `player` rows that match them.

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
// The players the caller follows.
export const myFollowing = spacetimedb.view(
  { name: 'my_following', public: true },
  t.array(player.rowType),
  ctx =>
    ctx.from.follow
      .where(f => f.follower.eq(ctx.sender))
      .rightSemijoin(ctx.from.player, (f, p) => f.followed.eq(p.identity))
);

// The players who follow the caller.
export const myFollowers = spacetimedb.view(
  { name: 'my_followers', public: true },
  t.array(player.rowType),
  ctx =>
    ctx.from.follow
      .where(f => f.followed.eq(ctx.sender))
      .rightSemijoin(ctx.from.player, (f, p) => f.follower.eq(p.identity))
);
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
public static partial class Module
{
    // The players the caller follows.
    [SpacetimeDB.View(Accessor = "MyFollowing", Public = true)]
    public static IQuery<Player> MyFollowing(ViewContext ctx)
    {
        var me = ctx.Sender;
        return ctx.From.Follow()
            .Where(f => f.Follower.Eq(me))
            .RightSemijoin(ctx.From.Player(), (f, p) => f.Followed.Eq(p.Identity));
    }

    // The players who follow the caller.
    [SpacetimeDB.View(Accessor = "MyFollowers", Public = true)]
    public static IQuery<Player> MyFollowers(ViewContext ctx)
    {
        var me = ctx.Sender;
        return ctx.From.Follow()
            .Where(f => f.Followed.Eq(me))
            .RightSemijoin(ctx.From.Player(), (f, p) => f.Follower.Eq(p.Identity));
    }
}
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
/// The players the caller follows.
#[view(accessor = my_following, public)]
fn my_following(ctx: &ViewContext) -> impl Query<Player> {
    let me = ctx.sender();
    ctx.from
        .follow()
        .r#where(move |f| f.follower.eq(me))
        .right_semijoin(ctx.from.player(), |f, p| f.followed.eq(p.identity))
}

/// The players who follow the caller.
#[view(accessor = my_followers, public)]
fn my_followers(ctx: &ViewContext) -> impl Query<Player> {
    let me = ctx.sender();
    ctx.from
        .follow()
        .r#where(move |f| f.followed.eq(me))
        .right_semijoin(ctx.from.player(), |f, p| f.follower.eq(p.identity))
}
```

</TabItem>
<TabItem value="cpp" label="C++">

```cpp
// The players the caller follows.
SPACETIMEDB_VIEW(Query<Player>, my_following, Public, ViewContext ctx) {
    auto me = ctx.sender();
    return ctx.from[follow]
        .where([me](const auto& f) { return f.follower.eq(me); })
        .right_semijoin(ctx.from[player], [](const auto& f, const auto& p) {
            return f.followed.eq(p.identity);
        });
}

// The players who follow the caller.
SPACETIMEDB_VIEW(Query<Player>, my_followers, Public, ViewContext ctx) {
    auto me = ctx.sender();
    return ctx.from[follow]
        .where([me](const auto& f) { return f.followed.eq(me); })
        .right_semijoin(ctx.from[player], [](const auto& f, const auto& p) {
            return f.follower.eq(p.identity);
        });
}
```

</TabItem>
</Tabs>

These views return a query instead of rows, so SpacetimeDB evaluates them with its query engine. When a follow changes, it works out which rows of each view changed, instead of running the view again. Each view depends on who asks, so SpacetimeDB keeps one per subscribed player, but each one only covers that player's follows.

The views return `player` rows, so they keep the `player` table's primary key. When a player renames, clients receive the change as an update to the existing row.

:::note
A semijoin can only match columns that have their own single-column index. That's why `follow` indexes `follower` and `followed` separately, rather than with one index on both columns.
:::

## Show the lists on the client

On the client, subscribe to `my_following` and `my_followers`. A player who is in both lists is a mutual follow, so the client finds them by comparing the two lists. Re-render the lists when a row is added, removed, or updated.

Call the reducers from your UI, for example from a button on a player's profile.

<Tabs groupId="client-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
import type { Identity } from 'spacetimedb';
import { DbConnection, tables } from './module_bindings';

const conn = DbConnection.builder()
  .withUri('ws://localhost:3000')
  .withDatabaseName('my-game')
  .onConnect(ctx => {
    ctx
      .subscriptionBuilder()
      .onApplied(() => showLists())
      .subscribe([tables.myFollowing, tables.myFollowers]);
  })
  .build();

function showLists() {
  const following = Array.from(conn.db.myFollowing.iter());
  const followers = Array.from(conn.db.myFollowers.iter());
  const mutual = following.filter(p => followers.some(f => f.identity.isEqual(p.identity)));
  console.log('Following:', following.map(p => p.name).join(', '));
  console.log('Followers:', followers.map(p => p.name).join(', '));
  console.log('Mutual:', mutual.map(p => p.name).join(', '));
}

for (const view of [conn.db.myFollowing, conn.db.myFollowers]) {
  view.onInsert(() => showLists());
  view.onDelete(() => showLists());
  view.onUpdate(() => showLists());
}

async function follow(target: Identity) {
  try {
    await conn.reducers.followPlayer({ target });
  } catch (error) {
    console.error(`Could not follow this player: ${error}`);
  }
}

function unfollow(target: Identity) {
  conn.reducers.unfollowPlayer({ target }).catch(console.error);
}

function removeFollower(follower: Identity) {
  conn.reducers.removeFollower({ follower }).catch(console.error);
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

var conn = DbConnection.Builder()
    .WithUri("http://localhost:3000")
    .WithDatabaseName("my-game")
    .OnConnect((conn, identity, token) =>
    {
        conn.SubscriptionBuilder()
            .OnApplied(ctx => ShowLists(ctx.Db))
            .AddQuery(q => q.From.MyFollowing())
            .AddQuery(q => q.From.MyFollowers())
            .Subscribe();
    })
    .Build();

conn.Db.MyFollowing.OnInsert += (ctx, row) => ShowLists(ctx.Db);
conn.Db.MyFollowing.OnDelete += (ctx, row) => ShowLists(ctx.Db);
conn.Db.MyFollowing.OnUpdate += (ctx, oldRow, row) => ShowLists(ctx.Db);
conn.Db.MyFollowers.OnInsert += (ctx, row) => ShowLists(ctx.Db);
conn.Db.MyFollowers.OnDelete += (ctx, row) => ShowLists(ctx.Db);
conn.Db.MyFollowers.OnUpdate += (ctx, oldRow, row) => ShowLists(ctx.Db);

conn.Reducers.OnFollowPlayer += (ctx, target) =>
{
    if (ctx.Event.Status is Status.Failed(var error))
    {
        Console.WriteLine($"Could not follow this player: {error}");
    }
};

void Follow(Identity target) => conn.Reducers.FollowPlayer(target);
void Unfollow(Identity target) => conn.Reducers.UnfollowPlayer(target);
void RemoveFollower(Identity follower) => conn.Reducers.RemoveFollower(follower);

static void ShowLists(RemoteTables db)
{
    var following = db.MyFollowing.Iter().ToList();
    var followers = db.MyFollowers.Iter().ToList();
    var mutual = following.Where(p => followers.Any(f => f.Identity == p.Identity));
    Console.WriteLine($"Following: {string.Join(", ", following.Select(p => p.Name))}");
    Console.WriteLine($"Followers: {string.Join(", ", followers.Select(p => p.Name))}");
    Console.WriteLine($"Mutual: {string.Join(", ", mutual.Select(p => p.Name))}");
}
```

Remember to call `conn.FrameTick()` regularly, for example once per frame, so the callbacks run.

</TabItem>
<TabItem value="rust" label="Rust">

```rust
mod module_bindings;
use module_bindings::*;
use spacetimedb_sdk::{DbContext, Identity, Table, TableWithPrimaryKey};

fn names<'a>(players: impl Iterator<Item = &'a Player>) -> String {
    players.map(|p| p.name.as_str()).collect::<Vec<_>>().join(", ")
}

fn show_lists(db: &RemoteTables) {
    let following: Vec<Player> = db.my_following().iter().collect();
    let followers: Vec<Player> = db.my_followers().iter().collect();
    let mutual = following
        .iter()
        .filter(|p| followers.iter().any(|f| f.identity == p.identity));
    println!("Following: {}", names(following.iter()));
    println!("Followers: {}", names(followers.iter()));
    println!("Mutual: {}", names(mutual));
}

fn main() {
    let conn = DbConnection::builder()
        .with_uri("http://localhost:3000")
        .with_database_name("my-game")
        .on_connect(|ctx, _identity, _token| {
            ctx.subscription_builder()
                .on_applied(|ctx| show_lists(&ctx.db))
                .add_query(|q| q.from.my_following())
                .add_query(|q| q.from.my_followers())
                .subscribe();
        })
        .build()
        .expect("failed to connect");

    conn.db().my_following().on_insert(|ctx, _| show_lists(&ctx.db));
    conn.db().my_following().on_delete(|ctx, _| show_lists(&ctx.db));
    conn.db().my_following().on_update(|ctx, _, _| show_lists(&ctx.db));
    conn.db().my_followers().on_insert(|ctx, _| show_lists(&ctx.db));
    conn.db().my_followers().on_delete(|ctx, _| show_lists(&ctx.db));
    conn.db().my_followers().on_update(|ctx, _, _| show_lists(&ctx.db));

    // Process messages from the database on a background thread.
    conn.run_threaded();

    // Your game loop runs here.
}

fn follow(conn: &DbConnection, target: Identity) {
    conn.reducers()
        .follow_player_then(target, |_ctx, result| {
            if let Ok(Err(error)) = result {
                eprintln!("Could not follow this player: {error}");
            }
        })
        .unwrap();
}

fn unfollow(conn: &DbConnection, target: Identity) {
    conn.reducers().unfollow_player(target).unwrap();
}

fn remove_follower(conn: &DbConnection, follower: Identity) {
    conn.reducers().remove_follower(follower).unwrap();
}
```

</TabItem>
</Tabs>

`onInsert`, `onDelete` and `onUpdate` run once for each row that changes, so a change can re-render the lists more than once. If rendering is expensive, re-render at most once per frame instead.

## Use it as a friends list

Follows are one-way, like on most social networks. To build friends that both players must agree on, like in most games, use the same table and reducers, and treat a follow that hasn't been returned yet as a friend request:

| In a friends list | With follows |
|---|---|
| Send a friend request | `follow_player` |
| Incoming requests | Players in `my_followers` but not in `my_following` |
| Outgoing requests | Players in `my_following` but not in `my_followers` |
| Accept a request | `follow_player`, to follow them back |
| Decline a request | `remove_follower` |
| Friends | Mutual follows |
| Remove a friend | `unfollow_player` and `remove_follower` |

To remove a friend in a single transaction, add a reducer that deletes both rows.

After a decline, nothing stops the other player from sending the request again, as often as they like. To prevent that, add a block list: a private table of blocked players that `follow_player` checks before it adds a follow.

## What's next?

You now have follow lists that stay live on every client:

- A private `follow` table records who follows whom, with an index on each side.
- Reducers add and remove follows, and limit how many players each player can follow.
- Two views built with the query builder show each player their own lists, and update incrementally.
- Clients find mutual follows by comparing the two lists, which also gives you a friends list with requests.

From here, you could add a block list, as described above, or show which of a player's follows are online.

To learn more about the features used here, see [Views](../../00200-core-concepts/00200-functions/00500-views.md), [Indexes](../../00200-core-concepts/00300-tables/00300-indexes.md) and [Access Permissions](../../00200-core-concepts/00300-tables/00400-access-permissions.md).

Next, you can let players talk to each other with [chat rooms and private DMs](./00300-chat-rooms-and-dms.md).

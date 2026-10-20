---
title: Paginate or infinite-scroll a large table
slug: /how-to/app-patterns/pagination
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';
import { CppModuleVersionNotice } from "@site/src/components/CppModuleVersionNotice";

:::note Prerequisites
You need a module and a client connected to it. If you don't have them yet, follow the [quickstart](../../00100-intro/00100-getting-started/00100-getting-started.md) for your language first.

This guide uses views that take arguments, which were added in SpacetimeDB 2.13. C++ modules can't declare them yet.
:::

In this guide, you'll build a feed of posts that loads 50 posts at a time as the user scrolls. Each client only receives the pages it has loaded, and those pages stay live: new posts show up as soon as they're published. You'll number each post, and use a [view](../../00200-core-concepts/00200-functions/00500-views.md) that takes a page number as an argument.

## How it works

- A private `post` table holds the posts. Each post gets a number, 0, 1, 2 and so on, with no gaps.
- A `create_post` reducer numbers each new post, from a counter in a private `feed_state` table.
- A `feed_page` view takes a page number and returns that page's posts: page 0 holds posts 0 to 49, page 1 holds posts 50 to 99, and so on.
- A `feed_info` view tells clients which page is the newest.
- Clients subscribe to `feed_page` once for each page they load, and drop the subscriptions of pages they no longer need.

Subscriptions keep a set of rows up to date, but they can't sort rows or stop after a given number of them: they have no `ORDER BY` or `LIMIT`. Numbering the posts turns "the next 50 posts" into a range of numbers, which a view can look up directly through an index.

## Define the tables

Add two private tables:

- `post` holds the posts. Its primary key, `seq`, is the post's number.
- `feed_state` has a single row, with `id` 0, that holds the number the next post gets.

Number the posts with your own counter rather than an auto-increment column. Auto-increment values can have gaps, for example after a restart, which would leave pages with missing posts. See [Auto-Increment](../../00200-core-concepts/00300-tables/00230-auto-increment.md#concurrency-and-gaps).

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
import { schema, table, t, SenderError } from 'spacetimedb/server';

// Posts per page.
const PAGE_SIZE = 50n;

const post = table(
  { name: 'post' },
  {
    // Numbered 0, 1, 2... with no gaps, in the order posts were created.
    seq: t.u64().primaryKey(),
    author: t.identity(),
    text: t.string(),
    postedAt: t.timestamp(),
  }
);

// The number the next post gets, in a single row with id 0.
const feedState = table(
  { name: 'feed_state' },
  {
    id: t.u8().primaryKey(),
    nextSeq: t.u64(),
  }
);

const spacetimedb = schema({ post, feedState });
export default spacetimedb;
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
using SpacetimeDB;

public static partial class Module
{
    // Posts per page.
    const ulong PageSize = 50;

    [SpacetimeDB.Table(Accessor = "Post")]
    public partial struct Post
    {
        // Numbered 0, 1, 2... with no gaps, in the order posts were created.
        [SpacetimeDB.PrimaryKey]
        public ulong Seq;
        public Identity Author;
        public string Text;
        public Timestamp PostedAt;
    }

    // The number the next post gets, in a single row with id 0.
    [SpacetimeDB.Table(Accessor = "FeedState")]
    public partial struct FeedState
    {
        [SpacetimeDB.PrimaryKey]
        public byte Id;
        public ulong NextSeq;
    }
}
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
use spacetimedb::{reducer, table, view, AnonymousViewContext, Identity, Query, ReducerContext, SpacetimeType, Table, Timestamp};

// Posts per page.
const PAGE_SIZE: u64 = 50;

#[table(accessor = post)]
pub struct Post {
    // Numbered 0, 1, 2... with no gaps, in the order posts were created.
    #[primary_key]
    seq: u64,
    author: Identity,
    text: String,
    posted_at: Timestamp,
}

// The number the next post gets, in a single row with id 0.
#[table(accessor = feed_state)]
pub struct FeedState {
    #[primary_key]
    id: u8,
    next_seq: u64,
}
```

</TabItem>
<TabItem value="cpp" label="C++">

<CppModuleVersionNotice />

```cpp
#include <spacetimedb.h>
using namespace SpacetimeDB;

// Posts per page.
constexpr uint64_t PAGE_SIZE = 50;

struct Post {
    // Numbered 0, 1, 2... with no gaps, in the order posts were created.
    uint64_t seq;
    Identity author;
    std::string text;
    Timestamp posted_at;
};
SPACETIMEDB_STRUCT(Post, seq, author, text, posted_at)
SPACETIMEDB_TABLE(Post, post, Private)
FIELD_PrimaryKey(post, seq)

// The number the next post gets, in a single row with id 0.
struct FeedState {
    uint8_t id;
    uint64_t next_seq;
};
SPACETIMEDB_STRUCT(FeedState, id, next_seq)
SPACETIMEDB_TABLE(FeedState, feed_state, Private)
FIELD_PrimaryKey(feed_state, id)
```

</TabItem>
</Tabs>

## Number each post

Insert the counter from the `init` reducer, then add a `create_post` reducer that gives each new post the counter's value and increments it. A reducer runs as a single transaction, so two posts published at the same moment can never get the same number.

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
export const init = spacetimedb.init(ctx => {
  ctx.db.feedState.insert({ id: 0, nextSeq: 0n });
});

export const createPost = spacetimedb.reducer({ text: t.string() }, (ctx, { text }) => {
  if (text === '') {
    throw new SenderError("a post can't be empty");
  }
  const state = ctx.db.feedState.id.find(0);
  if (!state) {
    throw new Error('feed not initialized');
  }
  ctx.db.post.insert({ seq: state.nextSeq, author: ctx.sender, text, postedAt: ctx.timestamp });
  ctx.db.feedState.id.update({ ...state, nextSeq: state.nextSeq + 1n });
});
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
public static partial class Module
{
    [SpacetimeDB.Reducer(ReducerKind.Init)]
    public static void Init(ReducerContext ctx)
    {
        ctx.Db.FeedState.Insert(new FeedState { Id = 0, NextSeq = 0 });
    }

    [SpacetimeDB.Reducer]
    public static void CreatePost(ReducerContext ctx, string text)
    {
        if (text == "")
        {
            throw new Exception("A post can't be empty");
        }
        var state = ctx.Db.FeedState.Id.Find(0) ?? throw new Exception("Feed not initialized");
        ctx.Db.Post.Insert(new Post { Seq = state.NextSeq, Author = ctx.Sender, Text = text, PostedAt = ctx.Timestamp });
        state.NextSeq += 1;
        ctx.Db.FeedState.Id.Update(state);
    }
}
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
#[reducer(init)]
pub fn init(ctx: &ReducerContext) {
    ctx.db.feed_state().insert(FeedState { id: 0, next_seq: 0 });
}

#[reducer]
pub fn create_post(ctx: &ReducerContext, text: String) -> Result<(), String> {
    if text.is_empty() {
        return Err("a post can't be empty".to_string());
    }
    let mut state = ctx.db.feed_state().id().find(0).ok_or("feed not initialized")?;
    ctx.db.post().insert(Post {
        seq: state.next_seq,
        author: ctx.sender(),
        text,
        posted_at: ctx.timestamp,
    });
    state.next_seq += 1;
    ctx.db.feed_state().id().update(state);
    Ok(())
}
```

</TabItem>
<TabItem value="cpp" label="C++">

```cpp
SPACETIMEDB_INIT(init, ReducerContext ctx) {
    ctx.db[feed_state].insert(FeedState{0, 0});
    return Ok();
}

SPACETIMEDB_REDUCER(create_post, ReducerContext ctx, std::string text) {
    if (text.empty()) {
        return Err("a post can't be empty");
    }
    auto state = ctx.db[feed_state_id].find(uint8_t(0));
    if (!state) {
        return Err("feed not initialized");
    }
    FeedState updated = *state;
    ctx.db[post].insert(Post{updated.next_seq, ctx.sender(), text, ctx.timestamp});
    updated.next_seq += 1;
    ctx.db[feed_state_id].update(updated);
    return Ok();
}
```

</TabItem>
</Tabs>

## Define the views

Add two public anonymous views:

- `feed_page` takes a page number as an argument, and returns the posts of that page. It returns a query on the `seq` primary key, which SpacetimeDB answers with a single range lookup, and keeps up to date the same way as a subscription query.
- `feed_info` returns the page the newest post is on, so clients know where the feed starts.

Like a reducer, a view can take arguments. SpacetimeDB computes the view separately for each value of its arguments, and because `feed_page` is anonymous, every client that loads the same page shares one result. A view that takes arguments can only be subscribed to as a whole: clients can't add a filter to it.

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
// The posts of one page: page 0 holds posts 0 to 49, page 1 holds posts 50 to 99, and so on.
export const feedPage = spacetimedb.anonymousView(
  { name: 'feed_page', public: true },
  { page: t.u64() },
  t.array(post.rowType),
  (ctx, { page }) => {
    const first = page * PAGE_SIZE;
    return ctx.from.post.where(p => p.seq.gte(first).and(p.seq.lt(first + PAGE_SIZE)));
  }
);

const pageInfo = t.row('PageInfo', {
  newestPage: t.u64(),
});

// The page the newest post is on.
export const feedInfo = spacetimedb.anonymousView(
  { name: 'feed_info', public: true },
  t.array(pageInfo),
  ctx => {
    const state = ctx.db.feedState.id.find(0);
    if (!state) {
      return [];
    }
    const newestPage = state.nextSeq === 0n ? 0n : (state.nextSeq - 1n) / PAGE_SIZE;
    return [{ newestPage }];
  }
);
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
public static partial class Module
{
    // The posts of one page: page 0 holds posts 0 to 49, page 1 holds posts 50 to 99, and so on.
    [SpacetimeDB.View(Accessor = "FeedPage", Public = true)]
    public static IQuery<Post> FeedPage(AnonymousViewContext ctx, ulong page)
    {
        var first = page * PageSize;
        return ctx.From.Post().Where(p => p.Seq.Gte(first).And(p.Seq.Lt(first + PageSize)));
    }

    [SpacetimeDB.Type]
    public partial struct PageInfo
    {
        public ulong NewestPage;
    }

    // The page the newest post is on.
    [SpacetimeDB.View(Accessor = "FeedInfo", Public = true)]
    public static List<PageInfo> FeedInfo(AnonymousViewContext ctx)
    {
        if (ctx.Db.FeedState.Id.Find(0) is not FeedState state)
        {
            return new List<PageInfo>();
        }
        var newestPage = state.NextSeq == 0 ? 0 : (state.NextSeq - 1) / PageSize;
        return new List<PageInfo> { new PageInfo { NewestPage = newestPage } };
    }
}
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
// The posts of one page: page 0 holds posts 0 to 49, page 1 holds posts 50 to 99, and so on.
#[view(accessor = feed_page, public)]
fn feed_page(ctx: &AnonymousViewContext, page: u64) -> impl Query<Post> {
    let first = page * PAGE_SIZE;
    ctx.from
        .post()
        .r#where(move |p| p.seq.gte(first).and(p.seq.lt(first + PAGE_SIZE)))
}

#[derive(SpacetimeType)]
pub struct PageInfo {
    newest_page: u64,
}

// The page the newest post is on.
#[view(accessor = feed_info, public)]
fn feed_info(ctx: &AnonymousViewContext) -> Vec<PageInfo> {
    let Some(state) = ctx.db.feed_state().id().find(0) else {
        return vec![];
    };
    vec![PageInfo {
        newest_page: state.next_seq.saturating_sub(1) / PAGE_SIZE,
    }]
}
```

</TabItem>
<TabItem value="cpp" label="C++">

C++ modules can't declare views that take arguments yet, so `feed_page` isn't available in C++.

```cpp
struct PageInfo {
    uint64_t newest_page;
};
SPACETIMEDB_STRUCT(PageInfo, newest_page)

// The page the newest post is on.
SPACETIMEDB_VIEW(std::vector<PageInfo>, feed_info, Public, AnonymousViewContext ctx) {
    auto state = ctx.db[feed_state_id].find(uint8_t(0));
    if (!state) {
        return {};
    }
    uint64_t newest_page = state->next_seq == 0 ? 0 : (state->next_seq - 1) / PAGE_SIZE;
    return {PageInfo{newest_page}};
}
```

</TabItem>
</Tabs>

## Load pages from the client

On the client, subscribe to `feed_info`. When it arrives, subscribe to the newest page and the page before it, so the screen is full even when the newest page only has a few posts. `feed_info` changes each time a new page starts, so the client loads new pages as they're created.

Keep the subscription handle of each loaded page:

- When the user scrolls to the end of the feed, subscribe to the page before the oldest one loaded.
- When a page is far off screen, unsubscribe from it. Its posts leave the client cache.

In TypeScript and C#, subscribe to a view that takes arguments with SQL, for example `SELECT * FROM feed_page(3)`. In Rust, use the typed `feed_page(3)` query. The posts of every loaded page arrive in the same `feed_page` table in the client cache, which doesn't keep them in order, so sort them by `seq` before you display them.

<Tabs groupId="client-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
import { DbConnection, tables, type SubscriptionHandle } from './module_bindings';

const conn = DbConnection.builder()
  .withUri('ws://localhost:3000')
  .withDatabaseName('my-app')
  .onConnect(ctx => {
    ctx.subscriptionBuilder().subscribe([tables.feedInfo]);
  })
  .build();

// The subscription of each loaded page.
const pages = new Map<bigint, SubscriptionHandle>();

function loadPage(page: bigint) {
  if (page < 0n || pages.has(page)) {
    return;
  }
  // Views that take arguments are subscribed to with SQL.
  pages.set(page, conn.subscriptionBuilder().subscribe(`SELECT * FROM feed_page(${page})`));
}

// Load the newest page and the one before it, so the screen is full even when
// the newest page only has a few posts. This runs again each time a new page starts.
conn.db.feedInfo.onInsert((_ctx, info) => {
  loadPage(info.newestPage);
  loadPage(info.newestPage - 1n);
});

// Call this when the user scrolls to the end of the feed.
function loadOlderPage() {
  if (pages.size > 0) {
    const oldest = [...pages.keys()].reduce((a, b) => (a < b ? a : b));
    loadPage(oldest - 1n);
  }
}

// Call this to drop a page that's far off screen.
function unloadPage(page: bigint) {
  pages.get(page)?.unsubscribe();
  pages.delete(page);
}

// Call this each frame to draw the loaded posts, newest first.
function drawFeed() {
  const posts = Array.from(conn.db.feedPage.iter()).sort((a, b) => (a.seq < b.seq ? 1 : -1));
  for (const post of posts) {
    console.log(`#${post.seq}: ${post.text}`);
  }
}

// Call this when the user publishes a post.
function publish(text: string) {
  conn.reducers.createPost({ text });
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
        conn.SubscriptionBuilder()
            .AddQuery(q => q.From.FeedInfo())
            .Subscribe();
    })
    .Build();

// The subscription of each loaded page.
var pages = new Dictionary<ulong, SubscriptionHandle>();

void LoadPage(long page)
{
    if (page < 0 || pages.ContainsKey((ulong)page))
    {
        return;
    }
    // Views that take arguments are subscribed to with SQL.
    pages[(ulong)page] = conn.SubscriptionBuilder().Subscribe(new[] { $"SELECT * FROM feed_page({page})" });
}

// Load the newest page and the one before it, so the screen is full even when
// the newest page only has a few posts. This runs again each time a new page starts.
conn.Db.FeedInfo.OnInsert += (ctx, info) =>
{
    LoadPage((long)info.NewestPage);
    LoadPage((long)info.NewestPage - 1);
};

// Call this when the user scrolls to the end of the feed.
void LoadOlderPage()
{
    if (pages.Count > 0)
    {
        LoadPage((long)pages.Keys.Min() - 1);
    }
}

// Call this to drop a page that's far off screen.
void UnloadPage(ulong page)
{
    if (pages.Remove(page, out var handle))
    {
        handle.Unsubscribe();
    }
}

// Call this each frame to draw the loaded posts, newest first.
void DrawFeed()
{
    foreach (var post in conn.Db.FeedPage.Iter().OrderByDescending(p => p.Seq))
    {
        Console.WriteLine($"#{post.Seq}: {post.Text}");
    }
}

// Call this when the user publishes a post.
void Publish(string text) => conn.Reducers.CreatePost(text);
```

Remember to call `conn.FrameTick()` regularly, for example once per frame, so the callbacks run.

</TabItem>
<TabItem value="rust" label="Rust">

```rust
mod module_bindings;
use module_bindings::*;
use spacetimedb_sdk::{DbContext, SubscriptionHandle as _, Table};
use std::collections::BTreeMap;
use std::sync::Mutex;

// The subscription of each loaded page.
static PAGES: Mutex<BTreeMap<u64, SubscriptionHandle>> = Mutex::new(BTreeMap::new());

fn load_page(ctx: &impl RemoteDbContext, page: u64) {
    let mut pages = PAGES.lock().unwrap();
    if !pages.contains_key(&page) {
        let handle = ctx
            .subscription_builder()
            .add_query(move |q| q.from.feed_page(page))
            .subscribe();
        pages.insert(page, handle);
    }
}

// Call this when the user scrolls to the end of the feed.
fn load_older_page(conn: &DbConnection) {
    let oldest = PAGES.lock().unwrap().keys().next().copied();
    if let Some(oldest) = oldest.filter(|&page| page > 0) {
        load_page(conn, oldest - 1);
    }
}

// Call this to drop a page that's far off screen.
fn unload_page(page: u64) {
    if let Some(handle) = PAGES.lock().unwrap().remove(&page) {
        let _ = handle.unsubscribe();
    }
}

// Call this each frame to draw the loaded posts, newest first.
fn draw_feed(conn: &DbConnection) {
    let mut posts: Vec<_> = conn.db().feed_page().iter().collect();
    posts.sort_by(|a, b| b.seq.cmp(&a.seq));
    for post in posts {
        println!("#{}: {}", post.seq, post.text);
    }
}

fn main() {
    let conn = DbConnection::builder()
        .with_uri("http://localhost:3000")
        .with_database_name("my-app")
        .on_connect(|ctx, _identity, _token| {
            ctx.subscription_builder()
                .add_query(|q| q.from.feed_info())
                .subscribe();
        })
        .build()
        .expect("failed to connect");

    // Load the newest page and the one before it, so the screen is full even when
    // the newest page only has a few posts. This runs again each time a new page starts.
    conn.db().feed_info().on_insert(|ctx, info| {
        load_page(ctx, info.newest_page);
        if info.newest_page > 0 {
            load_page(ctx, info.newest_page - 1);
        }
    });

    // Process messages from the database on a background thread.
    conn.run_threaded();

    // Your app's main loop runs here.
}

// Call this when the user publishes a post.
fn publish(conn: &DbConnection, text: String) {
    conn.reducers().create_post(text).unwrap();
}
```

</TabItem>
</Tabs>

## What's next?

You now have a feed that loads 50 posts at a time, and keeps the loaded pages live:

- Each post gets a number with no gaps, from a counter updated in the same transaction.
- The `feed_page` view takes a page number and returns that page's posts through a range lookup, shared by every client that loads the same page.
- Clients subscribe to one page at a time, and unsubscribe from the pages they no longer show.

To adapt it to your app:

- **Several feeds.** For a feed per channel, add a `channel_id` column to `post`, keep a counter per channel, and give `feed_page` a second argument: `feed_page(channel_id, page)`.
- **Jump to a page.** Subscribe to any page directly, for example page 0 to show the oldest posts.
- **Deleted posts.** Deleting a post leaves its page one post short. Don't renumber the posts: that would move every later post to a different page.
- **Other sort orders.** Numbering only works for one order, the order posts were created in. To page through rows in another order, such as search results, return one page of results from a [procedure](../../00200-core-concepts/00200-functions/00400-procedures.md) instead. That page isn't live.

To learn more about the features used here, see [Views](../../00200-core-concepts/00200-functions/00500-views.md), [Subscriptions](../../00200-core-concepts/00400-subscriptions.md) and [Auto-Increment](../../00200-core-concepts/00300-tables/00230-auto-increment.md).

Next, you can [add admin and moderator roles](./00200-admin-roles.md).

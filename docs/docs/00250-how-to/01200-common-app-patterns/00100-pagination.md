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

In this guide, you'll build a feed of posts that loads 50 posts at a time as the user scrolls. Each client only receives the pages it has loaded, and those pages stay live: new posts show up as soon as they're published. You'll use a [view](../../00200-core-concepts/00200-functions/00500-views.md) that takes a page number and a page size as arguments.

## How it works

- A private `post` table holds the posts, with an index on their `id`.
- A `create_post` reducer adds a post.
- A `feed_page` view takes a page number and a page size, and returns the posts of that page, newest first. With a page size of 50, page 0 holds the 50 newest posts, page 1 the 50 before them, and so on.
- Clients subscribe to `feed_page` once for each page they load, and drop the subscriptions of pages they no longer need.

Subscriptions keep a set of rows up to date, but they can't sort rows or stop after a given number of them: they have no `ORDER BY` or `LIMIT`. A view runs your own code, so it can sort the posts and return a single page.

## Define the table

Add a private `post` table. Its `id` is assigned automatically, in the order posts are created, and has an index. The view needs this index: views can't scan a whole table with `iter()`, so they read rows through indexes instead. It's a regular index rather than a primary key, because a primary key can only look up one value at a time, not a range of them.

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
import { schema, table, t, Range, SenderError } from 'spacetimedb/server';

const post = table(
  { name: 'post' },
  {
    id: t.u64().autoInc().index('btree'),
    author: t.identity(),
    text: t.string(),
    postedAt: t.timestamp(),
  }
);

const spacetimedb = schema({ post });
export default spacetimedb;
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
using SpacetimeDB;

public static partial class Module
{
    [SpacetimeDB.Table(Accessor = "Post")]
    public partial struct Post
    {
        [SpacetimeDB.AutoInc]
        [SpacetimeDB.Index.BTree]
        public ulong Id;
        public Identity Author;
        public string Text;
        public Timestamp PostedAt;
    }
}
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
use spacetimedb::{reducer, table, view, AnonymousViewContext, Identity, ReducerContext, Table, Timestamp};

#[table(accessor = post)]
pub struct Post {
    #[auto_inc]
    #[index(btree)]
    id: u64,
    author: Identity,
    text: String,
    posted_at: Timestamp,
}
```

</TabItem>
<TabItem value="cpp" label="C++">

<CppModuleVersionNotice />

```cpp
#include <spacetimedb.h>
using namespace SpacetimeDB;

struct Post {
    uint64_t id;
    Identity author;
    std::string text;
    Timestamp posted_at;
};
SPACETIMEDB_STRUCT(Post, id, author, text, posted_at)
SPACETIMEDB_TABLE(Post, post, Private)
FIELD_IndexAutoInc(post, id)
```

</TabItem>
</Tabs>

## Add a post

Add a `create_post` reducer that inserts a post. Pass 0 as the `id`: SpacetimeDB replaces it with the next value.

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
export const createPost = spacetimedb.reducer({ text: t.string() }, (ctx, { text }) => {
  if (text === '') {
    throw new SenderError("a post can't be empty");
  }
  ctx.db.post.insert({ id: 0n, author: ctx.sender, text, postedAt: ctx.timestamp });
});
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
public static partial class Module
{
    [SpacetimeDB.Reducer]
    public static void CreatePost(ReducerContext ctx, string text)
    {
        if (text == "")
        {
            throw new Exception("A post can't be empty");
        }
        ctx.Db.Post.Insert(new Post { Id = 0, Author = ctx.Sender, Text = text, PostedAt = ctx.Timestamp });
    }
}
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
#[reducer]
pub fn create_post(ctx: &ReducerContext, text: String) -> Result<(), String> {
    if text.is_empty() {
        return Err("a post can't be empty".to_string());
    }
    ctx.db.post().insert(Post {
        id: 0,
        author: ctx.sender(),
        text,
        posted_at: ctx.timestamp,
    });
    Ok(())
}
```

</TabItem>
<TabItem value="cpp" label="C++">

```cpp
SPACETIMEDB_REDUCER(create_post, ReducerContext ctx, std::string text) {
    if (text.empty()) {
        return Err("a post can't be empty");
    }
    ctx.db[post].insert(Post{0, ctx.sender(), text, ctx.timestamp});
    return Ok();
}
```

</TabItem>
</Tabs>

## Define the view

Add a public anonymous view named `feed_page`, with two arguments: the page number and the page size. It reads every post with a range query on the `id` index, starting at 0 and with no upper bound. Then it sorts the posts from newest to oldest, skips the posts of the pages before, and returns the next ones. It caps the page size at 100, so a client can't ask for the whole feed at once.

Like a reducer, a view can take arguments. SpacetimeDB computes the view separately for each set of argument values, and because `feed_page` is anonymous, every client that loads the same page with the same page size shares one result. A view that takes arguments can only be subscribed to as a whole: clients can't add a filter to it.

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
// The largest page size a client can ask for.
const MAX_PAGE_SIZE = 100;

// One page of the feed, newest posts first.
export const feedPage = spacetimedb.anonymousView(
  { name: 'feed_page', public: true },
  { page: t.u32(), pageSize: t.u32() },
  t.array(post.rowType),
  (ctx, { page, pageSize }) => {
    const size = Math.min(pageSize, MAX_PAGE_SIZE);
    const allPosts = new Range({ tag: 'included', value: 0n }, { tag: 'unbounded' });
    const posts = Array.from(ctx.db.post.id.filter(allPosts)).sort((a, b) => (a.id < b.id ? 1 : -1));
    return posts.slice(page * size, (page + 1) * size);
  }
);
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
public static partial class Module
{
    // The largest page size a client can ask for.
    const uint MaxPageSize = 100;

    // One page of the feed, newest posts first.
    [SpacetimeDB.View(Accessor = "FeedPage", Public = true)]
    public static List<Post> FeedPage(AnonymousViewContext ctx, uint page, uint pageSize)
    {
        var size = Math.Min(pageSize, MaxPageSize);
        return ctx.Db.Post.Id.Filter((0UL, ulong.MaxValue))
            .OrderByDescending(p => p.Id)
            .Skip((int)Math.Min((long)page * size, int.MaxValue))
            .Take((int)size)
            .ToList();
    }
}
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
// The largest page size a client can ask for.
const MAX_PAGE_SIZE: u32 = 100;

// One page of the feed, newest posts first.
#[view(accessor = feed_page, public)]
fn feed_page(ctx: &AnonymousViewContext, page: u32, page_size: u32) -> Vec<Post> {
    let size = page_size.min(MAX_PAGE_SIZE) as usize;
    let mut posts: Vec<Post> = ctx.db.post().id().filter(0u64..).collect();
    posts.sort_by(|a, b| b.id.cmp(&a.id));
    posts
        .into_iter()
        .skip((page as usize).saturating_mul(size))
        .take(size)
        .collect()
}
```

</TabItem>
<TabItem value="cpp" label="C++">

C++ modules can't declare views that take arguments yet, so `feed_page` isn't available in C++.

</TabItem>
</Tabs>

Because `feed_page` reads every post, SpacetimeDB runs it again each time a post is created or deleted, once for each page that clients have loaded. Each run reads the whole feed, so keep the number of posts each view reads in check: for example, give each channel its own feed, as described in [What's next](#whats-next).

## Load pages from the client

On the client, subscribe to page 0 to show the newest posts. Keep the subscription handle of each loaded page:

- When the user scrolls to the end of the feed, subscribe to the next page. If the loaded pages hold fewer posts than they have room for, the last one isn't full and there are no older posts to load.
- When a page is far off screen, unsubscribe from it. Its posts leave the client cache.

In TypeScript and C#, subscribe to a view that takes arguments with SQL, for example `SELECT * FROM feed_page(3, 50)`. In Rust, use the typed `feed_page(3, 50)` query. The posts of every loaded page arrive in the same `feed_page` table in the client cache, which doesn't keep them in order, so sort them by `id` before you display them.

Pages count from the newest post, so a new post moves every post one place down: each loaded page gains a post at the top and passes one on to the next page. The loaded pages keep covering the same positions in the feed, for example the 100 newest posts when pages 0 and 1 are loaded.

<Tabs groupId="client-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
import { DbConnection, type SubscriptionHandle } from './module_bindings';

// Posts per page.
const PAGE_SIZE = 50;

const conn = DbConnection.builder()
  .withUri('ws://localhost:3000')
  .withDatabaseName('my-app')
  // Start with page 0, the newest posts.
  .onConnect(() => loadPage(0))
  .build();

// The subscription of each loaded page.
const pages = new Map<number, SubscriptionHandle>();

function loadPage(page: number) {
  if (pages.has(page)) {
    return;
  }
  // Views that take arguments are subscribed to with SQL.
  pages.set(page, conn.subscriptionBuilder().subscribe(`SELECT * FROM feed_page(${page}, ${PAGE_SIZE})`));
}

// Call this when the user scrolls to the end of the feed.
function loadOlderPage() {
  const loaded = [...pages.keys()];
  // When the loaded pages aren't full, there are no older posts.
  if (loaded.length === 0 || conn.db.feedPage.count() < BigInt(loaded.length * PAGE_SIZE)) {
    return;
  }
  loadPage(Math.max(...loaded) + 1);
}

// Call this to drop a page that's far off screen.
function unloadPage(page: number) {
  pages.get(page)?.unsubscribe();
  pages.delete(page);
}

// Call this each frame to draw the loaded posts, newest first.
function drawFeed() {
  const posts = Array.from(conn.db.feedPage.iter()).sort((a, b) => (a.id < b.id ? 1 : -1));
  for (const post of posts) {
    console.log(`#${post.id}: ${post.text}`);
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

// Posts per page.
const int PageSize = 50;

// The subscription of each loaded page.
var pages = new Dictionary<uint, SubscriptionHandle>();

var conn = DbConnection.Builder()
    .WithUri("http://localhost:3000")
    .WithDatabaseName("my-app")
    // Start with page 0, the newest posts.
    .OnConnect((conn, identity, token) => LoadPage(conn, 0))
    .Build();

void LoadPage(DbConnection conn, uint page)
{
    if (pages.ContainsKey(page))
    {
        return;
    }
    // Views that take arguments are subscribed to with SQL.
    pages[page] = conn.SubscriptionBuilder().Subscribe(new[] { $"SELECT * FROM feed_page({page}, {PageSize})" });
}

// Call this when the user scrolls to the end of the feed.
void LoadOlderPage()
{
    // When the loaded pages aren't full, there are no older posts.
    if (pages.Count == 0 || conn.Db.FeedPage.Count < pages.Count * PageSize)
    {
        return;
    }
    LoadPage(conn, pages.Keys.Max() + 1);
}

// Call this to drop a page that's far off screen.
void UnloadPage(uint page)
{
    if (pages.Remove(page, out var handle))
    {
        handle.Unsubscribe();
    }
}

// Call this each frame to draw the loaded posts, newest first.
void DrawFeed()
{
    foreach (var post in conn.Db.FeedPage.Iter().OrderByDescending(p => p.Id))
    {
        Console.WriteLine($"#{post.Id}: {post.Text}");
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

// Posts per page.
const PAGE_SIZE: u32 = 50;

// The subscription of each loaded page.
static PAGES: Mutex<BTreeMap<u32, SubscriptionHandle>> = Mutex::new(BTreeMap::new());

fn load_page(ctx: &impl RemoteDbContext, page: u32) {
    let mut pages = PAGES.lock().unwrap();
    if !pages.contains_key(&page) {
        let handle = ctx
            .subscription_builder()
            .add_query(move |q| q.from.feed_page(page, PAGE_SIZE))
            .subscribe();
        pages.insert(page, handle);
    }
}

// Call this when the user scrolls to the end of the feed.
fn load_older_page(conn: &DbConnection) {
    let (loaded, last) = {
        let pages = PAGES.lock().unwrap();
        (pages.len() as u64, pages.keys().next_back().copied())
    };
    // When the loaded pages aren't full, there are no older posts.
    if let Some(last) = last {
        if conn.db().feed_page().count() == loaded * PAGE_SIZE as u64 {
            load_page(conn, last + 1);
        }
    }
}

// Call this to drop a page that's far off screen.
fn unload_page(page: u32) {
    if let Some(handle) = PAGES.lock().unwrap().remove(&page) {
        let _ = handle.unsubscribe();
    }
}

// Call this each frame to draw the loaded posts, newest first.
fn draw_feed(conn: &DbConnection) {
    let mut posts: Vec<_> = conn.db().feed_page().iter().collect();
    posts.sort_by(|a, b| b.id.cmp(&a.id));
    for post in posts {
        println!("#{}: {}", post.id, post.text);
    }
}

fn main() {
    let conn = DbConnection::builder()
        .with_uri("http://localhost:3000")
        .with_database_name("my-app")
        // Start with page 0, the newest posts.
        .on_connect(|ctx, _identity, _token| load_page(ctx, 0))
        .build()
        .expect("failed to connect");

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

- The `feed_page` view takes a page number and a page size, and returns that page of posts, newest first. Every client that loads the same page shares one result.
- Clients subscribe to one page at a time, and unsubscribe from the pages they no longer show.

To adapt it to your app:

- **Several feeds.** For a feed per channel, add an indexed `channel_id` column to `post` and give `feed_page` a third argument: `feed_page(channel_id, page, page_size)`. Read the posts with `filter` on `channel_id`, so each page only reads, and only runs again for, the posts of its own channel.
- **Jump to a page.** Subscribe to any page directly, for example to show page 10 without loading pages 0 to 9.
- **Other sort orders.** To sort by something else, such as likes, index that column and sort by it in the view.

To learn more about the features used here, see [Views](../../00200-core-concepts/00200-functions/00500-views.md), [Subscriptions](../../00200-core-concepts/00400-subscriptions.md) and [Indexes](../../00200-core-concepts/00300-tables/00300-indexes.md).

Next, you can [add admin and moderator roles](./00200-admin-roles.md).

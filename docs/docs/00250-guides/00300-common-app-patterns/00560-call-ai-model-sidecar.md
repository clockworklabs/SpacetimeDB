---
title: Call an AI model from a sidecar
slug: /guides/app-patterns/call-ai-model-sidecar
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';

:::note Prerequisites
You need a module and a client connected to it, [Node.js](https://nodejs.org/) to run the sidecar, and an API key from the provider of the AI model you want to use. If you don't have a module yet, follow the [quickstart](../../00100-intro/00100-getting-started/00100-getting-started.md) for your language first.
:::

In this guide, you'll let players ask an AI model for a reply, and have a sidecar answer them. A sidecar is a separate program that runs next to your database: it connects to it like any client, picks up the requests players make, calls the model, and writes the replies back.

Compared with [calling the model from your module](./00550-call-ai-model.md), a sidecar can use your provider's official SDK, stream replies, wait as long as a reply takes, and keep your API key out of the database. The module only stores requests and replies, so the same pattern works for any slow or external work, not just AI.

## How it works

- An `ai_request` table holds every request: who asked, the prompt, and the reply or error once it's done.
- Players call a `request_ai_reply` reducer to add a request, and read their own requests through a `my_ai_requests` view.
- The sidecar subscribes to a `pending_ai_requests` view, calls the model for each request, then calls a `complete_ai_request` reducer with the reply.
- An `ai_worker` table lists the identities allowed to read the queue and complete requests, so only your sidecar can answer.

## Define the tables

Add the `ai_request` and `ai_worker` tables. Both are private: players and the sidecar read them only through the views you'll add below. The `done` column has an index so the sidecar's view can find pending requests without reading every request.

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
import { schema, table, t, SenderError } from 'spacetimedb/server';

const aiRequest = table(
  { name: 'ai_request' },
  {
    id: t.u64().primaryKey().autoInc(),
    requester: t.identity().index('btree'),
    prompt: t.string(),
    text: t.string(),
    error: t.string(),
    done: t.bool().index('btree'),
    createdAt: t.timestamp(),
  }
);

const aiWorker = table(
  { name: 'ai_worker' },
  {
    identity: t.identity().primaryKey(),
  }
);

const spacetimedb = schema({ aiRequest, aiWorker });
export default spacetimedb;
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
using SpacetimeDB;

public static partial class Module
{
    [SpacetimeDB.Table(Accessor = "AiRequest")]
    public partial struct AiRequest
    {
        [SpacetimeDB.PrimaryKey]
        [SpacetimeDB.AutoInc]
        public ulong Id;

        [SpacetimeDB.Index.BTree]
        public Identity Requester;

        public string Prompt;
        public string Text;
        public string Error;

        [SpacetimeDB.Index.BTree]
        public bool Done;

        public Timestamp CreatedAt;
    }

    [SpacetimeDB.Table(Accessor = "AiWorker")]
    public partial struct AiWorker
    {
        [SpacetimeDB.PrimaryKey]
        public Identity Identity;
    }
}
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
use spacetimedb::{reducer, table, view, Identity, ReducerContext, Table, Timestamp, ViewContext};

#[table(accessor = ai_request)]
pub struct AiRequest {
    #[primary_key]
    #[auto_inc]
    id: u64,
    #[index(btree)]
    requester: Identity,
    prompt: String,
    text: String,
    error: String,
    #[index(btree)]
    done: bool,
    created_at: Timestamp,
}

#[table(accessor = ai_worker)]
pub struct AiWorker {
    #[primary_key]
    identity: Identity,
}
```

</TabItem>
</Tabs>

## Let players request a reply

Add a `request_ai_reply` reducer that players call with their prompt. It rejects empty or overly long prompts, and players who already have a request waiting, so one player can't fill the queue.

Then add a `my_ai_requests` view that returns the caller's own requests. Players subscribe to it to get their replies, and they never see anyone else's prompts.

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
const MAX_PROMPT_LENGTH = 2000;

export const requestAiReply = spacetimedb.reducer({ prompt: t.string() }, (ctx, { prompt }) => {
  if (prompt.length === 0 || prompt.length > MAX_PROMPT_LENGTH) {
    throw new SenderError(`The prompt must be 1 to ${MAX_PROMPT_LENGTH} characters long`);
  }
  // Each player can have one request waiting at a time.
  for (const request of ctx.db.aiRequest.requester.filter(ctx.sender)) {
    if (!request.done) {
      throw new SenderError('You already have a request waiting for a reply');
    }
  }
  ctx.db.aiRequest.insert({
    id: 0n,
    requester: ctx.sender,
    prompt,
    text: '',
    error: '',
    done: false,
    createdAt: ctx.timestamp,
  });
});

export const myAiRequests = spacetimedb.view(
  { name: 'my_ai_requests', public: true },
  t.array(aiRequest.rowType),
  ctx => Array.from(ctx.db.aiRequest.requester.filter(ctx.sender))
);
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
public static partial class Module
{
    const int MaxPromptLength = 2000;

    [SpacetimeDB.Reducer]
    public static void RequestAiReply(ReducerContext ctx, string prompt)
    {
        if (prompt.Length == 0 || prompt.Length > MaxPromptLength)
        {
            throw new Exception($"The prompt must be 1 to {MaxPromptLength} characters long");
        }
        // Each player can have one request waiting at a time.
        if (ctx.Db.AiRequest.Requester.Filter(ctx.Sender).Any(request => !request.Done))
        {
            throw new Exception("You already have a request waiting for a reply");
        }
        ctx.Db.AiRequest.Insert(new AiRequest
        {
            Id = 0,
            Requester = ctx.Sender,
            Prompt = prompt,
            Text = "",
            Error = "",
            Done = false,
            CreatedAt = ctx.Timestamp,
        });
    }

    [SpacetimeDB.View(Accessor = "MyAiRequests", Public = true, PrimaryKey = "Id")]
    public static List<AiRequest> MyAiRequests(ViewContext ctx)
    {
        return ctx.Db.AiRequest.Requester.Filter(ctx.Sender).ToList();
    }
}
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
const MAX_PROMPT_LENGTH: usize = 2000;

#[reducer]
pub fn request_ai_reply(ctx: &ReducerContext, prompt: String) -> Result<(), String> {
    if prompt.is_empty() || prompt.chars().count() > MAX_PROMPT_LENGTH {
        return Err(format!("The prompt must be 1 to {MAX_PROMPT_LENGTH} characters long"));
    }
    // Each player can have one request waiting at a time.
    if ctx.db.ai_request().requester().filter(ctx.sender()).any(|request| !request.done) {
        return Err("You already have a request waiting for a reply".to_string());
    }
    ctx.db.ai_request().insert(AiRequest {
        id: 0,
        requester: ctx.sender(),
        prompt,
        text: String::new(),
        error: String::new(),
        done: false,
        created_at: ctx.timestamp,
    });
    Ok(())
}

#[view(accessor = my_ai_requests, public, primary_key = id)]
fn my_ai_requests(ctx: &ViewContext) -> Vec<AiRequest> {
    ctx.db.ai_request().requester().filter(ctx.sender()).collect()
}
```

</TabItem>
</Tabs>

The view has a primary key, so when a request changes, clients receive an update to the existing row instead of a deletion and a new row.

## Let the sidecar answer

Add a `pending_ai_requests` view that returns the requests waiting for a reply, and a `complete_ai_request` reducer that stores a reply. Both check that the caller is listed in `ai_worker`: anyone else gets an empty queue and can't complete requests.

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
export const pendingAiRequests = spacetimedb.view(
  { name: 'pending_ai_requests', public: true },
  t.array(aiRequest.rowType),
  ctx => {
    // Only AI workers can see the queue.
    if (!ctx.db.aiWorker.identity.find(ctx.sender)) {
      return [];
    }
    return Array.from(ctx.db.aiRequest.done.filter(false));
  }
);

export const completeAiRequest = spacetimedb.reducer(
  { id: t.u64(), text: t.string(), error: t.string() },
  (ctx, { id, text, error }) => {
    if (!ctx.db.aiWorker.identity.find(ctx.sender)) {
      throw new SenderError('Only AI workers can complete requests');
    }
    const request = ctx.db.aiRequest.id.find(id);
    if (!request) {
      throw new SenderError('No such request');
    }
    if (request.done) {
      throw new SenderError('This request is already complete');
    }
    ctx.db.aiRequest.id.update({ ...request, text, error, done: true });
  }
);
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
public static partial class Module
{
    [SpacetimeDB.View(Accessor = "PendingAiRequests", Public = true, PrimaryKey = "Id")]
    public static List<AiRequest> PendingAiRequests(ViewContext ctx)
    {
        // Only AI workers can see the queue.
        if (ctx.Db.AiWorker.Identity.Find(ctx.Sender) is null)
        {
            return new List<AiRequest>();
        }
        return ctx.Db.AiRequest.Done.Filter(false).ToList();
    }

    [SpacetimeDB.Reducer]
    public static void CompleteAiRequest(ReducerContext ctx, ulong id, string text, string error)
    {
        if (ctx.Db.AiWorker.Identity.Find(ctx.Sender) is null)
        {
            throw new Exception("Only AI workers can complete requests");
        }
        var request = ctx.Db.AiRequest.Id.Find(id) ?? throw new Exception("No such request");
        if (request.Done)
        {
            throw new Exception("This request is already complete");
        }
        request.Text = text;
        request.Error = error;
        request.Done = true;
        ctx.Db.AiRequest.Id.Update(request);
    }
}
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
fn require_worker(ctx: &ReducerContext) -> Result<(), String> {
    match ctx.db.ai_worker().identity().find(ctx.sender()) {
        Some(_) => Ok(()),
        None => Err("Only AI workers can do this".to_string()),
    }
}

#[view(accessor = pending_ai_requests, public, primary_key = id)]
fn pending_ai_requests(ctx: &ViewContext) -> Vec<AiRequest> {
    // Only AI workers can see the queue.
    if ctx.db.ai_worker().identity().find(ctx.sender()).is_none() {
        return vec![];
    }
    ctx.db.ai_request().done().filter(false).collect()
}

#[reducer]
pub fn complete_ai_request(ctx: &ReducerContext, id: u64, text: String, error: String) -> Result<(), String> {
    require_worker(ctx)?;
    let request = ctx.db.ai_request().id().find(id).ok_or("No such request")?;
    if request.done {
        return Err("This request is already complete".to_string());
    }
    ctx.db.ai_request().id().update(AiRequest {
        text,
        error,
        done: true,
        ..request
    });
    Ok(())
}
```

</TabItem>
</Tabs>

Completed requests stay in `ai_request`, so players can read their past replies. Delete old ones from time to time, for example from a [scheduled reducer](../../00200-core-concepts/00300-tables/00500-schedule-tables.md), so the table doesn't grow forever.

## Write the sidecar

The sidecar is a Node.js program written in TypeScript. In a new directory next to your module, install the SpacetimeDB SDK, `tsx` to run TypeScript, and your provider's SDK, then generate the bindings for your module:

<Tabs groupId="ai-provider" queryString>
<TabItem value="anthropic" label="Anthropic">

```bash
npm install spacetimedb tsx @anthropic-ai/sdk
spacetime generate --lang typescript --out-dir src/module_bindings --module-path ../spacetimedb
```

</TabItem>
<TabItem value="openai" label="OpenAI">

```bash
npm install spacetimedb tsx openai
spacetime generate --lang typescript --out-dir src/module_bindings --module-path ../spacetimedb
```

</TabItem>
<TabItem value="google" label="Google">

```bash
npm install spacetimedb tsx @google/genai
spacetime generate --lang typescript --out-dir src/module_bindings --module-path ../spacetimedb
```

</TabItem>
</Tabs>

Create `src/sidecar.ts`. The sidecar connects to your database, saves its token so it keeps the same identity when it restarts, and subscribes to `pending_ai_requests`. For each pending request, it calls the model and then `complete_ai_request`. If the model call fails, it completes the request with an error instead, so the player isn't left waiting.

```typescript
import { existsSync, readFileSync, writeFileSync } from 'node:fs';
import { DbConnection, tables } from './module_bindings';
import type { AiRequest } from './module_bindings/types';

// The sidecar saves its token, so it keeps the same identity when it restarts.
const TOKEN_FILE = 'sidecar-token.txt';

const conn = DbConnection.builder()
  .withUri('ws://localhost:3000')
  .withDatabaseName('my-game')
  .withToken(existsSync(TOKEN_FILE) ? readFileSync(TOKEN_FILE, 'utf8') : undefined)
  .onConnect((ctx, identity, token) => {
    writeFileSync(TOKEN_FILE, token);
    console.log(`Sidecar connected as ${identity.toHexString()}`);
    ctx.subscriptionBuilder().subscribe([tables.pendingAiRequests]);
  })
  .build();

// Requests the sidecar is working on, so each one is handled only once.
const inProgress = new Set<bigint>();

// Runs for every pending request, including those waiting when the sidecar starts.
conn.db.pendingAiRequests.onInsert((_ctx, request) => {
  void handleRequest(request);
});

async function handleRequest(request: AiRequest) {
  if (inProgress.has(request.id)) {
    return;
  }
  inProgress.add(request.id);
  try {
    const text = await callModel(request.prompt);
    conn.reducers.completeAiRequest({ id: request.id, text, error: '' });
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    conn.reducers.completeAiRequest({ id: request.id, text: '', error: message });
  } finally {
    inProgress.delete(request.id);
  }
}
```

Then add the `callModel` function for your provider at the end of the file. Each provider's SDK reads its API key from an environment variable, so the key never enters your database.

The model names below are examples. Providers release new models often, so check your provider's documentation for the model you want.

<Tabs groupId="ai-provider" queryString>
<TabItem value="anthropic" label="Anthropic">

This uses the [Anthropic TypeScript SDK](https://platform.claude.com/docs/en/api/sdks/typescript). `fallbacks` asks Anthropic to retry on another model if the model declines the request; a request that is still declined comes back with a `refusal` stop reason.

```typescript
import Anthropic from '@anthropic-ai/sdk';

// Reads the API key from the ANTHROPIC_API_KEY environment variable.
const anthropic = new Anthropic();

async function callModel(prompt: string): Promise<string> {
  const message = await anthropic.beta.messages.create({
    model: 'claude-opus-5-5',
    max_tokens: 1024,
    output_config: { effort: 'low' },
    betas: ['server-side-fallback-2026-07-01'],
    fallbacks: 'default',
    messages: [{ role: 'user', content: prompt }],
  });
  if (message.stop_reason === 'refusal') {
    throw new Error('The model declined this request');
  }
  return message.content.map(block => (block.type === 'text' ? block.text : '')).join('');
}
```

</TabItem>
<TabItem value="openai" label="OpenAI">

This uses the [OpenAI TypeScript SDK](https://github.com/openai/openai-node) and its Responses API. `output_text` joins all the text the model generated.

```typescript
import OpenAI from 'openai';

// Reads the API key from the OPENAI_API_KEY environment variable.
const openai = new OpenAI();

async function callModel(prompt: string): Promise<string> {
  const response = await openai.responses.create({
    model: 'gpt-6-astra',
    input: prompt,
    max_output_tokens: 1024,
  });
  return response.output_text;
}
```

</TabItem>
<TabItem value="google" label="Google">

This uses the [Google Gen AI SDK](https://github.com/googleapis/js-genai). `text` joins all the text the model generated.

```typescript
import { GoogleGenAI } from '@google/genai';

// Reads the API key from the GEMINI_API_KEY environment variable.
const google = new GoogleGenAI({});

async function callModel(prompt: string): Promise<string> {
  const response = await google.models.generateContent({
    model: 'gemini-3.8-flash',
    contents: prompt,
    config: { maxOutputTokens: 1024 },
  });
  return response.text ?? '';
}
```

</TabItem>
</Tabs>

## Run the sidecar

Start the sidecar with your provider's API key:

<Tabs groupId="ai-provider" queryString>
<TabItem value="anthropic" label="Anthropic">

```bash
ANTHROPIC_API_KEY='your-api-key' npx tsx src/sidecar.ts
```

</TabItem>
<TabItem value="openai" label="OpenAI">

```bash
OPENAI_API_KEY='your-api-key' npx tsx src/sidecar.ts
```

</TabItem>
<TabItem value="google" label="Google">

```bash
GEMINI_API_KEY='your-api-key' npx tsx src/sidecar.ts
```

</TabItem>
</Tabs>

The first time it runs, the sidecar prints its identity, for example `Sidecar connected as c200…`. Add that identity to `ai_worker` with SQL, prefixed with `0x`. Only the database owner and collaborators can write to a table this way:

```bash
spacetime sql my-game "INSERT INTO ai_worker (identity) VALUES (0x{identity})"
```

where _identity_ is the hexadecimal identity the sidecar printed. The sidecar receives the pending requests as soon as the row is added, and keeps the same identity on later runs because it reuses the token it saved in `sidecar-token.txt`. Keep that file private: anyone with the token can act as your sidecar.

Run a single sidecar per database. Two sidecars would both answer the same requests.

## Get replies on the client

On the client, call `request_ai_reply` with the prompt, and subscribe to `my_ai_requests`. When the sidecar completes a request, the client receives an update to its row, with the reply in `text`, or the reason it failed in `error`.

<Tabs groupId="client-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
import { DbConnection, tables } from './module_bindings';
import type { AiRequest } from './module_bindings/types';

const conn = DbConnection.builder()
  .withUri('ws://localhost:3000')
  .withDatabaseName('my-game')
  .onConnect(ctx => {
    ctx.subscriptionBuilder().subscribe([tables.myAiRequests]);
  })
  .build();

// Call this when the player sends a message.
function askModel(prompt: string) {
  conn.reducers.requestAiReply({ prompt });
}

// A request's row is updated when the sidecar completes it.
conn.db.myAiRequests.onUpdate((_ctx, _oldRequest, request) => showReply(request));

function showReply(request: AiRequest) {
  if (!request.done) {
    return;
  }
  if (request.error) {
    console.error('The AI request failed:', request.error);
  } else {
    console.log(request.text);
  }
}
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
using SpacetimeDB;
using SpacetimeDB.Types;

var conn = DbConnection.Builder()
    .WithUri("http://localhost:3000")
    .WithDatabaseName("my-game")
    .OnConnect((conn, identity, token) =>
    {
        conn.SubscriptionBuilder()
            .AddQuery(q => q.From.MyAiRequests())
            .Subscribe();
    })
    .Build();

// Call this when the player sends a message.
void AskModel(string prompt) => conn.Reducers.RequestAiReply(prompt);

// A request's row is updated when the sidecar completes it.
conn.Db.MyAiRequests.OnUpdate += (ctx, oldRequest, request) =>
{
    if (!request.Done)
    {
        return;
    }
    if (request.Error != "")
    {
        Console.WriteLine($"The AI request failed: {request.Error}");
    }
    else
    {
        Console.WriteLine(request.Text);
    }
};
```

Remember to call `conn.FrameTick()` regularly, for example once per frame, so the callbacks run.

</TabItem>
<TabItem value="rust" label="Rust">

```rust
mod module_bindings;
use module_bindings::*;
use spacetimedb_sdk::{DbContext, TableWithPrimaryKey};

fn main() {
    let conn = DbConnection::builder()
        .with_uri("http://localhost:3000")
        .with_database_name("my-game")
        .on_connect(|ctx, _identity, _token| {
            ctx.subscription_builder()
                .add_query(|q| q.from.my_ai_requests())
                .subscribe();
        })
        .build()
        .expect("failed to connect");

    // A request's row is updated when the sidecar completes it.
    conn.db().my_ai_requests().on_update(|_ctx, _old_request, request| {
        if !request.done {
            return;
        }
        if request.error.is_empty() {
            println!("{}", request.text);
        } else {
            eprintln!("The AI request failed: {}", request.error);
        }
    });

    // Process messages from the database on a background thread.
    conn.run_threaded();

    // Your game loop runs here.
}

// Call this when the player sends a message.
fn ask_model(conn: &DbConnection, prompt: String) {
    conn.reducers().request_ai_reply(prompt).unwrap();
}
```

</TabItem>
</Tabs>

## Stream the reply

To show the reply while the model is still writing it, have the sidecar store the text so far as it arrives. Add an `update_ai_request_text` reducer that only AI workers can call:

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
export const updateAiRequestText = spacetimedb.reducer(
  { id: t.u64(), text: t.string() },
  (ctx, { id, text }) => {
    if (!ctx.db.aiWorker.identity.find(ctx.sender)) {
      throw new SenderError('Only AI workers can update requests');
    }
    const request = ctx.db.aiRequest.id.find(id);
    if (!request || request.done) {
      throw new SenderError('No such pending request');
    }
    ctx.db.aiRequest.id.update({ ...request, text });
  }
);
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
public static partial class Module
{
    [SpacetimeDB.Reducer]
    public static void UpdateAiRequestText(ReducerContext ctx, ulong id, string text)
    {
        if (ctx.Db.AiWorker.Identity.Find(ctx.Sender) is null)
        {
            throw new Exception("Only AI workers can update requests");
        }
        if (ctx.Db.AiRequest.Id.Find(id) is not AiRequest request || request.Done)
        {
            throw new Exception("No such pending request");
        }
        request.Text = text;
        ctx.Db.AiRequest.Id.Update(request);
    }
}
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
#[reducer]
pub fn update_ai_request_text(ctx: &ReducerContext, id: u64, text: String) -> Result<(), String> {
    require_worker(ctx)?;
    let request = ctx.db.ai_request().id().find(id).ok_or("No such request")?;
    if request.done {
        return Err("This request is already complete".to_string());
    }
    ctx.db.ai_request().id().update(AiRequest { text, ..request });
    Ok(())
}
```

</TabItem>
</Tabs>

In the sidecar, give `callModel` an `onText` callback that receives the text generated so far, and use your provider's streaming API:

<Tabs groupId="ai-provider" queryString>
<TabItem value="anthropic" label="Anthropic">

```typescript
async function callModel(prompt: string, onText: (textSoFar: string) => void): Promise<string> {
  const stream = anthropic.beta.messages.stream({
    model: 'claude-opus-5-5',
    max_tokens: 1024,
    output_config: { effort: 'low' },
    betas: ['server-side-fallback-2026-07-01'],
    fallbacks: 'default',
    messages: [{ role: 'user', content: prompt }],
  });
  let text = '';
  for await (const event of stream) {
    if (event.type === 'content_block_delta' && event.delta.type === 'text_delta') {
      text += event.delta.text;
      onText(text);
    }
  }
  const message = await stream.finalMessage();
  if (message.stop_reason === 'refusal') {
    throw new Error('The model declined this request');
  }
  return text;
}
```

</TabItem>
<TabItem value="openai" label="OpenAI">

```typescript
async function callModel(prompt: string, onText: (textSoFar: string) => void): Promise<string> {
  const stream = await openai.responses.create({
    model: 'gpt-6-astra',
    input: prompt,
    max_output_tokens: 1024,
    stream: true,
  });
  let text = '';
  for await (const event of stream) {
    if (event.type === 'response.output_text.delta') {
      text += event.delta;
      onText(text);
    }
  }
  return text;
}
```

</TabItem>
<TabItem value="google" label="Google">

```typescript
async function callModel(prompt: string, onText: (textSoFar: string) => void): Promise<string> {
  const stream = await google.models.generateContentStream({
    model: 'gemini-3.8-flash',
    contents: prompt,
    config: { maxOutputTokens: 1024 },
  });
  let text = '';
  for await (const chunk of stream) {
    text += chunk.text ?? '';
    onText(text);
  }
  return text;
}
```

</TabItem>
</Tabs>

Then, in `handleRequest`, pass a callback that sends the text to the database. Every call to `update_ai_request_text` is a transaction that updates the player's view, so send at most a few updates per second rather than one per word:

```typescript
let lastUpdate = 0;
const text = await callModel(request.prompt, textSoFar => {
  // Send at most 4 updates per second.
  if (Date.now() - lastUpdate >= 250) {
    lastUpdate = Date.now();
    conn.reducers.updateAiRequestText({ id: request.id, text: textSoFar });
  }
});
```

On the client, show `request.text` in the update callback even while `done` is false: it holds the reply so far.

## What's next?

You now have a sidecar that answers players' AI requests:

- Players add requests with `request_ai_reply` and read their replies through the `my_ai_requests` view.
- The sidecar reads the queue through the `pending_ai_requests` view, calls the model with your provider's SDK, and stores replies with `complete_ai_request`.
- The `ai_worker` table makes sure only your sidecar can read the queue and answer requests, and your API key stays with the sidecar.

To learn more about the features used here, see [Views](../../00200-core-concepts/00200-functions/00500-views.md), [Reducers](../../00200-core-concepts/00200-functions/00300-reducers/00300-reducers.md) and the [TypeScript client SDK](../../00200-core-concepts/00600-clients/00700-typescript-reference.md).

To call the model from your module instead, without running a separate program, see [Call an AI model from your module](./00550-call-ai-model.md).

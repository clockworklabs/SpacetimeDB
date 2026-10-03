---
title: Call an AI model
slug: /guides/app-patterns/call-ai-model
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';

:::note Prerequisites
You need a module and a client connected to it, and an API key from the provider of the AI model you want to use. If you don't have a module yet, follow the [quickstart](../../00100-intro/00100-getting-started/00100-getting-started.md) for your language first.
:::

In this guide, you'll let clients ask an AI model, such as a large language model (LLM), to generate text, for example a reply from a character in your game. The request goes through your module, so your provider API key never reaches the client. You'll use a [procedure](../../00200-core-concepts/00200-functions/00400-procedures.md) to make the HTTP request, and private tables to store the API key and to limit how often each player can call the model.

The examples cover Anthropic, OpenAI and Google. Other providers work the same way: only the request and the way you read the reply change.

## How it works

- A private `ai_config` table stores your provider API key on the database.
- A `call_model` function sends the prompt to your provider and returns the generated text. It's the only provider-specific code.
- A `generate_text` procedure checks the prompt and the caller's cooldown, calls `call_model`, and returns the text, or an error message, to the client.
- A private `model_usage` table records when each player last called the model.

The model is called from a procedure, not a reducer. Reducers can't make HTTP requests, because they run inside a transaction. A procedure can, and it doesn't hold a transaction open while it waits for the model, which can take several seconds.

## Store the API key

Add a private `ai_config` table with a single row that holds your API key. The code below also defines the private `model_usage` table, which you'll use to limit requests. Clients can't read private tables.

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
import { TimeDuration } from 'spacetimedb';
import { schema, table, t, type InferSchema, type ProcedureCtx } from 'spacetimedb/server';

const aiConfig = table(
  { name: 'ai_config' },
  {
    id: t.u8().primaryKey(),
    apiKey: t.string(),
  }
);

const modelUsage = table(
  { name: 'model_usage' },
  {
    identity: t.identity().primaryKey(),
    lastRequest: t.timestamp(),
  }
);

const spacetimedb = schema({ aiConfig, modelUsage });
export default spacetimedb;

type Ctx = ProcedureCtx<InferSchema<typeof spacetimedb>>;
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
#pragma warning disable STDB_UNSTABLE
using System.Text.Json.Nodes;
using SpacetimeDB;

public static partial class Module
{
    [SpacetimeDB.Table(Accessor = "AiConfig")]
    public partial struct AiConfig
    {
        [SpacetimeDB.PrimaryKey]
        public byte Id;
        public string ApiKey;
    }

    [SpacetimeDB.Table(Accessor = "ModelUsage")]
    public partial struct ModelUsage
    {
        [SpacetimeDB.PrimaryKey]
        public Identity Identity;
        public Timestamp LastRequest;
    }
}
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
use spacetimedb::http::{Request, Timeout};
use spacetimedb::{procedure, table, Identity, ProcedureContext, Table, Timestamp};
use std::time::Duration;

#[table(accessor = ai_config)]
pub struct AiConfig {
    #[primary_key]
    id: u8,
    api_key: String,
}

#[table(accessor = model_usage)]
pub struct ModelUsage {
    #[primary_key]
    identity: Identity,
    last_request: Timestamp,
}
```

Add `serde_json` to your module's `Cargo.toml` to build and read JSON:

```toml
[dependencies]
serde_json = "1"
```

</TabItem>
</Tabs>

After you publish your module, insert the key with SQL. Only the database owner and collaborators can write to a table this way:

```bash
spacetime sql my-game "INSERT INTO ai_config (id, api_key) VALUES (0, 'your-api-key')"
```

To replace the key later, update the row:

```bash
spacetime sql my-game "UPDATE ai_config SET api_key = 'your-new-api-key' WHERE id = 0"
```

The key stays in the database across publishes, but publishing with `--delete-data` erases it along with your other data.

:::warning
Keep the key away from anything clients can read: don't make `ai_config` public, and don't return the key from a procedure or a view, or write it to the logs.
:::

## Call the model

Add a `call_model` function for your provider. It builds the request, sends it with `http.send` (or `http.fetch` in TypeScript), and returns the generated text. Each provider puts the text in a different place in its reply, and a reply can contain parts that aren't text, so the function keeps only the text.

The model names below are examples. Providers release new models often, so check your provider's documentation for the model you want.

<Tabs groupId="ai-provider" queryString>
<TabItem value="anthropic" label="Anthropic">

This calls Anthropic's [Messages API](https://platform.claude.com/docs/en/api/messages). `fallbacks` asks Anthropic to retry on another model if the model declines the request; a request that is still declined comes back with a `refusal` stop reason.

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
function callModel(ctx: Ctx, apiKey: string, prompt: string): string {
  const response = ctx.http.fetch('https://api.anthropic.com/v1/messages', {
    method: 'POST',
    headers: {
      'content-type': 'application/json',
      'x-api-key': apiKey,
      'anthropic-version': '2023-06-01',
      'anthropic-beta': 'server-side-fallback-2026-07-01',
    },
    body: JSON.stringify({
      model: 'claude-opus-5-5',
      max_tokens: 1024,
      output_config: { effort: 'low' },
      fallbacks: 'default',
      messages: [{ role: 'user', content: prompt }],
    }),
    timeout: TimeDuration.fromMillis(60_000),
  });
  if (response.status !== 200) {
    throw new Error(`The model API returned status ${response.status}`);
  }

  const reply = response.json();
  if (reply.stop_reason === 'refusal') {
    throw new Error('The model declined this request');
  }
  const blocks: { type: string; text?: string }[] = reply.content;
  return blocks
    .filter(block => block.type === 'text')
    .map(block => block.text ?? '')
    .join('');
}
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
public static partial class Module
{
    static string CallModel(ProcedureContext ctx, string apiKey, string prompt)
    {
        var body = new JsonObject
        {
            ["model"] = "claude-opus-5-5",
            ["max_tokens"] = 1024,
            ["output_config"] = new JsonObject { ["effort"] = "low" },
            ["fallbacks"] = "default",
            ["messages"] = new JsonArray(new JsonObject { ["role"] = "user", ["content"] = prompt }),
        };
        var request = new HttpRequest
        {
            Method = SpacetimeDB.HttpMethod.Post,
            Uri = "https://api.anthropic.com/v1/messages",
            Headers = new List<HttpHeader>
            {
                new HttpHeader("content-type", "application/json"),
                new HttpHeader("x-api-key", apiKey),
                new HttpHeader("anthropic-version", "2023-06-01"),
                new HttpHeader("anthropic-beta", "server-side-fallback-2026-07-01"),
            },
            Body = HttpBody.FromString(body.ToJsonString()),
            Timeout = TimeSpan.FromSeconds(60),
        };
        var response = ctx.Http.Send(request).UnwrapOrThrow();
        if (response.StatusCode != 200)
        {
            throw new Exception($"The model API returned status {response.StatusCode}");
        }

        var reply = JsonNode.Parse(response.Body.ToStringUtf8Lossy())!;
        if ((string?)reply["stop_reason"] == "refusal")
        {
            throw new Exception("The model declined this request");
        }
        return string.Concat(reply["content"]!.AsArray()
            .Where(block => (string?)block!["type"] == "text")
            .Select(block => (string?)block!["text"]));
    }
}
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
fn call_model(ctx: &mut ProcedureContext, api_key: &str, prompt: &str) -> Result<String, String> {
    let body = serde_json::json!({
        "model": "claude-opus-5-5",
        "max_tokens": 1024,
        "output_config": { "effort": "low" },
        "fallbacks": "default",
        "messages": [{ "role": "user", "content": prompt }],
    });
    let request = Request::builder()
        .uri("https://api.anthropic.com/v1/messages")
        .method("POST")
        .header("content-type", "application/json")
        .header("x-api-key", api_key)
        .header("anthropic-version", "2023-06-01")
        .header("anthropic-beta", "server-side-fallback-2026-07-01")
        .extension(Timeout(Duration::from_secs(60).into()))
        .body(body.to_string())
        .map_err(|e| e.to_string())?;
    let response = ctx.http.send(request).map_err(|e| format!("Request failed: {e:?}"))?;
    let (parts, body) = response.into_parts();
    if parts.status != 200 {
        return Err(format!("The model API returned status {}", parts.status));
    }

    let reply: serde_json::Value = serde_json::from_slice(&body.into_bytes()).map_err(|e| e.to_string())?;
    if reply["stop_reason"] == "refusal" {
        return Err("The model declined this request".to_string());
    }
    let text = reply["content"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|block| block["type"] == "text")
        .filter_map(|block| block["text"].as_str())
        .collect();
    Ok(text)
}
```

</TabItem>
</Tabs>

</TabItem>
<TabItem value="openai" label="OpenAI">

This calls OpenAI's [Responses API](https://developers.openai.com/api/docs/guides/text). The text is in the `output_text` parts of the `message` items in the reply's `output`.

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
function callModel(ctx: Ctx, apiKey: string, prompt: string): string {
  const response = ctx.http.fetch('https://api.openai.com/v1/responses', {
    method: 'POST',
    headers: {
      'content-type': 'application/json',
      authorization: `Bearer ${apiKey}`,
    },
    body: JSON.stringify({
      model: 'gpt-6-astra',
      input: prompt,
      max_output_tokens: 1024,
    }),
    timeout: TimeDuration.fromMillis(60_000),
  });
  if (response.status !== 200) {
    throw new Error(`The model API returned status ${response.status}`);
  }

  const reply = response.json();
  const items: { type: string; content?: { type: string; text?: string }[] }[] = reply.output;
  return items
    .filter(item => item.type === 'message')
    .flatMap(item => item.content ?? [])
    .filter(part => part.type === 'output_text')
    .map(part => part.text ?? '')
    .join('');
}
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
public static partial class Module
{
    static string CallModel(ProcedureContext ctx, string apiKey, string prompt)
    {
        var body = new JsonObject
        {
            ["model"] = "gpt-6-astra",
            ["input"] = prompt,
            ["max_output_tokens"] = 1024,
        };
        var request = new HttpRequest
        {
            Method = SpacetimeDB.HttpMethod.Post,
            Uri = "https://api.openai.com/v1/responses",
            Headers = new List<HttpHeader>
            {
                new HttpHeader("content-type", "application/json"),
                new HttpHeader("authorization", $"Bearer {apiKey}"),
            },
            Body = HttpBody.FromString(body.ToJsonString()),
            Timeout = TimeSpan.FromSeconds(60),
        };
        var response = ctx.Http.Send(request).UnwrapOrThrow();
        if (response.StatusCode != 200)
        {
            throw new Exception($"The model API returned status {response.StatusCode}");
        }

        var reply = JsonNode.Parse(response.Body.ToStringUtf8Lossy())!;
        return string.Concat(reply["output"]!.AsArray()
            .Where(item => (string?)item!["type"] == "message")
            .SelectMany(item => item!["content"]!.AsArray())
            .Where(part => (string?)part!["type"] == "output_text")
            .Select(part => (string?)part!["text"]));
    }
}
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
fn call_model(ctx: &mut ProcedureContext, api_key: &str, prompt: &str) -> Result<String, String> {
    let body = serde_json::json!({
        "model": "gpt-6-astra",
        "input": prompt,
        "max_output_tokens": 1024,
    });
    let request = Request::builder()
        .uri("https://api.openai.com/v1/responses")
        .method("POST")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {api_key}"))
        .extension(Timeout(Duration::from_secs(60).into()))
        .body(body.to_string())
        .map_err(|e| e.to_string())?;
    let response = ctx.http.send(request).map_err(|e| format!("Request failed: {e:?}"))?;
    let (parts, body) = response.into_parts();
    if parts.status != 200 {
        return Err(format!("The model API returned status {}", parts.status));
    }

    let reply: serde_json::Value = serde_json::from_slice(&body.into_bytes()).map_err(|e| e.to_string())?;
    let text = reply["output"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|item| item["type"] == "message")
        .flat_map(|item| item["content"].as_array().into_iter().flatten())
        .filter(|part| part["type"] == "output_text")
        .filter_map(|part| part["text"].as_str())
        .collect();
    Ok(text)
}
```

</TabItem>
</Tabs>

</TabItem>
<TabItem value="google" label="Google">

This calls the Gemini API's [`generateContent` method](https://ai.google.dev/api/generate-content). The model name is part of the URL, and the text is in the `parts` of the first candidate.

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
function callModel(ctx: Ctx, apiKey: string, prompt: string): string {
  const model = 'gemini-3.8-flash';
  const response = ctx.http.fetch(
    `https://generativelanguage.googleapis.com/v1beta/models/${model}:generateContent`,
    {
      method: 'POST',
      headers: {
        'content-type': 'application/json',
        'x-goog-api-key': apiKey,
      },
      body: JSON.stringify({
        contents: [{ parts: [{ text: prompt }] }],
        generationConfig: { maxOutputTokens: 1024 },
      }),
      timeout: TimeDuration.fromMillis(60_000),
    }
  );
  if (response.status !== 200) {
    throw new Error(`The model API returned status ${response.status}`);
  }

  const reply = response.json();
  const parts: { text?: string }[] = reply.candidates?.[0]?.content?.parts ?? [];
  return parts.map(part => part.text ?? '').join('');
}
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
public static partial class Module
{
    static string CallModel(ProcedureContext ctx, string apiKey, string prompt)
    {
        var model = "gemini-3.8-flash";
        var body = new JsonObject
        {
            ["contents"] = new JsonArray(new JsonObject
            {
                ["parts"] = new JsonArray(new JsonObject { ["text"] = prompt }),
            }),
            ["generationConfig"] = new JsonObject { ["maxOutputTokens"] = 1024 },
        };
        var request = new HttpRequest
        {
            Method = SpacetimeDB.HttpMethod.Post,
            Uri = $"https://generativelanguage.googleapis.com/v1beta/models/{model}:generateContent",
            Headers = new List<HttpHeader>
            {
                new HttpHeader("content-type", "application/json"),
                new HttpHeader("x-goog-api-key", apiKey),
            },
            Body = HttpBody.FromString(body.ToJsonString()),
            Timeout = TimeSpan.FromSeconds(60),
        };
        var response = ctx.Http.Send(request).UnwrapOrThrow();
        if (response.StatusCode != 200)
        {
            throw new Exception($"The model API returned status {response.StatusCode}");
        }

        var reply = JsonNode.Parse(response.Body.ToStringUtf8Lossy())!;
        var parts = reply["candidates"]?[0]?["content"]?["parts"]?.AsArray() ?? new JsonArray();
        return string.Concat(parts.Select(part => (string?)part?["text"]));
    }
}
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
fn call_model(ctx: &mut ProcedureContext, api_key: &str, prompt: &str) -> Result<String, String> {
    let model = "gemini-3.8-flash";
    let body = serde_json::json!({
        "contents": [{ "parts": [{ "text": prompt }] }],
        "generationConfig": { "maxOutputTokens": 1024 },
    });
    let request = Request::builder()
        .uri(format!("https://generativelanguage.googleapis.com/v1beta/models/{model}:generateContent"))
        .method("POST")
        .header("content-type", "application/json")
        .header("x-goog-api-key", api_key)
        .extension(Timeout(Duration::from_secs(60).into()))
        .body(body.to_string())
        .map_err(|e| e.to_string())?;
    let response = ctx.http.send(request).map_err(|e| format!("Request failed: {e:?}"))?;
    let (parts, body) = response.into_parts();
    if parts.status != 200 {
        return Err(format!("The model API returned status {}", parts.status));
    }

    let reply: serde_json::Value = serde_json::from_slice(&body.into_bytes()).map_err(|e| e.to_string())?;
    let text = reply["candidates"][0]["content"]["parts"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|part| part["text"].as_str())
        .collect();
    Ok(text)
}
```

</TabItem>
</Tabs>

</TabItem>
</Tabs>

## Generate text for a client

Add a `generate_text` procedure that clients call with their prompt. Every call costs you money, so the procedure first rejects prompts that are empty or too long, and players who called it less than 5 seconds ago. It reads the API key and checks the cooldown in short transactions, then calls `call_model` outside of them.

The procedure returns a `GeneratedText` value with two fields: `text` holds the generated text, and `error` explains what went wrong, or is empty on success. Returning errors this way, instead of throwing, means clients get the same kind of result whichever language your module is written in.

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
const MAX_PROMPT_LENGTH = 2000;
const COOLDOWN_MICROS = 5_000_000n; // 5 seconds

const generatedText = t.object('GeneratedText', {
  text: t.string(),
  error: t.string(),
});

export const generateText = spacetimedb.procedure(
  { prompt: t.string() },
  generatedText,
  (ctx, { prompt }) => {
    try {
      return { text: tryGenerateText(ctx, prompt), error: '' };
    } catch (error) {
      return { text: '', error: error instanceof Error ? error.message : String(error) };
    }
  }
);

function tryGenerateText(ctx: Ctx, prompt: string): string {
  if (prompt.length === 0 || prompt.length > MAX_PROMPT_LENGTH) {
    throw new Error(`The prompt must be 1 to ${MAX_PROMPT_LENGTH} characters long`);
  }

  // Read the API key from its private table.
  const apiKey = ctx.withTx(tx => tx.db.aiConfig.id.find(0)?.apiKey);
  if (!apiKey) {
    throw new Error('The AI API key is not set');
  }

  // Check and record the caller's cooldown in a short transaction.
  const allowed = ctx.withTx(tx => {
    const usage = tx.db.modelUsage.identity.find(tx.sender);
    if (usage && tx.timestamp.since(usage.lastRequest).micros < COOLDOWN_MICROS) {
      return false;
    }
    if (usage) {
      tx.db.modelUsage.identity.update({ ...usage, lastRequest: tx.timestamp });
    } else {
      tx.db.modelUsage.insert({ identity: tx.sender, lastRequest: tx.timestamp });
    }
    return true;
  });
  if (!allowed) {
    throw new Error('Too many requests, try again in a few seconds');
  }

  // Call the model outside the transaction.
  const text = callModel(ctx, apiKey, prompt);
  if (text.length === 0) {
    throw new Error('The model returned no text');
  }
  return text;
}
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
public static partial class Module
{
    const int MaxPromptLength = 2000;
    static readonly TimeSpan Cooldown = TimeSpan.FromSeconds(5);

    [SpacetimeDB.Type]
    public partial struct GeneratedText
    {
        public string Text;
        public string Error;
    }

    [SpacetimeDB.Procedure]
    public static GeneratedText GenerateText(ProcedureContext ctx, string prompt)
    {
        try
        {
            return new GeneratedText { Text = TryGenerateText(ctx, prompt), Error = "" };
        }
        catch (Exception error)
        {
            return new GeneratedText { Text = "", Error = error.Message };
        }
    }

    static string TryGenerateText(ProcedureContext ctx, string prompt)
    {
        if (prompt.Length == 0 || prompt.Length > MaxPromptLength)
        {
            throw new Exception($"The prompt must be 1 to {MaxPromptLength} characters long");
        }

        // Read the API key from its private table.
        var apiKey = ctx.WithTx(tx => tx.Db.AiConfig.Id.Find(0)?.ApiKey);
        if (string.IsNullOrEmpty(apiKey))
        {
            throw new Exception("The AI API key is not set");
        }

        // Check and record the caller's cooldown in a short transaction.
        var allowed = ctx.WithTx(tx =>
        {
            if (tx.Db.ModelUsage.Identity.Find(tx.Sender) is ModelUsage usage)
            {
                if ((TimeSpan)tx.Timestamp.TimeDurationSince(usage.LastRequest) < Cooldown)
                {
                    return false;
                }
                usage.LastRequest = tx.Timestamp;
                tx.Db.ModelUsage.Identity.Update(usage);
            }
            else
            {
                tx.Db.ModelUsage.Insert(new ModelUsage { Identity = tx.Sender, LastRequest = tx.Timestamp });
            }
            return true;
        });
        if (!allowed)
        {
            throw new Exception("Too many requests, try again in a few seconds");
        }

        // Call the model outside the transaction.
        var text = CallModel(ctx, apiKey, prompt);
        if (text.Length == 0)
        {
            throw new Exception("The model returned no text");
        }
        return text;
    }
}
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
const MAX_PROMPT_LENGTH: usize = 2000;
const COOLDOWN: Duration = Duration::from_secs(5);

#[derive(spacetimedb::SpacetimeType)]
pub struct GeneratedText {
    pub text: String,
    pub error: String,
}

#[procedure]
pub fn generate_text(ctx: &mut ProcedureContext, prompt: String) -> GeneratedText {
    match try_generate_text(ctx, &prompt) {
        Ok(text) => GeneratedText { text, error: String::new() },
        Err(error) => GeneratedText { text: String::new(), error },
    }
}

fn try_generate_text(ctx: &mut ProcedureContext, prompt: &str) -> Result<String, String> {
    if prompt.is_empty() || prompt.chars().count() > MAX_PROMPT_LENGTH {
        return Err(format!("The prompt must be 1 to {MAX_PROMPT_LENGTH} characters long"));
    }

    // Read the API key from its private table.
    let api_key = ctx
        .with_tx(|tx| tx.db.ai_config().id().find(0).map(|config| config.api_key))
        .ok_or("The AI API key is not set")?;

    // Check and record the caller's cooldown in a short transaction.
    let allowed = ctx.with_tx(|tx| {
        let usage = tx.db.model_usage().identity().find(tx.sender());
        if let Some(usage) = usage {
            let elapsed = tx.timestamp.duration_since(usage.last_request).unwrap_or_default();
            if elapsed < COOLDOWN {
                return false;
            }
            tx.db.model_usage().identity().update(ModelUsage {
                last_request: tx.timestamp,
                ..usage
            });
        } else {
            tx.db.model_usage().insert(ModelUsage {
                identity: tx.sender(),
                last_request: tx.timestamp,
            });
        }
        true
    });
    if !allowed {
        return Err("Too many requests, try again in a few seconds".to_string());
    }

    // Call the model outside the transaction.
    let text = call_model(ctx, &api_key, prompt)?;
    if text.is_empty() {
        return Err("The model returned no text".to_string());
    }
    Ok(text)
}
```

</TabItem>
</Tabs>

The cooldown limits each player, not your total spending. Most providers also let you set a spending limit on your account, which is worth doing before you release your game.

## Call the procedure from a client

On the client, call `generate_text` with the prompt. The call completes once the model has replied, or once the procedure has rejected the request. Check `error` to tell the two apart.

<Tabs groupId="client-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
import { DbConnection } from './module_bindings';

const conn = DbConnection.builder()
  .withUri('ws://localhost:3000')
  .withDatabaseName('my-game')
  .build();

// Call this when the player sends a message.
async function askModel(prompt: string) {
  const result = await conn.procedures.generateText({ prompt });
  if (result.error) {
    console.error('Text generation failed:', result.error);
  } else {
    console.log(result.text);
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
    .Build();

// Call this when the player sends a message.
void AskModel(string prompt)
{
    conn.Procedures.GenerateText(prompt, (ctx, result) =>
    {
        if (!result.IsSuccess || result.Value is not GeneratedText generated)
        {
            Console.WriteLine($"Could not call the procedure: {result.Error}");
        }
        else if (generated.Error != "")
        {
            Console.WriteLine($"Text generation failed: {generated.Error}");
        }
        else
        {
            Console.WriteLine(generated.Text);
        }
    });
}
```

Remember to call `conn.FrameTick()` regularly, for example once per frame, so the callback runs.

</TabItem>
<TabItem value="rust" label="Rust">

```rust
mod module_bindings;
use module_bindings::*;

fn main() {
    let conn = DbConnection::builder()
        .with_uri("http://localhost:3000")
        .with_database_name("my-game")
        .build()
        .expect("failed to connect");

    // Process messages from the database on a background thread.
    conn.run_threaded();

    // Your game loop runs here.
}

// Call this when the player sends a message.
fn ask_model(conn: &DbConnection, prompt: String) {
    conn.procedures.generate_text_then(prompt, |_ctx, result| match result {
        Ok(generated) if generated.error.is_empty() => println!("{}", generated.text),
        Ok(generated) => eprintln!("Text generation failed: {}", generated.error),
        Err(error) => eprintln!("Could not call the procedure: {error:?}"),
    });
}
```

</TabItem>
</Tabs>

## What's next?

You now have a module that generates text with an AI model on behalf of its clients:

- A `call_model` function holds the only provider-specific code, so switching providers means replacing one function.
- The API key lives in a private table on the database, never on the client.
- The `generate_text` procedure rejects oversized prompts and players who call it too often, calls the model outside any transaction, and returns errors as values that every client can read.

To learn more about the features used here, see [Procedures](../../00200-core-concepts/00200-functions/00400-procedures.md) and [Access Permissions](../../00200-core-concepts/00300-tables/00400-access-permissions.md).

To call other web services the same way, see [Call an external API from your module](./00500-call-external-api.md).

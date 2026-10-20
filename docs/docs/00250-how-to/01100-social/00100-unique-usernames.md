---
title: Keep usernames unique and block offensive ones
slug: /how-to/social/unique-usernames
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';

:::note Prerequisites
You need a module and a client connected to it, and an [OpenAI API key](https://platform.openai.com/api-keys). If you don't have a module yet, follow the [quickstart](../../00100-intro/00100-getting-started/00100-getting-started.md) for your language first.
:::

In this guide, you'll let players pick a username that no one else has, ignoring case, and reject names that are offensive. You'll use a [unique column](../../00200-core-concepts/00300-tables/00240-constraints.md#unique-columns) to keep names unique, a profanity library to catch swear words, and OpenAI's [moderation API](https://developers.openai.com/api/docs/guides/moderation) to catch hateful or harassing names. The API is called from a [procedure](../../00200-core-concepts/00200-functions/00400-procedures.md), with an API key stored in an [environment variable](../../00200-core-concepts/00100-databases/00700-environment-variables.md).

## How it works

- A public `player` table holds each player's `name`, as they typed it, and its lowercase form, `name_key`. `name_key` is unique, so "Bob" and "bob" can't both exist. A player only gets a row once they've picked a valid name, so the table also tells you who's ready to play.
- A `set_username` procedure checks a name in three steps, cheapest first:
  1. The name must be 3 to 20 letters, digits or underscores.
  2. A profanity library rejects swear words and, depending on the library, disguised spellings that use digits in place of letters. It runs inside your module, so it's instant and free.
  3. The OpenAI moderation API rejects names that its model judges hateful, harassing, sexual or violent.
- A private `name_attempt` table records when each player last asked for a name, so a player can't flood the moderation API with requests.
- An `OPENAI_API_KEY` environment variable stores your API key on the database.

The two content checks cover different things. A word list only knows the words on it, but it catches swearing, which isn't one of the moderation API's categories. The moderation API judges what a name means, so it can reject a hateful or threatening name that uses no swear words at all.

`set_username` is a procedure, not a reducer, because reducers can't make HTTP requests. The procedure calls the moderation API outside of any transaction, and only opens short transactions to read and write tables.

## Install a profanity library

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

Add the [`obscenity`](https://www.npmjs.com/package/obscenity) package to your module. It recognizes common disguises, such as digits in place of letters and repeated letters:

```bash
npm install obscenity
```

</TabItem>
<TabItem value="csharp" label="C#">

Add the [`mk.profanity`](https://www.nuget.org/packages/mk.profanity) package to your module. It matches whole words, so it doesn't reject innocent names that happen to contain a swear word, like `classic`. But it misses disguises such as digits in place of letters, and swear words glued to other text, like a number right after the word:

```bash
dotnet add package mk.profanity
```

</TabItem>
<TabItem value="rust" label="Rust">

Add the [`rustrict`](https://crates.io/crates/rustrict) crate to your module. It recognizes common disguises, such as digits in place of letters and spaces between letters. Its default features include chat tools this guide doesn't need, so turn them off. Also add `serde_json` to build and read JSON:

```bash
cargo add rustrict --no-default-features --features censor
cargo add serde_json
```

</TabItem>
</Tabs>

## Define the tables

Add the `player` and `name_attempt` tables, and declare the `OPENAI_API_KEY` environment variable. Clients can read `player` to show everyone's names, but they can't read `name_attempt`, which is private.

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
import { TimeDuration } from 'spacetimedb';
import {
  schema,
  table,
  t,
  SenderError,
  type InferSchema,
  type ProcedureCtx,
  type ReducerCtx,
} from 'spacetimedb/server';
import {
  DataSet,
  RegExpMatcher,
  englishDataset,
  englishRecommendedTransformers,
  pattern,
} from 'obscenity';

const player = table(
  { name: 'player', public: true },
  {
    identity: t.identity().primaryKey(),
    name: t.string(),
    nameKey: t.string().unique(),
  }
);

const nameAttempt = table(
  { name: 'name_attempt' },
  {
    identity: t.identity().primaryKey(),
    lastAttempt: t.timestamp(),
  }
);

const spacetimedb = schema(
  { player, nameAttempt },
  { env: { OPENAI_API_KEY: t.string() } }
);
export default spacetimedb;

type Ctx = ProcedureCtx<InferSchema<typeof spacetimedb>>;
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
#pragma warning disable STDB_UNSTABLE
using System.Text.Json.Nodes;
using System.Text.RegularExpressions;
using SpacetimeDB;

[SpacetimeDB.Env]
public partial struct ModuleEnv
{
    public string OPENAI_API_KEY;
}

public static partial class Module
{
    [SpacetimeDB.Table(Accessor = "Player", Public = true)]
    public partial struct Player
    {
        [SpacetimeDB.PrimaryKey]
        public Identity Identity;
        public string Name;
        [SpacetimeDB.Unique]
        public string NameKey;
    }

    [SpacetimeDB.Table(Accessor = "NameAttempt")]
    public partial struct NameAttempt
    {
        [SpacetimeDB.PrimaryKey]
        public Identity Identity;
        public Timestamp LastAttempt;
    }
}
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
use rustrict::{Censor, Trie, Type};
use spacetimedb::http::{Request, Timeout};
use spacetimedb::{procedure, reducer, table, Identity, ProcedureContext, ReducerContext, Table, Timestamp};
use std::sync::LazyLock;
use std::time::Duration;

#[spacetimedb::env]
pub struct Env {
    pub OPENAI_API_KEY: String,
}

#[table(accessor = player, public)]
pub struct Player {
    #[primary_key]
    identity: Identity,
    name: String,
    #[unique]
    name_key: String,
}

#[table(accessor = name_attempt)]
pub struct NameAttempt {
    #[primary_key]
    identity: Identity,
    last_attempt: Timestamp,
}
```

</TabItem>
</Tabs>

The API key is required, so SpacetimeDB refuses to publish the module without one. Pass it in the publishing command's environment the first time you publish:

```bash
OPENAI_API_KEY='your-api-key' spacetime publish my-game
```

The key stays in the database across later publishes. To replace it, run the same command with `--env-only`, which updates environment variables without uploading your module. See [Environment Variables](../../00200-core-concepts/00100-databases/00700-environment-variables.md#supply-values-when-publishing) for other ways to supply it.

:::warning
Keep the key away from anything clients can read: don't put it in a `spacetime.json` file you commit, and don't return it from a procedure or a view, or write it to the logs.
:::

## Check the name's format

Add a `name_key` function that rejects names that are too short, too long, or that contain anything other than ASCII letters, digits and underscores. It returns the name's key: its lowercase form, which the `player` table keeps unique.

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
const MIN_NAME_LENGTH = 3;
const MAX_NAME_LENGTH = 20;
const NAME_PATTERN = new RegExp(`^[A-Za-z0-9_]{${MIN_NAME_LENGTH},${MAX_NAME_LENGTH}}$`);

// Checks the name's format and returns its key: the lowercase form that must be unique.
function nameKey(name: string): string {
  if (!NAME_PATTERN.test(name)) {
    throw new Error(
      `Names must be ${MIN_NAME_LENGTH} to ${MAX_NAME_LENGTH} letters, digits or underscores`
    );
  }
  return name.toLowerCase();
}
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
public static partial class Module
{
    const int MinNameLength = 3;
    const int MaxNameLength = 20;
    static readonly Regex NamePattern = new($@"^[A-Za-z0-9_]{{{MinNameLength},{MaxNameLength}}}\z");

    // Checks the name's format and returns its key: the lowercase form that must be unique.
    static string NameKey(string name)
    {
        if (!NamePattern.IsMatch(name))
        {
            throw new Exception($"Names must be {MinNameLength} to {MaxNameLength} letters, digits or underscores");
        }
        return name.ToLowerInvariant();
    }
}
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
const MIN_NAME_LENGTH: usize = 3;
const MAX_NAME_LENGTH: usize = 20;

/// Checks the name's format and returns its key: the lowercase form that must be unique.
fn name_key(name: &str) -> Result<String, String> {
    let valid_length = (MIN_NAME_LENGTH..=MAX_NAME_LENGTH).contains(&name.len());
    let valid_chars = name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    if !valid_length || !valid_chars {
        return Err(format!(
            "Names must be {MIN_NAME_LENGTH} to {MAX_NAME_LENGTH} letters, digits or underscores"
        ));
    }
    Ok(name.to_ascii_lowercase())
}
```

</TabItem>
</Tabs>

Limiting names to ASCII keeps uniqueness simple. Unicode has many characters that look alike, such as the Latin `a` and the Cyrillic `а`, and characters that don't show at all. If you allowed them, two names could look identical on screen while having different keys. If your players need names in other scripts, look into Unicode normalization and the [confusable detection](https://www.unicode.org/reports/tr39/) described by the Unicode standard before you allow them.

## Filter profanity with a library

Add an `is_profane` function that checks a name against the library's English word list. Every word list blocks some innocent words and misses some offensive ones, so the code also shows how to allow a word that the list blocks, and block a word that it allows. Here, it blocks `moderator`, so players can't pose as your staff.

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
const words = new DataSet<{ originalWord: string }>()
  .addAll(englishDataset)
  // Allow a word that the built-in list blocks: "cumin" is a spice.
  .addPhrase(phrase => phrase.addWhitelistedTerm('cumin'))
  // Block a word that the built-in list allows, so players can't pose as staff.
  .addPhrase(phrase => phrase.setMetadata({ originalWord: 'moderator' }).addPattern(pattern`moderator`));

const matcher = new RegExpMatcher({ ...words.build(), ...englishRecommendedTransformers });

function isProfane(text: string): boolean {
  return matcher.hasMatch(text);
}
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
public static partial class Module
{
    static readonly mk.profanity.ProfanityFilter Words = new(options => options
        // Allow a word that the built-in list blocks: "hell" is fine in a fantasy game.
        .AllowWords(new[] { "hell" })
        // Block a word that the built-in list allows, so players can't pose as staff.
        .AppendBadWords(new[] { "moderator" }));

    static bool IsProfane(string text) => Words.ContainsProfanity(text);
}
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
static WORDS: LazyLock<Trie> = LazyLock::new(|| {
    let mut words = Trie::default();
    // Allow a word that the built-in list blocks: "bobs" reads like a rude word.
    words.set("bobs", Type::NONE);
    // Block a word that the built-in list allows, so players can't pose as staff.
    words.set("moderator", Type::OFFENSIVE & Type::SEVERE);
    words
});

fn is_profane(text: &str) -> bool {
    Censor::from_str(text).with_trie(&WORDS).analyze().is(Type::INAPPROPRIATE)
}
```

</TabItem>
</Tabs>

The word list is part of your module's code, so you change it by publishing your module again.

## Check names with the moderation API

Add an `is_flagged` function that sends a name to OpenAI's moderation endpoint, `POST /v1/moderations`, with the `omni-moderation-latest` model. OpenAI doesn't charge for this endpoint. The reply has one result per input, and its `flagged` field is `true` when the text falls into any harmful category, such as hate, harassment or sexual content.

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
// Asks the OpenAI moderation API whether the text is harmful.
function isFlagged(ctx: Ctx, text: string): boolean {
  const response = ctx.http.fetch('https://api.openai.com/v1/moderations', {
    method: 'POST',
    headers: {
      'content-type': 'application/json',
      authorization: `Bearer ${ctx.env.OPENAI_API_KEY}`,
    },
    body: JSON.stringify({ model: 'omni-moderation-latest', input: text }),
    timeout: TimeDuration.fromMillis(10_000),
  });
  if (response.status !== 200) {
    throw new Error(`The moderation API returned status ${response.status}`);
  }

  const flagged = response.json().results?.[0]?.flagged;
  if (typeof flagged !== 'boolean') {
    throw new Error('The moderation API returned no result');
  }
  return flagged;
}
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
public static partial class Module
{
    // Asks the OpenAI moderation API whether the text is harmful.
    static bool IsFlagged(ProcedureContext ctx, string text)
    {
        var body = new JsonObject
        {
            ["model"] = "omni-moderation-latest",
            ["input"] = text,
        };
        var request = new HttpRequest
        {
            Method = SpacetimeDB.HttpMethod.Post,
            Uri = "https://api.openai.com/v1/moderations",
            Headers = new List<HttpHeader>
            {
                new HttpHeader("content-type", "application/json"),
                new HttpHeader("authorization", $"Bearer {ctx.Env.OPENAI_API_KEY}"),
            },
            Body = HttpBody.FromString(body.ToJsonString()),
            Timeout = TimeSpan.FromSeconds(10),
        };
        var response = ctx.Http.Send(request).UnwrapOrThrow();
        if (response.StatusCode != 200)
        {
            throw new Exception($"The moderation API returned status {response.StatusCode}");
        }

        var reply = JsonNode.Parse(response.Body.ToStringUtf8Lossy())!;
        return (bool?)reply["results"]?[0]?["flagged"]
            ?? throw new Exception("The moderation API returned no result");
    }
}
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
/// Asks the OpenAI moderation API whether the text is harmful.
fn is_flagged(ctx: &mut ProcedureContext, text: &str) -> Result<bool, String> {
    let api_key = ctx.env.OPENAI_API_KEY();
    let body = serde_json::json!({ "model": "omni-moderation-latest", "input": text });
    let request = Request::builder()
        .uri("https://api.openai.com/v1/moderations")
        .method("POST")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {api_key}"))
        .extension(Timeout(Duration::from_secs(10).into()))
        .body(body.to_string())
        .map_err(|e| e.to_string())?;
    let response = ctx.http.send(request).map_err(|e| format!("Request failed: {e:?}"))?;
    let (parts, body) = response.into_parts();
    if parts.status != 200 {
        return Err(format!("The moderation API returned status {}", parts.status));
    }

    let reply: serde_json::Value = serde_json::from_slice(&body.into_bytes()).map_err(|e| e.to_string())?;
    reply["results"][0]["flagged"]
        .as_bool()
        .ok_or_else(|| "The moderation API returned no result".to_string())
}
```

</TabItem>
</Tabs>

`flagged` uses OpenAI's own thresholds. To be stricter or more lenient, read the per-category scores in the result's `category_scores` instead, from 0 to 1. OpenAI may change these scores when it updates the model, so check your thresholds again from time to time.

Other moderation services work the same way: only `is_flagged` changes.

## Set a username

Add the `set_username` procedure that clients call with the name they want. Players call it both to pick their first name and to change it later.

The procedure runs the checks in order of cost, and stops at the first one that fails:

1. It checks the name's format, then runs the word list on the key, with underscores read as spaces so the library sees `bad_word` as two words. The moderation API later gets the same text.
2. In a short transaction, it rejects names that another player already has, then rejects players who asked for a name less than 5 seconds ago. A taken name doesn't count towards the limit, because it never reaches the API.
3. It calls the moderation API, outside of any transaction.
4. In a second transaction, it checks again that the name is free, because another player may have taken it while the API was answering, and saves it.

The procedure returns an empty string on success, or a message that explains what went wrong. Returning errors as values, instead of throwing, means clients get the same kind of result whichever language your module is written in.

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
const COOLDOWN_MICROS = 5_000_000n; // 5 seconds

export const setUsername = spacetimedb.procedure(
  { name: t.string() },
  t.string(),
  (ctx, { name }) => {
    try {
      trySetUsername(ctx, name.trim());
      return '';
    } catch (error) {
      return error instanceof Error ? error.message : String(error);
    }
  }
);

function trySetUsername(ctx: Ctx, name: string) {
  // Check the format and the word list first: they're free.
  const key = nameKey(name);
  const text = key.replaceAll('_', ' ');
  if (isProfane(text)) {
    throw new Error('This name is not allowed');
  }

  // Reject names that are taken, then check and record the caller's cooldown.
  const error = ctx.withTx(tx => {
    const other = tx.db.player.nameKey.find(key);
    if (other && !other.identity.isEqual(tx.sender)) {
      return 'This name is taken';
    }
    const attempt = tx.db.nameAttempt.identity.find(tx.sender);
    if (attempt && tx.timestamp.since(attempt.lastAttempt).micros < COOLDOWN_MICROS) {
      return 'Too many attempts, try again in a few seconds';
    }
    if (attempt) {
      tx.db.nameAttempt.identity.update({ ...attempt, lastAttempt: tx.timestamp });
    } else {
      tx.db.nameAttempt.insert({ identity: tx.sender, lastAttempt: tx.timestamp });
    }
    return '';
  });
  if (error) {
    throw new Error(error);
  }

  // Call the moderation API outside any transaction.
  let flagged: boolean;
  try {
    flagged = isFlagged(ctx, text);
  } catch (error) {
    console.error('Moderation check failed:', error);
    throw new Error('Could not check this name, try again later');
  }
  if (flagged) {
    throw new Error('This name is not allowed');
  }

  // Save the name. Check again: another player may have taken it during the API call.
  const taken = ctx.withTx(tx => {
    const other = tx.db.player.nameKey.find(key);
    if (other && !other.identity.isEqual(tx.sender)) {
      return true;
    }
    const row = { identity: tx.sender, name, nameKey: key };
    if (tx.db.player.identity.find(tx.sender)) {
      tx.db.player.identity.update(row);
    } else {
      tx.db.player.insert(row);
    }
    return false;
  });
  if (taken) {
    throw new Error('This name is taken');
  }
}
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
public static partial class Module
{
    static readonly TimeSpan Cooldown = TimeSpan.FromSeconds(5);

    [SpacetimeDB.Procedure]
    public static string SetUsername(ProcedureContext ctx, string name)
    {
        try
        {
            TrySetUsername(ctx, name.Trim());
            return "";
        }
        catch (Exception error)
        {
            return error.Message;
        }
    }

    static void TrySetUsername(ProcedureContext ctx, string name)
    {
        // Check the format and the word list first: they're free.
        var key = NameKey(name);
        var text = key.Replace('_', ' ');
        if (IsProfane(text))
        {
            throw new Exception("This name is not allowed");
        }

        // Reject names that are taken, then check and record the caller's cooldown.
        var error = ctx.WithTx(tx =>
        {
            if (tx.Db.Player.NameKey.Find(key) is Player other && other.Identity != tx.Sender)
            {
                return "This name is taken";
            }
            if (tx.Db.NameAttempt.Identity.Find(tx.Sender) is NameAttempt attempt)
            {
                if ((TimeSpan)tx.Timestamp.TimeDurationSince(attempt.LastAttempt) < Cooldown)
                {
                    return "Too many attempts, try again in a few seconds";
                }
                attempt.LastAttempt = tx.Timestamp;
                tx.Db.NameAttempt.Identity.Update(attempt);
            }
            else
            {
                tx.Db.NameAttempt.Insert(new NameAttempt { Identity = tx.Sender, LastAttempt = tx.Timestamp });
            }
            return "";
        });
        if (error != "")
        {
            throw new Exception(error);
        }

        // Call the moderation API outside any transaction.
        bool flagged;
        try
        {
            flagged = IsFlagged(ctx, text);
        }
        catch (Exception e)
        {
            Log.Error($"Moderation check failed: {e.Message}");
            throw new Exception("Could not check this name, try again later");
        }
        if (flagged)
        {
            throw new Exception("This name is not allowed");
        }

        // Save the name. Check again: another player may have taken it during the API call.
        var taken = ctx.WithTx(tx =>
        {
            if (tx.Db.Player.NameKey.Find(key) is Player other && other.Identity != tx.Sender)
            {
                return true;
            }
            var player = new Player { Identity = tx.Sender, Name = name, NameKey = key };
            if (tx.Db.Player.Identity.Find(tx.Sender) is not null)
            {
                tx.Db.Player.Identity.Update(player);
            }
            else
            {
                tx.Db.Player.Insert(player);
            }
            return false;
        });
        if (taken)
        {
            throw new Exception("This name is taken");
        }
    }
}
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
const COOLDOWN: Duration = Duration::from_secs(5);

#[procedure]
pub fn set_username(ctx: &mut ProcedureContext, name: String) -> String {
    match try_set_username(ctx, name.trim()) {
        Ok(()) => String::new(),
        Err(error) => error,
    }
}

fn try_set_username(ctx: &mut ProcedureContext, name: &str) -> Result<(), String> {
    // Check the format and the word list first: they're free.
    let key = name_key(name)?;
    let text = key.replace('_', " ");
    if is_profane(&text) {
        return Err("This name is not allowed".to_string());
    }

    // Reject names that are taken, then check and record the caller's cooldown.
    ctx.with_tx(|tx| {
        if let Some(other) = tx.db.player().name_key().find(&key) {
            if other.identity != tx.sender() {
                return Err("This name is taken".to_string());
            }
        }
        let attempt = tx.db.name_attempt().identity().find(tx.sender());
        if let Some(attempt) = attempt {
            let elapsed = tx.timestamp.duration_since(attempt.last_attempt).unwrap_or_default();
            if elapsed < COOLDOWN {
                return Err("Too many attempts, try again in a few seconds".to_string());
            }
            tx.db.name_attempt().identity().update(NameAttempt {
                last_attempt: tx.timestamp,
                ..attempt
            });
        } else {
            tx.db.name_attempt().insert(NameAttempt {
                identity: tx.sender(),
                last_attempt: tx.timestamp,
            });
        }
        Ok(())
    })?;

    // Call the moderation API outside any transaction.
    let flagged = is_flagged(ctx, &text).map_err(|error| {
        log::error!("Moderation check failed: {error}");
        "Could not check this name, try again later".to_string()
    })?;
    if flagged {
        return Err("This name is not allowed".to_string());
    }

    // Save the name. Check again: another player may have taken it during the API call.
    ctx.with_tx(|tx| {
        if let Some(other) = tx.db.player().name_key().find(&key) {
            if other.identity != tx.sender() {
                return Err("This name is taken".to_string());
            }
        }
        let player = Player {
            identity: tx.sender(),
            name: name.to_string(),
            name_key: key.clone(),
        };
        if tx.db.player().identity().find(tx.sender()).is_some() {
            tx.db.player().identity().update(player);
        } else {
            tx.db.player().insert(player);
        }
        Ok(())
    })
}
```

</TabItem>
</Tabs>

When the moderation API can't be reached, or answers with an error, the procedure refuses the name and asks the player to try again later. It logs the details for you, and sends players a generic message. If you'd rather keep accepting names during an OpenAI outage, log the error and continue instead: the name then only passes the word list.

The unique constraint on `name_key` is the final guarantee: even if your code had a bug, the database would never store two rows with the same key.

## Require a name before playing

Players only get a `player` row once their name has passed every check, so a row means the player is ready to play. Add a `require_player` helper that returns the caller's row, or an error if they haven't picked a name yet, and call it at the start of every reducer that needs one. The `join_game` reducer below shows how; in [Build a matchmaking queue](../01000-games/00300-matchmaking-queue.md), `join_queue` is a good place for it.

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
// Returns the caller's player row, or throws if they haven't picked a name yet.
function requirePlayer(ctx: ReducerCtx<InferSchema<typeof spacetimedb>>) {
  const row = ctx.db.player.identity.find(ctx.sender);
  if (!row) {
    throw new SenderError('Pick a username first');
  }
  return row;
}

export const joinGame = spacetimedb.reducer(ctx => {
  const me = requirePlayer(ctx);
  // Your game logic goes here.
  console.info(`${me.name} joined the game`);
});
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
public static partial class Module
{
    // Returns the caller's player row, or throws if they haven't picked a name yet.
    static Player RequirePlayer(ReducerContext ctx) =>
        ctx.Db.Player.Identity.Find(ctx.Sender) ?? throw new Exception("Pick a username first");

    [SpacetimeDB.Reducer]
    public static void JoinGame(ReducerContext ctx)
    {
        var me = RequirePlayer(ctx);
        // Your game logic goes here.
        Log.Info($"{me.Name} joined the game");
    }
}
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
/// Returns the caller's player row, or an error if they haven't picked a name yet.
fn require_player(ctx: &ReducerContext) -> Result<Player, String> {
    ctx.db
        .player()
        .identity()
        .find(ctx.sender())
        .ok_or_else(|| "Pick a username first".to_string())
}

#[reducer]
pub fn join_game(ctx: &ReducerContext) -> Result<(), String> {
    let me = require_player(ctx)?;
    // Your game logic goes here.
    log::info!("{} joined the game", me.name);
    Ok(())
}
```

</TabItem>
</Tabs>

Clients see the same thing: they subscribe to `player` to list the other players, so players who haven't picked a name yet never show up in lobbies or leaderboards.

## Pick a name from the client

On the client, subscribe to `player` to show players' names. Once the subscription is applied, look up the player's own row: if they don't have one, show the name picker. When they submit a name, call `set_username` and show the error if there is one. Their row arrives through `onInsert` once a name passes every check, which is your signal to start the game. Returning players get theirs as soon as the subscription is applied.

<Tabs groupId="client-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
import { DbConnection, tables } from './module_bindings';

const conn = DbConnection.builder()
  .withUri('ws://localhost:3000')
  .withDatabaseName('my-game')
  .onConnect((ctx, identity) => {
    ctx
      .subscriptionBuilder()
      .onApplied(() => {
        // Players without a row haven't picked a name yet.
        if (!conn.db.player.identity.find(identity)) {
          console.log('Pick a username');
        }
      })
      .subscribe([tables.player]);
  })
  .build();

// The player's own row arrives once their name has passed every check.
conn.db.player.onInsert((_ctx, row) => {
  if (row.identity.isEqual(conn.identity!)) {
    console.log(`Ready to play as ${row.name}`);
  }
});

// Call this when the player submits a name.
async function pickName(name: string) {
  const error = await conn.procedures.setUsername({ name });
  if (error) {
    console.error(`Could not use this name: ${error}`);
  } else {
    console.log(`Your name is now ${name.trim()}`);
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
            .OnApplied(ctx =>
            {
                // Players without a row haven't picked a name yet.
                if (ctx.Db.Player.Identity.Find(identity) is null)
                {
                    Console.WriteLine("Pick a username");
                }
            })
            .AddQuery(q => q.From.Player())
            .Subscribe();
    })
    .Build();

// The player's own row arrives once their name has passed every check.
conn.Db.Player.OnInsert += (ctx, player) =>
{
    if (player.Identity == conn.Identity)
    {
        Console.WriteLine($"Ready to play as {player.Name}");
    }
};

// Call this when the player submits a name.
void PickName(string name)
{
    conn.Procedures.SetUsername(name, (ctx, result) =>
    {
        if (!result.IsSuccess)
        {
            Console.WriteLine($"Could not call the procedure: {result.Error}");
        }
        else if (result.Value != "")
        {
            Console.WriteLine($"Could not use this name: {result.Value}");
        }
        else
        {
            Console.WriteLine($"Your name is now {name.Trim()}");
        }
    });
}
```

Remember to call `conn.FrameTick()` regularly, for example once per frame, so the callbacks run.

</TabItem>
<TabItem value="rust" label="Rust">

```rust
mod module_bindings;
use module_bindings::*;
use spacetimedb_sdk::{DbContext, Table};

fn main() {
    let conn = DbConnection::builder()
        .with_uri("http://localhost:3000")
        .with_database_name("my-game")
        .on_connect(|ctx, identity, _token| {
            ctx.subscription_builder()
                .on_applied(move |ctx| {
                    // Players without a row haven't picked a name yet.
                    if ctx.db.player().identity().find(&identity).is_none() {
                        println!("Pick a username");
                    }
                })
                .add_query(|q| q.from.player())
                .subscribe();
        })
        .build()
        .expect("failed to connect");

    // The player's own row arrives once their name has passed every check.
    conn.db().player().on_insert(|ctx, player| {
        if player.identity == ctx.identity() {
            println!("Ready to play as {}", player.name);
        }
    });

    // Process messages from the database on a background thread.
    conn.run_threaded();

    // Your game loop runs here.
}

// Call this when the player submits a name.
fn pick_name(conn: &DbConnection, name: String) {
    let trimmed = name.trim().to_string();
    conn.procedures.set_username_then(name, move |_ctx, result| match result {
        Ok(error) if error.is_empty() => println!("Your name is now {trimmed}"),
        Ok(error) => eprintln!("Could not use this name: {error}"),
        Err(error) => eprintln!("Could not call the procedure: {error:?}"),
    });
}
```

</TabItem>
</Tabs>

You can also check the format on the client before calling `set_username`, so players see a mistake as they type. Keep the checks on the server too: clients can be modified, so only the module can enforce them.

## What's next?

You now have usernames that are unique and checked before anyone sees them:

- A unique `name_key` column keeps names unique, ignoring case.
- A profanity library rejects swear words for free, inside your module.
- The OpenAI moderation API rejects names that its model judges harmful, even without swear words, with the API key stored in an environment variable.
- The `set_username` procedure runs the cheap checks first, calls the API outside any transaction, and returns errors as values that every client can read.
- Only players with a name have a `player` row, so reducers that need one start with `require_player`.

To learn more about the features used here, see [Constraints](../../00200-core-concepts/00300-tables/00240-constraints.md), [Procedures](../../00200-core-concepts/00200-functions/00400-procedures.md) and [Environment Variables](../../00200-core-concepts/00100-databases/00700-environment-variables.md).

To call other web services from your module the same way, see [Call an AI model from your module](../01200-common-app-patterns/00550-call-ai-model.md).

Next, you can let players [build a friends or follow list](./00200-friends-list.md).

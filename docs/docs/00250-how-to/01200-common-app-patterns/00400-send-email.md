---
title: Send an email from your module
slug: /how-to/app-patterns/send-email
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';

:::note Prerequisites
You need a module and a client connected to it, and an API key from an email provider. This guide uses [Resend](https://resend.com). If you don't have a module yet, follow the [quickstart](../../00100-intro/00100-getting-started/00100-getting-started.md) for your language first.
:::

In this guide, you'll send emails to players from your module, such as a welcome email when a player adds their address. Your module calls your email provider's HTTP API, so the provider's API key never reaches the client. You'll queue each email in a [schedule table](../../00200-core-concepts/00300-tables/00500-schedule-tables.md), send it from a [procedure](../../00200-core-concepts/00200-functions/00400-procedures.md), and store the API key in an [environment variable](../../00200-core-concepts/00100-databases/00700-environment-variables.md).

## How it works

- An `EMAIL_API_KEY` environment variable stores your provider API key on the database. You set it when you publish.
- A private `player_email` table holds each player's email address. A `set_email` reducer saves it.
- A private `email_outbox` schedule table holds the emails to send. A `queue_email` function adds one, from any reducer.
- Each row of `email_outbox` runs the `send_email` procedure, which sends the email to your provider. If the provider can't take it right now, `send_email` queues it again a minute later, up to 3 attempts in all.

Reducers can't make HTTP requests, because they run inside a transaction, so they queue emails instead of sending them. The procedure only sends an email once the transaction that queued it has committed. If a reducer fails, its changes are undone, including its rows in `email_outbox`, so its emails are never sent.

## Define the tables and the API key

Declare an `EMAIL_API_KEY` environment variable in your module. It's required, so SpacetimeDB refuses to publish the module without a value, and your procedure can always read it.

Then add the two tables. Both are private, so clients can't read anyone's address, or the emails waiting to be sent:

- `player_email` has one row per player who added an address.
- `email_outbox` is the schedule table. Each row is an email: its recipient, subject and text, and `attempt`, which counts the times it has been tried. Its `scheduled_at` column says when to send it, and SpacetimeDB then calls `send_email` with the row.

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
import { ScheduleAt, TimeDuration, type Infer } from 'spacetimedb';
import { schema, table, t, SenderError, type InferSchema, type ProcedureCtx, type ReducerCtx } from 'spacetimedb/server';

const playerEmail = table(
  { name: 'player_email' },
  {
    identity: t.identity().primaryKey(),
    email: t.string(),
  }
);

const emailOutbox = table(
  { name: 'email_outbox' },
  {
    scheduledId: t.u64().primaryKey().autoInc(),
    scheduledAt: t.scheduleAt(),
    to: t.string(),
    subject: t.string(),
    text: t.string(),
    attempt: t.u32(),
  }
);

const spacetimedb = schema(
  { playerEmail, emailOutbox },
  { env: { EMAIL_API_KEY: t.string() } }
);
export default spacetimedb;

type Ctx = ReducerCtx<InferSchema<typeof spacetimedb>>;
type ProcCtx = ProcedureCtx<InferSchema<typeof spacetimedb>>;
type Email = Infer<typeof emailOutbox.rowType>;
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
#pragma warning disable STDB_UNSTABLE
using System.Text.Json.Nodes;
using SpacetimeDB;

[SpacetimeDB.Env]
public partial struct ModuleEnv
{
    public string EMAIL_API_KEY;
}

public static partial class Module
{
    [SpacetimeDB.Table(Accessor = "PlayerEmail")]
    public partial struct PlayerEmail
    {
        [SpacetimeDB.PrimaryKey]
        public Identity Identity;
        public string Email;
    }

    [SpacetimeDB.Table(Accessor = "EmailOutbox", Scheduled = "SendEmail", ScheduledAt = "ScheduledAt")]
    public partial struct EmailOutbox
    {
        [SpacetimeDB.PrimaryKey]
        [SpacetimeDB.AutoInc]
        public ulong ScheduledId;
        public ScheduleAt ScheduledAt;
        public string To;
        public string Subject;
        public string Text;
        public uint Attempt;
    }
}
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
use spacetimedb::http::{Request, Timeout};
use spacetimedb::{procedure, reducer, table, Identity, ProcedureContext, ReducerContext, ScheduleAt, Table};
use std::time::Duration;

#[spacetimedb::env]
pub struct Env {
    pub EMAIL_API_KEY: String,
}

#[table(accessor = player_email)]
pub struct PlayerEmail {
    #[primary_key]
    identity: Identity,
    email: String,
}

#[table(accessor = email_outbox, scheduled(send_email))]
pub struct EmailOutbox {
    #[primary_key]
    #[auto_inc]
    scheduled_id: u64,
    scheduled_at: ScheduleAt,
    to: String,
    subject: String,
    text: String,
    attempt: u32,
}
```

Add `serde_json` to your module's `Cargo.toml` to build JSON:

```toml
[dependencies]
serde_json = "1"
```

</TabItem>
</Tabs>

Set the key when you publish, by putting it in the publishing command's environment:

```bash
EMAIL_API_KEY='your-api-key' spacetime publish my-game
```

The key stays in the database across later publishes, so you only pass it once. To replace it, publish again with `--env-only`, which updates environment variables without uploading your module:

```bash
EMAIL_API_KEY='your-new-api-key' spacetime publish my-game --env-only
```

Publishing with `--delete-data` erases the key along with your other data, so pass it again when you do. See [Environment Variables](../../00200-core-concepts/00100-databases/00700-environment-variables.md#supply-values-when-publishing) for every way to supply it.

:::warning
Keep the key away from anything clients can read: don't put it in a `spacetime.json` file you commit, and don't return it from a procedure or a view, or write it to the logs.
:::

## Queue emails

Add a `queue_email` function that inserts an email into `email_outbox`, scheduled for the current time. Call it from any reducer that needs to send an email. SpacetimeDB runs `send_email` with the row once the reducer's transaction commits.

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
// Queues an email. It's sent once the transaction commits, and never if it fails.
function queueEmail(ctx: Ctx, to: string, subject: string, text: string) {
  ctx.db.emailOutbox.insert({
    scheduledId: 0n,
    scheduledAt: ScheduleAt.time(ctx.timestamp.microsSinceUnixEpoch),
    to,
    subject,
    text,
    attempt: 1,
  });
}
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
public static partial class Module
{
    // Queues an email. It's sent once the transaction commits, and never if it fails.
    static void QueueEmail(ReducerContext ctx, string to, string subject, string text)
    {
        ctx.Db.EmailOutbox.Insert(new EmailOutbox
        {
            ScheduledAt = new ScheduleAt.Time(ctx.Timestamp),
            To = to,
            Subject = subject,
            Text = text,
            Attempt = 1,
        });
    }
}
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
/// Queues an email. It's sent once the transaction commits, and never if it fails.
fn queue_email(ctx: &ReducerContext, to: String, subject: String, text: String) {
    ctx.db.email_outbox().insert(EmailOutbox {
        scheduled_id: 0,
        scheduled_at: ScheduleAt::Time(ctx.timestamp),
        to,
        subject,
        text,
        attempt: 1,
    });
}
```

</TabItem>
</Tabs>

## Send queued emails

Add a `post_email` function that sends an email to Resend's [send email API](https://resend.com/docs/api-reference/emails/send-email), and returns the HTTP status of the reply, or 0 if the request didn't get a reply. It's the only provider-specific code: other providers, such as Postmark, SendGrid or Amazon SES, work the same way, with a different URL, headers and body.

Replace the `from` address with one on a domain you've verified with your provider. Providers reject emails from other domains.

Then add the `send_email` procedure. SpacetimeDB calls it with each row of `email_outbox` when the row's time comes, and deletes the row first. `send_email` sends the email with `post_email`, then looks at the status:

- A 2xx status means the provider accepted the email.
- No reply, a 429 status (too many requests) or a 5xx status (a server error) means the provider couldn't take the email right now. `send_email` inserts it again, a minute later, until it has tried 3 times.
- Any other status means the provider rejected the email, for example because the address is invalid, or the API key is wrong. Trying again wouldn't help, so `send_email` logs the status and drops the email.

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
const FROM = 'My Game <noreply@mygame.example>';
const MAX_ATTEMPTS = 3;
const RETRY_DELAY_MICROS = 60_000_000n; // 1 minute

// Sends the email, and returns the HTTP status of the reply, or 0 if there was no reply.
function postEmail(ctx: ProcCtx, email: Email): number {
  try {
    const response = ctx.http.fetch('https://api.resend.com/emails', {
      method: 'POST',
      headers: {
        'content-type': 'application/json',
        authorization: `Bearer ${ctx.env.EMAIL_API_KEY}`,
      },
      body: JSON.stringify({
        from: FROM,
        to: [email.to],
        subject: email.subject,
        text: email.text,
      }),
      timeout: TimeDuration.fromMillis(10_000),
    });
    return response.status;
  } catch (error) {
    console.error(`Could not reach the email API: ${error}`);
    return 0;
  }
}

export const sendEmail = spacetimedb.procedure(
  { onSchedule: emailOutbox },
  { email: emailOutbox.rowType },
  t.unit(),
  (ctx, { email }) => {
    const status = postEmail(ctx, email);
    if (status >= 200 && status < 300) {
      return {};
    }
    const canRetry = status === 0 || status === 429 || status >= 500;
    if (!canRetry || email.attempt >= MAX_ATTEMPTS) {
      console.error(`Dropping email ${email.scheduledId} after attempt ${email.attempt}: status ${status}`);
      return {};
    }
    ctx.withTx(tx => {
      tx.db.emailOutbox.insert({
        ...email,
        scheduledId: 0n,
        scheduledAt: ScheduleAt.time(tx.timestamp.microsSinceUnixEpoch + RETRY_DELAY_MICROS),
        attempt: email.attempt + 1,
      });
    });
    return {};
  }
);
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
public static partial class Module
{
    const string From = "My Game <noreply@mygame.example>";
    const uint MaxAttempts = 3;
    static readonly TimeSpan RetryDelay = TimeSpan.FromMinutes(1);

    // Sends the email, and returns the HTTP status of the reply, or 0 if there was no reply.
    static int PostEmail(ProcedureContext ctx, EmailOutbox email)
    {
        var body = new JsonObject
        {
            ["from"] = From,
            ["to"] = new JsonArray(email.To),
            ["subject"] = email.Subject,
            ["text"] = email.Text,
        };
        var request = new HttpRequest
        {
            Method = SpacetimeDB.HttpMethod.Post,
            Uri = "https://api.resend.com/emails",
            Headers = new List<HttpHeader>
            {
                new HttpHeader("content-type", "application/json"),
                new HttpHeader("authorization", $"Bearer {ctx.Env.EMAIL_API_KEY}"),
            },
            Body = HttpBody.FromString(body.ToJsonString()),
            Timeout = TimeSpan.FromSeconds(10),
        };
        switch (ctx.Http.Send(request))
        {
            case Result<HttpResponse, HttpError>.OkR(var response):
                return response.StatusCode;
            case Result<HttpResponse, HttpError>.ErrR(var error):
                Log.Error($"Could not reach the email API: {error}");
                return 0;
            default:
                return 0;
        }
    }

    [SpacetimeDB.Procedure]
    public static void SendEmail(ProcedureContext ctx, EmailOutbox email)
    {
        var status = PostEmail(ctx, email);
        if (status >= 200 && status < 300)
        {
            return;
        }
        var canRetry = status == 0 || status == 429 || status >= 500;
        if (!canRetry || email.Attempt >= MaxAttempts)
        {
            Log.Error($"Dropping email {email.ScheduledId} after attempt {email.Attempt}: status {status}");
            return;
        }
        var retry = email;
        retry.ScheduledId = 0;
        retry.Attempt = email.Attempt + 1;
        ctx.WithTx(tx =>
        {
            retry.ScheduledAt = new ScheduleAt.Time(tx.Timestamp + RetryDelay);
            return tx.Db.EmailOutbox.Insert(retry);
        });
    }
}
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
const FROM: &str = "My Game <noreply@mygame.example>";
const MAX_ATTEMPTS: u32 = 3;
const RETRY_DELAY: Duration = Duration::from_secs(60);

/// Sends the email, and returns the HTTP status of the reply, or 0 if there was no reply.
fn post_email(ctx: &mut ProcedureContext, email: &EmailOutbox) -> u16 {
    let body = serde_json::json!({
        "from": FROM,
        "to": [email.to],
        "subject": email.subject,
        "text": email.text,
    });
    let request = Request::builder()
        .uri("https://api.resend.com/emails")
        .method("POST")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {}", ctx.env.EMAIL_API_KEY()))
        .extension(Timeout(Duration::from_secs(10).into()))
        .body(body.to_string())
        .expect("the request is valid");
    match ctx.http.send(request) {
        Ok(response) => response.status().as_u16(),
        Err(error) => {
            log::error!("Could not reach the email API: {error:?}");
            0
        }
    }
}

#[procedure]
pub fn send_email(ctx: &mut ProcedureContext, email: EmailOutbox) {
    let status = post_email(ctx, &email);
    if (200..300).contains(&status) {
        return;
    }
    let can_retry = status == 0 || status == 429 || status >= 500;
    if !can_retry || email.attempt >= MAX_ATTEMPTS {
        log::error!(
            "Dropping email {} after attempt {}: status {status}",
            email.scheduled_id,
            email.attempt
        );
        return;
    }
    ctx.with_tx(|tx| {
        tx.db.email_outbox().insert(EmailOutbox {
            scheduled_id: 0,
            scheduled_at: ScheduleAt::Time(tx.timestamp + RETRY_DELAY),
            to: email.to.clone(),
            subject: email.subject.clone(),
            text: email.text.clone(),
            attempt: email.attempt + 1,
        });
    });
}
```

</TabItem>
</Tabs>

The logs don't include the recipient or the text of the email, so they don't reveal players' addresses. Clients can't call `send_email` themselves: scheduled procedures are private, so only the scheduler, the database owner and team collaborators can run them. That's what stops a client from sending any email it likes through your account.

## Let players add their address

Add a `check_email` function that removes the spaces at both ends of an address, and rejects it if it's too long, contains spaces, or doesn't have an `@` followed by a domain with a dot. This catches typos, but only an email the player receives proves that the address is theirs.

Then add a `set_email` reducer that saves the caller's address. The first time a player adds an address, it also queues a welcome email. Changing the address later doesn't send anything, so a player can't use `set_email` to send email after email.

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
const MAX_EMAIL_LENGTH = 254;

// Removes the spaces at both ends of an address, and checks that it looks like one.
function checkEmail(email: string) {
  const trimmed = email.trim();
  const at = trimmed.indexOf('@');
  if (
    trimmed.length > MAX_EMAIL_LENGTH ||
    /\s/.test(trimmed) ||
    at < 1 ||
    !trimmed.slice(at + 1).includes('.')
  ) {
    throw new SenderError("That doesn't look like an email address");
  }
  return trimmed;
}

export const setEmail = spacetimedb.reducer({ email: t.string() }, (ctx, { email }) => {
  const address = checkEmail(email);
  const existing = ctx.db.playerEmail.identity.find(ctx.sender);
  if (existing) {
    ctx.db.playerEmail.identity.update({ ...existing, email: address });
    return;
  }
  ctx.db.playerEmail.insert({ identity: ctx.sender, email: address });
  queueEmail(ctx, address, 'Welcome to My Game', "Thanks for adding your email address. We'll write when something needs your attention.");
});
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
public static partial class Module
{
    const int MaxEmailLength = 254;

    // Removes the spaces at both ends of an address, and checks that it looks like one.
    static string CheckEmail(string email)
    {
        var trimmed = email.Trim();
        var at = trimmed.IndexOf('@');
        if (trimmed.Length > MaxEmailLength
            || trimmed.Any(char.IsWhiteSpace)
            || at < 1
            || !trimmed[(at + 1)..].Contains('.'))
        {
            throw new Exception("That doesn't look like an email address");
        }
        return trimmed;
    }

    [SpacetimeDB.Reducer]
    public static void SetEmail(ReducerContext ctx, string email)
    {
        var address = CheckEmail(email);
        if (ctx.Db.PlayerEmail.Identity.Find(ctx.Sender) is PlayerEmail existing)
        {
            existing.Email = address;
            ctx.Db.PlayerEmail.Identity.Update(existing);
            return;
        }
        ctx.Db.PlayerEmail.Insert(new PlayerEmail { Identity = ctx.Sender, Email = address });
        QueueEmail(ctx, address, "Welcome to My Game", "Thanks for adding your email address. We'll write when something needs your attention.");
    }
}
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
const MAX_EMAIL_LENGTH: usize = 254;

/// Removes the spaces at both ends of an address, and checks that it looks like one.
fn check_email(email: &str) -> Result<String, String> {
    let email = email.trim();
    let looks_valid = match email.split_once('@') {
        Some((local, domain)) => !local.is_empty() && domain.contains('.'),
        None => false,
    };
    if !looks_valid || email.chars().count() > MAX_EMAIL_LENGTH || email.chars().any(char::is_whitespace) {
        return Err("That doesn't look like an email address".to_string());
    }
    Ok(email.to_string())
}

#[reducer]
pub fn set_email(ctx: &ReducerContext, email: String) -> Result<(), String> {
    let address = check_email(&email)?;
    if let Some(existing) = ctx.db.player_email().identity().find(ctx.sender()) {
        ctx.db.player_email().identity().update(PlayerEmail { email: address, ..existing });
        return Ok(());
    }
    ctx.db.player_email().insert(PlayerEmail {
        identity: ctx.sender(),
        email: address.clone(),
    });
    queue_email(
        ctx,
        address,
        "Welcome to My Game".to_string(),
        "Thanks for adding your email address. We'll write when something needs your attention.".to_string(),
    );
    Ok(())
}
```

</TabItem>
</Tabs>

:::warning
Anyone can connect with a new identity, so anyone can still make your module email any address, once per identity. Before you go live, let only signed-in players add an address, send a confirmation link before you send anything else, and limit how many emails your module sends in total. Providers suspend accounts that send too many unwanted emails.
:::

## Call it from a client

On the client, call `set_email` with the address the player entered, and show the error if the reducer rejects it.

<Tabs groupId="client-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
import { DbConnection } from './module_bindings';

const conn = DbConnection.builder()
  .withUri('ws://localhost:3000')
  .withDatabaseName('my-game')
  .build();

// Call this when the player saves their email address.
async function saveEmail(email: string) {
  try {
    await conn.reducers.setEmail({ email });
  } catch (error) {
    console.error(`Could not save the email address: ${error}`);
  }
}
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
using System;
using SpacetimeDB;
using SpacetimeDB.Types;

var conn = DbConnection.Builder()
    .WithUri("http://localhost:3000")
    .WithDatabaseName("my-game")
    .Build();

conn.Reducers.OnSetEmail += (ctx, email) =>
{
    if (ctx.Event.Status is Status.Failed(var error))
    {
        Console.WriteLine($"Could not save the email address: {error}");
    }
};

// Call this when the player saves their email address.
void SaveEmail(string email) => conn.Reducers.SetEmail(email);
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

// Call this when the player saves their email address.
fn save_email(conn: &DbConnection, email: String) {
    conn.reducers
        .set_email_then(email, |_ctx, result| {
            if let Ok(Err(error)) = result {
                eprintln!("Could not save the email address: {error}");
            }
        })
        .unwrap();
}
```

</TabItem>
</Tabs>

## What's next?

You now send emails from your module:

- Reducers queue emails in a private schedule table, so an email only goes out if the change that asked for it commits.
- A scheduled procedure sends each email, with the API key from an environment variable, and tries again when the provider can't take it right now.
- Players' addresses are in a private table, and clients can't send emails directly.

To adapt it to your app:

- **Notify players who aren't online.** Call `queue_email` from the reducers where something happens that a player shouldn't miss, such as `send_direct_message` in [Chat rooms and private DMs](../01100-social/00300-chat-rooms-and-dms.md). To only email players who aren't connected, record who is online with the `client_connected` and `client_disconnected` [lifecycle reducers](../../00200-core-concepts/00200-functions/00300-reducers/00500-lifecycle.md).
- **Confirm addresses.** Add a code to `player_email`, send it in the welcome email, and add a reducer that marks the address as confirmed when the player enters it. Only send other emails to confirmed addresses.
- **Duplicate emails.** If the provider accepts an email but its reply is lost, `send_email` sends it again. To avoid that, store an id with each email that stays the same across attempts, and send it in Resend's `Idempotency-Key` header.
- **HTML emails.** Send an `html` field along with `text`. Keep `text` for email apps that don't show HTML.
- **Unsubscribe.** Add a reducer that deletes the caller's `player_email` row, and tell players about it in each email.

To learn more about the features used here, see [Procedures](../../00200-core-concepts/00200-functions/00400-procedures.md), [Schedule Tables](../../00200-core-concepts/00300-tables/00500-schedule-tables.md) and [Environment Variables](../../00200-core-concepts/00100-databases/00700-environment-variables.md).

Next, you can [call an external API from your module](./00500-call-external-api.md).

---
title: Unit Test Modules
sidebar_label: Unit Testing
slug: /how-to/unit-testing
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';

# Unit Test Modules

A module's reducers, procedures and views are ordinary functions, and most of what they do is read and write tables. Spacetime includes a test harness that runs those functions against an in-memory datastore built from your module's schema, so you can unit test them without publishing a database or running a Spacetime host.

The datastore is the real one, not a mock or a simulator, so tables enforce their constraints and transactions roll back exactly as they would in production. Because it runs in-process and in-memory, a fresh test database takes about a millisecond to create. Each test can have its own, and tests are fast, isolated and deterministic.

The harness controls everything a function would otherwise get from the host: who the caller is, what time it is, the random number seed and, for procedures, the replies to outgoing HTTP requests.

The test utilities are available in the `spacetimedb` module libraries starting with version **2.12.0**. These are server-module tests, not client SDK tests. Keep integration tests against a running server for subscriptions, network connections, deployment, and durability behavior.

## Set up and test a reducer

The examples below use a small message table adapted from the [Rust chat template tests](https://github.com/clockworklabs/SpacetimeDB/blob/master/templates/chat-console-rs/spacetimedb/src/lib.rs) and [TypeScript chat template tests](https://github.com/clockworklabs/SpacetimeDB/blob/master/templates/chat-react-ts/spacetimedb/src/index.test.ts). Run the commands from your module directory, usually `spacetimedb/`.

<Tabs groupId="server-language" defaultValue="typescript">
<TabItem value="typescript" label="TypeScript">

### Configure Vitest

Use Node.js 22.x (22.23.4+), 24.x (24.22.1+), or 26.12.1+. Install the module library and test runner:

```bash
npm install spacetimedb@^2.12.0
npm install --save-dev vitest@^3.2.4 typescript
```

Your `package.json` should use `"type": "module"`. Add a test script alongside any existing scripts:

```json title="package.json (excerpt)"
{
  "type": "module",
  "scripts": {
    "test": "vitest run"
  }
}
```

Module code imports host functions from `spacetime:sys`, which only exists inside a Spacetime host. The `spacetimedbModuleTestPlugin` replaces those imports with the in-memory datastore. Create this configuration in the module directory, not the client project:

```typescript title="vitest.config.ts"
import { defineConfig, type Plugin } from 'vitest/config';
import { spacetimedbModuleTestPlugin } from 'spacetimedb/server/test-utils/vitest';

export default defineConfig({
  plugins: [spacetimedbModuleTestPlugin() as unknown as Plugin],
  test: {
    environment: 'node',
  },
});
```

The plugin also configures the installed `spacetimedb` package to pass through Vitest's transforms. The package includes the test datastore's WebAssembly runtime; you do not need to compile it yourself.

### Define the module

For a standalone example, put this in `src/index.ts`. In an existing module, keep your own schema and reducers instead.

```typescript title="src/index.ts"
import { schema, table, t, SenderError } from 'spacetimedb/server';

const message = table(
  { public: true },
  { sender: t.identity(), sent: t.timestamp(), text: t.string() }
);

const spacetimedb = schema({ message });
export default spacetimedb;

export const sendMessage = spacetimedb.reducer(
  { text: t.string() },
  (ctx, { text }) => {
    if (!text) throw new SenderError('Messages must not be empty');
    ctx.db.message.insert({ sender: ctx.sender, sent: ctx.timestamp, text });
  }
);
```

### Test successful calls and rollback

Import both the default schema and the module's exports. Pass both to `createModuleTestHarness` so it can describe your module. Each call creates a fresh database.

A reducer defined with `spacetimedb.reducer(...)` can be called as a function, taking a reducer context and an object of its arguments. A reducer with no arguments takes an empty object, `{}`. `withReducerTx` supplies the context and wraps the call in a transaction.

```typescript title="src/index.test.ts"
import { expect, it } from 'vitest';
import { ConnectionId, Timestamp, type Identity } from 'spacetimedb';
import {
  createModuleTestHarness,
  TestAuth,
} from 'spacetimedb/server/test-utils';
import spacetimedb, * as moduleExports from './index';

function aliceAuth() {
  return TestAuth.fromJwtPayload(
    JSON.stringify({ iss: 'test-issuer', sub: 'alice' }),
    new ConnectionId(1n)
  );
}

it('stores the caller, timestamp, and message', () => {
  const test = createModuleTestHarness(spacetimedb, moduleExports);
  const now = new Timestamp(1_234_567n);
  let sender: Identity | undefined;
  test.clock.set(now);

  test.withReducerTx(aliceAuth(), ctx => {
    sender = ctx.sender;
    moduleExports.sendMessage(ctx, { text: 'Hello!' });
  });

  const messages = [...test.db.message.iter()];
  expect(sender).toBeDefined();
  expect(messages).toHaveLength(1);
  expect(messages[0]!.sender.isEqual(sender!)).toBe(true);
  expect(messages[0]!.sent.microsSinceUnixEpoch).toBe(now.microsSinceUnixEpoch);
  expect(messages[0]!.text).toBe('Hello!');
});

it('rolls back all writes when the transaction throws', () => {
  const test = createModuleTestHarness(spacetimedb, moduleExports);

  expect(() =>
    test.withReducerTx(aliceAuth(), ctx => {
      moduleExports.sendMessage(ctx, { text: 'This write will roll back' });
      moduleExports.sendMessage(ctx, { text: '' });
    })
  ).toThrow('Messages must not be empty');

  expect(test.db.message.count()).toBe(0n);
});
```

Run the tests:

```bash
npm test
```

`withReducerTx` commits when its callback returns normally and rolls back when it throws, then rethrows the error. Keep the callback synchronous: do not use `async` or return a promise. Propagate errors out of the callback if you expect rollback; catching an error inside it and returning normally commits the transaction.

### Harness options and properties

`createModuleTestHarness` takes an optional third argument, which sets the harness's starting clock, random seed and identity:

```typescript
import { Timestamp } from 'spacetimedb';
import {
  createModuleTestHarness,
  TestClock,
} from 'spacetimedb/server/test-utils';

const test = createModuleTestHarness(spacetimedb, moduleExports, {
  clock: new TestClock(Timestamp.fromDate(new Date('2026-10-12T12:00:00Z'))),
  rngSeed: 42n,
});
```

If most of your tests use the same options, wrap this call in a function of your own and call that at the start of each test.

| Option           | Description                                                            | Default                    |
| ---------------- | ---------------------------------------------------------------------- | -------------------------- |
| `clock`          | The [test clock](#control-time) that supplies `ctx.timestamp`.         | A clock at the Unix epoch. |
| `rngSeed`        | The seed for [`ctx.random`](#control-randomness).                      | `0`                        |
| `moduleIdentity` | The database's own identity, which is `ctx.sender` for internal calls. | `Identity.zero()`          |

The harness exposes:

| Property                                      | Description                                                               |
| --------------------------------------------- | ------------------------------------------------------------------------- |
| `db`                                          | The database's tables, with the same accessors as `ctx.db` in a reducer.  |
| `clock`                                       | The `TestClock` behind `ctx.timestamp`.                                   |
| `rng`                                         | The `TestRng` behind `ctx.random`.                                        |
| `moduleIdentity`                              | The database's own identity.                                              |
| `withReducerTx(auth, body)`                   | Runs `body` with a reducer context, inside a transaction.                 |
| `procedureContext(auth)`                      | A procedure context, for [testing procedures](#test-procedures-and-http). |
| `procedureContextBuilder(auth)`               | Builds a procedure context with an HTTP responder or hooks.               |
| `viewContext(auth)`, `anonymousViewContext()` | Contexts for calling [views](#test-what-clients-will-see).                |
| `runQuery(query)`                             | Runs a query built with the query builder, and returns its rows.          |

</TabItem>
<TabItem value="rust" label="Rust">

### Enable the test utilities

Use Rust 1.97 or later for `spacetimedb` 2.12.0.

Keep your normal `spacetimedb` dependency and add a dev-dependency with the `test-utils` feature. Use the same version for both dependencies:

```toml title="Cargo.toml (excerpt)"
[dependencies]
spacetimedb = "2.12.0"

[dev-dependencies]
spacetimedb = { version = "2.12.0", features = ["test-utils"] }
icu_calendar = ">=2.1, <2.2"
```

:::note
The test datastore depends on `temporal_rs` 0.1.2, which needs `icu_calendar` 2.1.x specifically. Cargo will try to download the latest version (2.2.x) unless we pin it.

If you see `error[E0432]: unresolved import icu_calendar::cal::AnyCalendarDifferenceError`, check you've pinned `icu_calendar` as shown, or run `cargo update -p icu_calendar --precise 2.1.1`.

This won't be needed once [#6317](https://github.com/clockworklabs/SpacetimeDB/issues/6317) is fixed.
:::

Run these tests natively with `cargo test`, not with a WebAssembly target. The feature enables module registration and the in-memory test datastore without requiring a running host.

### Define the module

For a standalone example, put this in `src/lib.rs`. In an existing module, use your own tables and reducers instead.

```rust title="src/lib.rs"
use spacetimedb::{Identity, ReducerContext, Table, Timestamp};

#[spacetimedb::table(accessor = message, public)]
pub struct Message {
    sender: Identity,
    sent: Timestamp,
    text: String,
}

#[spacetimedb::reducer]
pub fn send_message(ctx: &ReducerContext, text: String) -> Result<(), String> {
    if text.is_empty() {
        return Err("Messages must not be empty".to_string());
    }
    ctx.db.message().insert(Message {
        sender: ctx.sender(),
        sent: ctx.timestamp,
        text,
    });
    Ok(())
}
```

### Test successful calls and rollback

Add a test module to `src/lib.rs`. `TestContext::new()` discovers the tables and functions registered in the test binary and creates a fresh database.

```rust title="src/lib.rs (continued)"
#[cfg(test)]
mod tests {
    use super::*;
    use spacetimedb::test_utils::{TestAuth, TestContext};
    use spacetimedb::ConnectionId;

    fn alice_auth() -> TestAuth {
        TestAuth::from_jwt_payload(
            r#"{"iss":"test-issuer","sub":"alice"}"#,
            ConnectionId::from_u128(1),
        )
        .expect("valid test claims")
    }

    #[test]
    fn stores_caller_timestamp_and_message() {
        let test = TestContext::new().expect("test database");
        let now = Timestamp::from_micros_since_unix_epoch(1_234_567);
        test.clock.set(now);

        test.with_reducer_tx(alice_auth(), |ctx| {
            send_message(ctx, "Hello!".to_string())
        })
        .expect("successful reducer");

        let messages = test.db.message().iter().collect::<Vec<_>>();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].sender, Identity::from_claims("test-issuer", "alice"));
        assert_eq!(messages[0].sent, now);
        assert_eq!(messages[0].text, "Hello!");
    }

    #[test]
    fn rolls_back_all_writes_on_error() {
        let test = TestContext::new().expect("test database");

        let result = test.with_reducer_tx(alice_auth(), |ctx| {
            send_message(ctx, "This write will roll back".to_string())?;
            send_message(ctx, String::new())
        });

        assert_eq!(result, Err("Messages must not be empty".to_string()));
        assert_eq!(test.db.message().count(), 0);
    }
}
```

Run the tests:

```bash
cargo test
```

`with_reducer_tx` commits when its callback returns `Ok`, rolls back on `Err`, and rolls back during panic unwinding. Use `?` to propagate reducer errors. For a reducer that returns `()`, call it inside the closure and then return `Ok::<(), String>(())`.

`TestContext` exposes the database's tables as `test.db`, the test clock as `test.clock` and the random number source as `test.rng`. The database's own identity is always `Identity::ZERO`.

</TabItem>
</Tabs>

The rollback examples deliberately call the reducer twice inside one test transaction to demonstrate atomicity. Calls from clients normally run in separate transactions; use one transaction per call when testing that behavior. Running several reducers in one transaction is still useful for setting up a scenario in a single step.

The rest of this guide shows fragments rather than whole files. Code marked `// In the module:` extends the module above; in TypeScript, add any new tables to its `schema({ ... })` call. Tests extend the test file above, importing the module functions they call in TypeScript, or inside its `mod tests` module in Rust.

## Choose the caller

Every call into the module is made by someone, which decides `ctx.sender`. Each test call takes a `TestAuth` saying who the caller is, and there are two kinds of caller:

- **The database itself.** Use it for [scheduled reducers](#test-scheduled-reducers), which the database calls on its own behalf. The sender is the module identity, there is no connection ID or JWT, and the reducer's context reports the call as internal.
- **A client**, built from a JSON claims payload, not a signed token, and a connection ID. Use it for reducers that clients call, and for the client connected and client disconnected lifecycle reducers. The sender is derived from the `iss` and `sub` claims, so the same issuer and subject always give the same identity. The reducer sees the claims as it would for a real connection, including any custom claims. This tests module logic that reads claims; it does not test production JWT signature verification.

The `init` [lifecycle reducer](../00200-core-concepts/00200-functions/00300-reducers/00500-lifecycle.md) is neither. On a published database its sender is the identity that owns the database, usually whoever published it, not the module identity. If your `init` ignores `ctx.sender`, either caller will do. If it reads the sender, for example to record an owner who alone may call admin reducers, call it as a client standing in for the publisher, and use that same client for the owner's calls later in the test. Calling it as the database itself would record the module identity as the owner, and the test's owner-only calls would then be refused.

The harness does not simulate clients connecting or automatically call your initialization or connection reducers. Invoke those functions explicitly inside a test transaction when they are part of your setup, as the chat templates do.

A small helper keeps tests readable. Put it alongside your tests. It gives each client its own connection ID:

<Tabs groupId="server-language" defaultValue="typescript">
<TabItem value="typescript" label="TypeScript">

The `TestAuth` is the first argument to `withReducerTx`. `TestAuth.internal()` is the database itself, and `TestAuth.fromJwtPayload(payload, connectionId)` is a client. In the reducer, `ctx.senderAuth.isInternal` is `true` for an internal call, and `ctx.senderAuth.jwt` holds a client's claims.

```typescript
import { ConnectionId } from 'spacetimedb';
import { TestAuth } from 'spacetimedb/server/test-utils';

let nextConnection = 1n;

// A client signed in as `subject`, with any extra claims.
export function principal(
  subject: string,
  claims: Record<string, unknown> = {}
) {
  return TestAuth.fromJwtPayload(
    JSON.stringify({
      iss: 'https://auth.example.com',
      sub: subject,
      ...claims,
    }),
    new ConnectionId(nextConnection++)
  );
}
```

</TabItem>
<TabItem value="rust" label="Rust">

The `TestAuth` is the first argument to `with_reducer_tx`. `TestAuth::internal()` is the database itself, and `TestAuth::from_jwt_payload(payload, connection_id)` is a client; it returns a `Result`, which fails if the payload isn't valid JSON claims. In the reducer, `ctx.sender_auth().is_internal()` is `true` for an internal call, and `ctx.sender_auth().jwt()` holds a client's claims.

The helper builds its claims with `serde_json`, so add it to `[dev-dependencies]`:

```toml title="Cargo.toml (excerpt)"
[dev-dependencies]
serde_json = "1"
```

The Rust test fragments in the rest of this guide go inside the `mod tests` module above:

```rust
use serde_json::{json, Value};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_CONNECTION: AtomicU64 = AtomicU64::new(1);

/// A client signed in as `subject`, with any extra claims.
fn principal(subject: &str, claims: Value) -> TestAuth {
    let mut payload = json!({ "iss": "https://auth.example.com", "sub": subject });
    if let (Value::Object(payload), Value::Object(extra)) = (&mut payload, claims) {
        payload.extend(extra);
    }
    let connection = NEXT_CONNECTION.fetch_add(1, Ordering::Relaxed);
    TestAuth::from_jwt_payload(payload.to_string(), ConnectionId::from_u128(connection.into()))
        .expect("valid test claims")
}
```

</TabItem>
</Tabs>

### Give a client a role

To give a client a role that a reducer checks for, the test needs the identity the client will have before it calls anything. Read it as the sender of a view context for that client. For a module that records admins in a `user` table, a test can make `alice` an admin before calling any reducers:

<Tabs groupId="server-language" defaultValue="typescript">
<TabItem value="typescript" label="TypeScript">

```typescript
// In the module:
const user = table(
  { name: 'user' },
  { identity: t.identity().primaryKey(), admin: t.bool() }
);
```

```typescript
const test = createModuleTestHarness(spacetimedb, moduleExports);
const alice = principal('alice');

test.db.user.insert({ identity: test.viewContext(alice).sender, admin: true });

// Reducers called as `alice` now find an admin row for the caller.
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
// In the module:
#[spacetimedb::table(accessor = user)]
pub struct User {
    #[primary_key]
    identity: Identity,
    admin: bool,
}
```

```rust
let test = TestContext::new().expect("test database");
let alice = test.view_context(principal("alice", json!({}))).sender();

test.db.user().insert(User { identity: alice, admin: true });

// Reducers called as `alice` now find an admin row for the caller.
```

`Identity::from_claims(issuer, subject)` computes the same identity without a view context.

</TabItem>
</Tabs>

### Check which tokens a client may connect with

A client connected reducer runs as the connecting client, so a test can check which tokens it accepts. This connect reducer accepts only tokens from your own auth provider that were issued for your application, and returning an error from it is what rejects the connection. It checks both claims: the issuer says who signed the token, and the audience says which application it was meant for. Checking only one would let a token from another provider, or one issued for another application, through. See [Restricting auth providers](../00200-core-concepts/00500-authentication/00500-usage.md#example-restricting-auth-providers) for the full guidance.

<Tabs groupId="server-language" defaultValue="typescript">
<TabItem value="typescript" label="TypeScript">

```typescript
// In the module:
const ISSUER = 'https://auth.example.com';
const CLIENT_ID = 'my-app';

export const onConnect = spacetimedb.clientConnected(ctx => {
  const jwt = ctx.senderAuth.jwt;
  if (jwt == null) {
    throw new SenderError('A token is required to connect');
  }
  if (jwt.issuer !== ISSUER) {
    throw new SenderError('This token was issued by another provider');
  }
  if (!jwt.audience.includes(CLIENT_ID)) {
    throw new SenderError('This token was issued for another application');
  }
});
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
// In the module:
const ISSUER: &str = "https://auth.example.com";
const CLIENT_ID: &str = "my-app";

#[spacetimedb::reducer(client_connected)]
pub fn on_connect(ctx: &ReducerContext) -> Result<(), String> {
    let Some(jwt) = ctx.sender_auth().jwt() else {
        return Err("A token is required to connect".to_string());
    };
    if jwt.issuer() != ISSUER {
        return Err("This token was issued by another provider".to_string());
    }
    if !jwt.audience().iter().any(|aud| aud == CLIENT_ID) {
        return Err("This token was issued for another application".to_string());
    }
    Ok(())
}
```

</TabItem>
</Tabs>

The tests connect with a token for this application, a token for another application, and a token from another provider:

<Tabs groupId="server-language" defaultValue="typescript">
<TabItem value="typescript" label="TypeScript">

```typescript
it('accepts a token issued for this application', () => {
  const test = createModuleTestHarness(spacetimedb, moduleExports);

  expect(() =>
    test.withReducerTx(principal('alice', { aud: 'my-app' }), ctx =>
      onConnect(ctx, {})
    )
  ).not.toThrow();
});

it('rejects a token issued for another application', () => {
  const test = createModuleTestHarness(spacetimedb, moduleExports);

  expect(() =>
    test.withReducerTx(principal('alice', { aud: 'another-app' }), ctx =>
      onConnect(ctx, {})
    )
  ).toThrow('This token was issued for another application');
});

it('rejects a token from another provider', () => {
  const test = createModuleTestHarness(spacetimedb, moduleExports);

  expect(() =>
    test.withReducerTx(
      principal('alice', { iss: 'https://evil.example.com', aud: 'my-app' }),
      ctx => onConnect(ctx, {})
    )
  ).toThrow('This token was issued by another provider');
});
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
#[test]
fn accepts_a_token_issued_for_this_application() {
    let test = TestContext::new().expect("test database");
    let auth = principal("alice", json!({ "aud": "my-app" }));

    assert_eq!(test.with_reducer_tx(auth, on_connect), Ok(()));
}

#[test]
fn rejects_a_token_issued_for_another_application() {
    let test = TestContext::new().expect("test database");
    let auth = principal("alice", json!({ "aud": "another-app" }));

    assert_eq!(
        test.with_reducer_tx(auth, on_connect),
        Err("This token was issued for another application".to_string())
    );
}

#[test]
fn rejects_a_token_from_another_provider() {
    let test = TestContext::new().expect("test database");
    let auth = principal("alice", json!({ "iss": "https://evil.example.com", "aud": "my-app" }));

    assert_eq!(
        test.with_reducer_tx(auth, on_connect),
        Err("This token was issued by another provider".to_string())
    );
}
```

</TabItem>
</Tabs>

## Set up and inspect tables

The harness reads and writes the tables directly through `test.db`, outside any reducer, with the same accessors as a reducer's `ctx.db`. Use this to put the database into the state a test needs, rather than reaching that state through a series of reducer calls, and to check the result afterwards. Rows written this way obey the table's constraints, so inserting a duplicate primary key fails just as it would in a reducer.

Create a new harness or context per test. In Rust, `TestContext::clone()` shares the existing database rather than creating an independent one.

The order of rows returned by `iter()` is not guaranteed. Sort them before comparing against a list, as the [scheduled reducer example](#test-scheduled-reducers) does.

## Control time

Reducers and procedures read the current time from their context, which the harness takes from a test clock. The clock starts at the Unix epoch unless you give it another time, and it only moves when a test moves it. Fixing the clock lets tests state timestamps exactly, such as "an edit made 20 minutes ago", and check the timestamps the module writes for equality rather than within a tolerance.

Set the clock before calling a reducer or building a procedure context:

<Tabs groupId="server-language" defaultValue="typescript">
<TabItem value="typescript" label="TypeScript">

```typescript
import { Timestamp, TimeDuration } from 'spacetimedb';

test.clock.set(Timestamp.fromDate(new Date('2026-10-12T12:00:00Z')));
test.clock.advance(TimeDuration.fromMillis(15_000));
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
use spacetimedb::TimeDuration;

test.clock.set(now);
test.clock.advance(TimeDuration::from_micros(15_000_000));
```

</TabItem>
</Tabs>

When a procedure sleeps, the harness advances the test clock to the wake time instead of waiting.

## Control randomness

Reducers and procedures get their random numbers from their context, which the harness seeds from a test random number source. Each call into the module draws a fresh seed from that source, so successive calls see different numbers, but the whole sequence depends only on the source's seed. The same seed always gives the same numbers, which lets a test state exactly what a reducer that rolls dice or shuffles a list will produce.

Choose the seed before the first call, and reseed during a test to start the sequence again:

<Tabs groupId="server-language" defaultValue="typescript">
<TabItem value="typescript" label="TypeScript">

```typescript
it('rolls the same dice for the same seed', () => {
  const test = createModuleTestHarness(spacetimedb, moduleExports, {
    rngSeed: 42n,
  });
  const rollDice = () =>
    test.withReducerTx(TestAuth.internal(), ctx =>
      [1, 2, 3].map(() => ctx.random.integerInRange(1, 6))
    );
  const first = rollDice();

  test.rng.setSeed(42n);

  expect(rollDice()).toEqual(first);
});
```

`rngSeed` is a [harness option](#harness-options-and-properties). A seed can be a `bigint`, a `number` or a `Uint8Array`. `test.rng.seed()` returns the current seed.

</TabItem>
<TabItem value="rust" label="Rust">

```rust
#[test]
fn rolls_the_same_dice_for_the_same_seed() {
    use spacetimedb::rand::Rng;

    let test = TestContext::new().expect("test database");
    test.rng.set_seed(42);
    let roll_dice = || {
        test.with_reducer_tx(TestAuth::internal(), |ctx| {
            Ok::<_, String>([1, 2, 3].map(|_| ctx.rng().gen_range(1..=6)))
        })
        .expect("successful reducer")
    };
    let first = roll_dice();

    test.rng.set_seed(42);

    assert_eq!(roll_dice(), first);
}
```

`test.rng.set_seed(seed)` takes a `u64`. `test.rng` requires the `rand` feature, which is enabled by default.

</TabItem>
</Tabs>

## Test scheduled reducers

The harness stores rows in [schedule tables](../00200-core-concepts/00300-tables/00500-schedule-tables.md) like any other rows, but never runs them: no timer fires during a test. To test a scheduled reducer, call it directly as the database itself, passing a row of its schedule table as the argument.

This module takes edits more than 15 minutes old out of the live set, every five minutes:

<Tabs groupId="server-language" defaultValue="typescript">
<TabItem value="typescript" label="TypeScript">

```typescript
// In the module:
const edit = table(
  { name: 'edit' },
  { id: t.u64().primaryKey(), editedAt: t.timestamp(), live: t.bool() }
);

const expireTimer = table(
  { name: 'expire_timer' },
  { scheduledId: t.u64().primaryKey().autoInc(), scheduledAt: t.scheduleAt() }
);

const FIFTEEN_MINUTES = 15n * 60n * 1_000_000n;

export const expireOldEdits = spacetimedb.reducer(
  { onSchedule: expireTimer },
  { timer: expireTimer.rowType },
  ctx => {
    for (const e of ctx.db.edit.iter()) {
      if (e.live && ctx.timestamp.since(e.editedAt).micros > FIFTEEN_MINUTES) {
        ctx.db.edit.id.update({ ...e, live: false });
      }
    }
  }
);
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
// In the module:
use spacetimedb::ScheduleAt;
use std::time::Duration;

#[spacetimedb::table(accessor = edit)]
pub struct Edit {
    #[primary_key]
    id: u64,
    edited_at: Timestamp,
    live: bool,
}

#[spacetimedb::table(accessor = expire_timer, scheduled(expire_old_edits))]
pub struct ExpireTimer {
    #[primary_key]
    #[auto_inc]
    scheduled_id: u64,
    scheduled_at: ScheduleAt,
}

const FIFTEEN_MINUTES: Duration = Duration::from_secs(15 * 60);

#[spacetimedb::reducer]
pub fn expire_old_edits(ctx: &ReducerContext, _timer: ExpireTimer) {
    for e in ctx.db.edit().iter() {
        let age = ctx.timestamp.duration_since(e.edited_at);
        if e.live && age.is_some_and(|age| age > FIFTEEN_MINUTES) {
            ctx.db.edit().id().update(Edit { live: false, ..e });
        }
    }
}
```

</TabItem>
</Tabs>

The test writes the edits straight into the table, calls the reducer with a timer row, and checks which edits are still live:

<Tabs groupId="server-language" defaultValue="typescript">
<TabItem value="typescript" label="TypeScript">

```typescript
import { ScheduleAt, Timestamp } from 'spacetimedb';

const EXPIRE_TIMER = {
  scheduledId: 1n,
  scheduledAt: ScheduleAt.interval(5n * 60n * 1_000_000n),
};

it('takes edits more than 15 minutes old out of the live set', () => {
  const test = createModuleTestHarness(spacetimedb, moduleExports);
  const now = Timestamp.fromDate(new Date('2026-10-12T12:00:00Z'));
  const minutesAgo = (minutes: bigint) =>
    new Timestamp(now.microsSinceUnixEpoch - minutes * 60n * 1_000_000n);
  test.clock.set(now);

  test.db.edit.insert({ id: 1n, editedAt: minutesAgo(16n), live: true });
  test.db.edit.insert({ id: 2n, editedAt: minutesAgo(15n), live: true });
  test.db.edit.insert({ id: 3n, editedAt: minutesAgo(1n), live: true });

  test.withReducerTx(TestAuth.internal(), ctx =>
    expireOldEdits(ctx, { timer: EXPIRE_TIMER })
  );

  const edits = [...test.db.edit.iter()].sort((a, b) => Number(a.id - b.id));
  expect(edits.map(e => e.live)).toEqual([false, true, true]);
});
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
#[test]
fn takes_edits_more_than_15_minutes_old_out_of_the_live_set() {
    let test = TestContext::new().expect("test database");
    let now = Timestamp::parse_from_rfc3339("2026-10-12T12:00:00Z").expect("valid time");
    let minutes_ago = |minutes: u64| now - Duration::from_secs(minutes * 60);
    test.clock.set(now);

    test.db.edit().insert(Edit { id: 1, edited_at: minutes_ago(16), live: true });
    test.db.edit().insert(Edit { id: 2, edited_at: minutes_ago(15), live: true });
    test.db.edit().insert(Edit { id: 3, edited_at: minutes_ago(1), live: true });

    let timer = ExpireTimer {
        scheduled_id: 1,
        scheduled_at: ScheduleAt::Interval(Duration::from_secs(5 * 60).into()),
    };
    test.with_reducer_tx(TestAuth::internal(), |ctx| {
        expire_old_edits(ctx, timer);
        Ok::<(), String>(())
    })
    .expect("successful reducer");

    let mut edits = test.db.edit().iter().collect::<Vec<_>>();
    edits.sort_by_key(|e| e.id);
    assert_eq!(edits.iter().map(|e| e.live).collect::<Vec<_>>(), [false, true, true]);
}
```

</TabItem>
</Tabs>

## Test procedures and HTTP

A [procedure](../00200-core-concepts/00200-functions/00400-procedures.md) runs outside a transaction and opens its own, so the harness gives it a procedure context rather than wrapping it in a transaction. Create a procedure context for a caller and call the procedure with it.

<Tabs groupId="server-language" defaultValue="typescript">
<TabItem value="typescript" label="TypeScript">

Create a procedure context with `test.procedureContext(auth)`. Inside the procedure, `ctx.withTx(...)` does the database work.

</TabItem>
<TabItem value="rust" label="Rust">

Procedure APIs require the `unstable` feature on the dev-dependency: `features = ["test-utils", "unstable"]`. Create a procedure context with `test.procedure_context(auth)`. Use `ctx.try_with_tx(...)` for database work; `ctx.with_tx(...)` currently panics in tests ([#6296](https://github.com/clockworklabs/SpacetimeDB/pull/6296)).

</TabItem>
</Tabs>

Procedures can make HTTP requests, which no test should send to a real server. Instead, the test supplies a responder, which answers each request the procedure makes. No real HTTP request is made, and a request made without a responder fails with `no test HTTP responder configured`. Install the responder with the procedure context builder:

<Tabs groupId="server-language" defaultValue="typescript">
<TabItem value="typescript" label="TypeScript">

```typescript
it('reads a mocked HTTP response body', () => {
  const test = createModuleTestHarness(spacetimedb, moduleExports);
  const ctx = test
    .procedureContextBuilder(TestAuth.internal())
    .http((_test, request, _body) => {
      expect(request.uri).toBe('https://example.invalid/status');
      return {
        code: 200,
        headers: { entries: [] },
        version: { tag: 'Http11' },
        body: 'healthy',
      };
    })
    .build();

  expect(ctx.http.fetch('https://example.invalid/status').text()).toBe(
    'healthy'
  );
});
```

The responder receives the harness, request metadata, and request body bytes. Return a `TestHttpResponse`: status `code`, `headers`, `version`, and an optional `body` (`string` or `Uint8Array`). An omitted body is empty. Throwing from the responder looks like a network failure to the procedure.

</TabItem>
<TabItem value="rust" label="Rust">

```rust
#[test]
fn reads_a_mocked_http_response_body() {
    let test = TestContext::new().expect("test database");
    let ctx = test
        .procedure_context_builder(TestAuth::internal())
        .http(|_, request| {
            assert_eq!(request.uri().to_string(), "https://example.invalid/status");
            Ok(spacetimedb::http::Response::builder()
                .status(200)
                .body(spacetimedb::http::Body::from("healthy"))
                .expect("valid test response"))
        })
        .build();

    let response = ctx.http.get("https://example.invalid/status").expect("mock response");
    let body = response.into_body().into_string().expect("text body");
    assert_eq!(body, "healthy");
}
```

The responder receives the test context and the request, and returns a `Response`. Returning `Err` looks like a network failure to the procedure.

</TabItem>
</Tabs>

A responder can also record the requests it receives and answer each in turn from a script. This lets a test check both what the procedure asked for and how it handled the reply, including errors and network failures. This procedure checks a service's health, and records a failure when the service returns an error status or can't be reached:

<Tabs groupId="server-language" defaultValue="typescript">
<TabItem value="typescript" label="TypeScript">

```typescript
// In the module:
const fetchFailure = table(
  { name: 'fetch_failure' },
  { id: t.u64().primaryKey().autoInc(), at: t.timestamp() }
);

export const checkStatus = spacetimedb.procedure(t.unit(), ctx => {
  let ok = false;
  try {
    ok = ctx.http.fetch('https://example.invalid/status').ok;
  } catch {
    // A network failure leaves `ok` false.
  }
  if (!ok) {
    ctx.withTx(tx => {
      tx.db.fetchFailure.insert({ id: 0n, at: tx.timestamp });
    });
  }
  return {};
});
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
// In the module:
use spacetimedb::ProcedureContext;

#[spacetimedb::table(accessor = fetch_failure)]
pub struct FetchFailure {
    #[primary_key]
    #[auto_inc]
    id: u64,
    at: Timestamp,
}

#[spacetimedb::procedure]
pub fn check_status(ctx: &mut ProcedureContext) -> Result<(), String> {
    // A network failure counts as not ok.
    let ok = ctx
        .http
        .get("https://example.invalid/status")
        .is_ok_and(|response| response.status().is_success());
    if !ok {
        ctx.try_with_tx(|tx| {
            tx.db.fetch_failure().insert(FetchFailure { id: 0, at: tx.timestamp });
            Ok::<(), String>(())
        })?;
    }
    Ok(())
}
```

</TabItem>
</Tabs>

A scripted responder, to put alongside your tests:

<Tabs groupId="server-language" defaultValue="typescript">
<TabItem value="typescript" label="TypeScript">

```typescript
import type { TestHttpResponse } from 'spacetimedb/server/test-utils';

// Answers requests in turn from `replies`: a body to send as JSON with
// status 200, a bare status code, or an Error to throw as a network failure.
function fakeServer(...replies: (object | number | Error)[]) {
  const requests: URL[] = [];
  const responder = (
    _test: unknown,
    req: { uri: string }
  ): TestHttpResponse => {
    requests.push(new URL(req.uri));
    const reply = replies.shift();
    if (reply === undefined) throw new Error(`Unexpected request: ${req.uri}`);
    if (reply instanceof Error) throw reply;
    return {
      code: typeof reply === 'number' ? reply : 200,
      headers: { entries: [] },
      version: { tag: 'Http11' },
      body: typeof reply === 'number' ? '' : JSON.stringify(reply),
    };
  };
  return { responder, requests };
}
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
use spacetimedb::http::{Body, Error, Request, Response};
use std::{cell::RefCell, collections::VecDeque, rc::Rc};

/// Answers requests in turn from `replies`: a status code to send, or an
/// error to return as a network failure. Records the path of each request.
fn fake_server(
    replies: impl IntoIterator<Item = Result<u16, &'static str>>,
) -> (
    impl Fn(&TestContext, Request) -> Result<Response, Error>,
    Rc<RefCell<Vec<String>>>,
) {
    let replies = RefCell::new(replies.into_iter().collect::<VecDeque<_>>());
    let requests = Rc::new(RefCell::new(Vec::new()));
    let seen = requests.clone();
    let responder = move |_: &TestContext, request: Request| {
        seen.borrow_mut().push(request.uri().path().to_string());
        match replies.borrow_mut().pop_front() {
            Some(Ok(status)) => Ok(Response::builder()
                .status(status)
                .body(Body::empty())
                .expect("valid test response")),
            Some(Err(message)) => Err(Error::new(message)),
            None => panic!("Unexpected request: {}", request.uri()),
        }
    };
    (responder, requests)
}
```

</TabItem>
</Tabs>

The test gives the procedure an error status, then a network failure, and checks that it asked for the right path and recorded one failure each time:

<Tabs groupId="server-language" defaultValue="typescript">
<TabItem value="typescript" label="TypeScript">

```typescript
it.each([
  ['an HTTP error', 503],
  ['a network failure', new Error('connection reset')],
])('counts %s as a failure', (_name, reply) => {
  const test = createModuleTestHarness(spacetimedb, moduleExports);
  const server = fakeServer(reply);
  const ctx = test
    .procedureContextBuilder(TestAuth.internal())
    .http(server.responder)
    .build();

  checkStatus(ctx, {});

  expect(server.requests.map(url => url.pathname)).toEqual(['/status']);
  expect(test.db.fetchFailure.count()).toBe(1n);
});
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
#[test]
fn counts_http_errors_and_network_failures_as_failures() {
    for reply in [Ok(503), Err("connection reset")] {
        let test = TestContext::new().expect("test database");
        let (responder, requests) = fake_server([reply]);
        let mut ctx = test
            .procedure_context_builder(TestAuth::internal())
            .http(responder)
            .build();

        check_status(&mut ctx).expect("procedure completes");

        assert_eq!(*requests.borrow(), ["/status"], "for {reply:?}");
        assert_eq!(test.db.fetch_failure().count(), 1, "for {reply:?}");
    }
}
```

</TabItem>
</Tabs>

A test can also act at points during a procedure, using hooks that run after each transaction the procedure commits and before each sleep. These let you test code that must re-check database state between transactions without relying on wall-clock delays. Pass the hooks to the procedure context builder:

<Tabs groupId="server-language" defaultValue="typescript">
<TabItem value="typescript" label="TypeScript">

Create the hooks with `createProcedureTestHooks()`, add them with `afterTxCommit` and `onSleep`, and pass them to the builder's `hooks` method.

</TabItem>
<TabItem value="rust" label="Rust">

Create the hooks with `ProcedureTestHooks::new()`, add them with `after_tx_commit` and `on_sleep`, and pass them to the builder's `hooks` method.

</TabItem>
</Tabs>

## Test what clients will see

A test can't subscribe, but it can check the [views](../00200-core-concepts/00200-functions/00500-views.md) and queries that clients subscribe to. Call a view with a context for a chosen caller, and check what it returns. This makes per-user views testable: call the same view as two different clients and check that each sees only their own rows.

The harness builds a view context for a chosen caller, or an anonymous one for anonymous views. A view that returns rows gives them back directly. A view that returns a typed query gives back the query, which the harness runs against the committed tables.

Queries run in a test always run as the database owner, which bypasses [row-level security](./00400-row-level-security.md) and can read private tables. A query's results are therefore the rows it matches, not necessarily the rows a particular client would receive.

In this module, each client sees only the notes they wrote:

<Tabs groupId="server-language" defaultValue="typescript">
<TabItem value="typescript" label="TypeScript">

```typescript
// In the module:
const note = table({ name: 'note' }, { owner: t.identity(), text: t.string() });

export const addNote = spacetimedb.reducer(
  { text: t.string() },
  (ctx, { text }) => {
    ctx.db.note.insert({ owner: ctx.sender, text });
  }
);

export const myNotes = spacetimedb.view(
  { name: 'my_notes', public: true },
  t.array(note.rowType),
  ctx => ctx.from.note.where(row => row.owner.eq(ctx.sender))
);
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
// In the module:
use spacetimedb::{Query, ViewContext};

#[spacetimedb::table(accessor = note)]
pub struct Note {
    owner: Identity,
    text: String,
}

#[spacetimedb::reducer]
pub fn add_note(ctx: &ReducerContext, text: String) {
    ctx.db.note().insert(Note { owner: ctx.sender(), text });
}

#[spacetimedb::view(accessor = my_notes, public)]
pub fn my_notes(ctx: &ViewContext) -> impl Query<Note> {
    ctx.from.note().r#where(|n| n.owner.eq(ctx.sender()))
}
```

</TabItem>
</Tabs>

The test adds a note as each of two clients, then calls the view as each of them:

<Tabs groupId="server-language" defaultValue="typescript">
<TabItem value="typescript" label="TypeScript">

A view defined with `spacetimedb.view(...)` can be called as a function, taking a view context and, if the view has parameters, an object of its arguments. Build the context with `test.viewContext(auth)` or `test.anonymousViewContext()`, and run a returned query with `test.runQuery(...)`:

```typescript
it('shows each client only their own notes', () => {
  const test = createModuleTestHarness(spacetimedb, moduleExports);
  const alice = principal('alice');
  const bob = principal('bob');
  test.withReducerTx(alice, ctx => addNote(ctx, { text: 'from Alice' }));
  test.withReducerTx(bob, ctx => addNote(ctx, { text: 'from Bob' }));

  const notesFor = (auth: TestAuth) =>
    test
      .runQuery<{ text: string }>(myNotes(test.viewContext(auth)))
      .map(n => n.text);

  expect(notesFor(alice)).toEqual(['from Alice']);
  expect(notesFor(bob)).toEqual(['from Bob']);
});
```

`test.runQuery` also takes queries built directly from `test.viewContext(auth).from`, including `where` filters and semijoins. It accepts only queries that read rows.

</TabItem>
<TabItem value="rust" label="Rust">

A view is an ordinary function, taking a view context and any parameters. Build the context with `test.view_context(auth)` or `test.anonymous_view_context()`, and run a returned query with `test.run_query(...)`:

```rust
#[test]
fn shows_each_client_only_their_own_notes() {
    let test = TestContext::new().expect("test database");
    for (subject, text) in [("alice", "from Alice"), ("bob", "from Bob")] {
        test.with_reducer_tx(principal(subject, json!({})), |ctx| {
            add_note(ctx, text.to_string());
            Ok::<(), String>(())
        })
        .expect("successful reducer");
    }

    let notes_for = |subject: &str| {
        let ctx = test.view_context(principal(subject, json!({})));
        let notes: Vec<Note> = test.run_query(my_notes(&ctx)).expect("query runs");
        notes.into_iter().map(|n| n.text).collect::<Vec<_>>()
    };

    assert_eq!(notes_for("alice"), ["from Alice"]);
    assert_eq!(notes_for("bob"), ["from Bob"]);
}
```

`test.run_query` also takes queries built directly from a `QueryBuilder`, including `r#where` filters and semijoins.

</TabItem>
</Tabs>

## Limitations

- **Scheduled reducers don't run automatically.** Rows in schedule tables are stored, but no timer fires. Call scheduled reducers directly, as shown [above](#test-scheduled-reducers).
- **No subscriptions or clients.** The harness doesn't deliver rows to subscribers or run client callbacks. Test the views and queries clients subscribe to instead, as shown [above](#test-what-clients-will-see).
- **No row-level security.** Queries run as the database owner, so they ignore row-level security rules and can read private tables.
- **Views can't be queried by name.** Call the view's function instead.
- **No lifecycle simulation.** Connecting, disconnecting and publishing don't call lifecycle reducers. Call them explicitly.

<Tabs groupId="server-language" defaultValue="typescript">
<TabItem value="typescript" label="TypeScript">

- **Host functions outside the datastore.** Module code that calls a host function the harness doesn't provide, such as `volatile_nonatomic_schedule_immediate`, throws `Unsupported SpacetimeDB host syscall in module unit test`.

### Known issues

- **Range scans on indexes** ([#6299](https://github.com/clockworklabs/SpacetimeDB/issues/6299)). The test utilities bundle their own copy of the SDK runtime, so a `Range` imported from `spacetimedb/server` in module code isn't recognized as a range by the harness. An index `filter` or `delete` with a `Range` is treated as a point lookup instead: on a number column it silently matches no rows, and on a bigint-backed column, such as a timestamp, it throws `Cannot convert undefined to a BigInt`. The silent case can make a test of range deletion pass when nothing was deleted. A published database is unaffected.
- **Event table rows written outside a reducer transaction persist.** [Event table](../00200-core-concepts/00300-tables/00550-event-tables.md) rows inserted inside `withReducerTx` are deleted when the transaction commits, as on a published database. Rows inserted directly through the harness's `test.db`, or by a procedure's `withTx`, stay in the table.

</TabItem>
<TabItem value="rust" label="Rust">

### Known issues

- **`ProcedureContext::with_tx` panics** ([#6296](https://github.com/clockworklabs/SpacetimeDB/pull/6296)). In a test, `with_tx` makes host calls instead of using the test datastore, and panics with `holding &mut HandlerContext, so should not be in a tx already`. Use `try_with_tx` in procedures you want to test until the fix is released.
- **Projects from the `chat-console-rs` template have an invalid dev-dependency** ([#6298](https://github.com/clockworklabs/SpacetimeDB/issues/6298)). `spacetime init` writes the template's `spacetimedb` dev-dependency with a `path` into the SpacetimeDB repository. Replace it with a version that matches the normal dependency, as shown in [Enable the test utilities](#enable-the-test-utilities).

</TabItem>
</Tabs>

## More examples

- [Rust chat module tests](https://github.com/clockworklabs/SpacetimeDB/blob/master/templates/chat-console-rs/spacetimedb/src/lib.rs): table registration, independent databases, authenticated reducer calls, procedures, typed queries, and HTTP mocks.
- [TypeScript chat module tests](https://github.com/clockworklabs/SpacetimeDB/blob/master/templates/chat-react-ts/spacetimedb/src/index.test.ts): connection lifecycle, validation failures, caller identity, and timestamps.
- Learn how [reducers](../00200-core-concepts/00200-functions/00300-reducers/00300-reducers.md) handle [errors](../00200-core-concepts/00200-functions/00300-reducers/00600-error-handling.md), which tests can check for.

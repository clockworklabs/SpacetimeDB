---
title: Unit Test Modules
sidebar_label: Unit Testing
slug: /how-to/unit-testing
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';

# Unit Test Modules

Test Rust and TypeScript module code against an isolated, in-memory database without starting a SpacetimeDB server or publishing a module. Use these tests to check reducer behavior, table constraints, transaction rollback, and code that depends on the caller or current time.

The test utilities are available in the `spacetimedb` module libraries starting with version **2.12.0**. These are server-module tests, not client SDK tests. Keep integration tests against a running server for subscriptions, network connections, deployment, and durability behavior.

## Set up and test a reducer

The examples below use a small message table adapted from the [Rust chat template](https://github.com/clockworklabs/SpacetimeDB/blob/master/templates/chat-console-rs/spacetimedb/src/lib.rs) and [TypeScript chat tests](https://github.com/clockworklabs/SpacetimeDB/blob/master/templates/chat-react-ts/spacetimedb/src/index.test.ts). Run the commands from your module directory, usually `spacetimedb/`.

<Tabs groupId="server-language" defaultValue="typescript">
<TabItem value="typescript" label="TypeScript">

### Configure Vitest

Use Node.js 22 or later. Install the module library and test runner:

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

Create this configuration in the module directory, not the client project:

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

The plugin lets module imports run under Vitest instead of requiring SpacetimeDB host syscalls. It also configures the installed `spacetimedb` package to pass through Vitest's transforms. The package includes the test datastore's WebAssembly runtime; you do not need to compile it yourself.

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

```typescript title="src/index.test.ts"
import { expect, it } from 'vitest';
import { ConnectionId, Timestamp, type Identity } from 'spacetimedb';
import { createModuleTestHarness, TestAuth } from 'spacetimedb/server/test-utils';
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

  expect(() => test.withReducerTx(aliceAuth(), ctx => {
    moduleExports.sendMessage(ctx, { text: 'This write will roll back' });
    moduleExports.sendMessage(ctx, { text: '' });
  })).toThrow('Messages must not be empty');

  expect(test.db.message.count()).toBe(0n);
});
```

Run the tests:

```bash
npm test
```

`withReducerTx` commits when its callback returns normally and rolls back when it throws. Keep the callback synchronous: do not use `async` or return a promise. Propagate errors out of the callback if you expect rollback; catching an error inside it and returning normally commits the transaction.

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
```

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

:::note Fresh dependency resolution with 2.12.0
If a fresh `Cargo.lock` selects ICU 2.3, the native test dependencies can fail to compile `temporal_rs` with missing ICU calendar types such as `AnyCalendarDifferenceError`. For this dependency combination, add these compatibility pins alongside the entries in `[dev-dependencies]`, then run `cargo test` again:

```toml
icu_calendar = "=2.1.1"
icu_locale = "=2.1.1"
```

Keep the resulting `Cargo.lock` for reproducible test builds. These pins work around a transitive dependency incompatibility, not a requirement of the module test API.
:::

</TabItem>
</Tabs>

The rollback examples deliberately call the reducer twice inside one test transaction to demonstrate atomicity. Calls from clients normally run in separate transactions; use one transaction per call when testing that behavior.

## Control identities, time, and setup

- **Caller identity:** `TestAuth.fromJwtPayload` / `TestAuth::from_jwt_payload` accepts a JSON claims payload, not a signed token. Use different `iss` and `sub` claims for different users and pass a connection ID to simulate a connected caller. This tests module logic that reads claims; it does not test production JWT signature verification.
- **Internal calls:** `TestAuth.internal()` / `TestAuth::internal()` creates a call with the module identity, no connection ID, and no JWT. The default test module identity is zero.
- **Time:** the test clock starts at the Unix epoch. Set it before constructing a reducer or procedure context to make timestamps deterministic. You can also advance it with `test.clock.advance(...)`.
- **Randomness:** `test.rng.setSeed(...)` in TypeScript and `test.rng.set_seed(...)` in Rust seed future contexts deterministically. Rust requires the `rand` feature, enabled by default.
- **Lifecycle:** the harness does not simulate clients connecting or automatically call your initialization/connection reducers. Invoke those functions explicitly inside a test transaction when they are part of your setup, as the chat templates do.
- **Fixtures and isolation:** seed data through `test.db` or a test transaction. Create a new harness/context per test; Rust `TestContext::clone()` shares the existing database rather than creating an independent one.

## Test procedures, views, and HTTP

The same database can back procedure and view contexts. Rust procedure APIs additionally require the `unstable` feature on the dev-dependency: `features = ["test-utils", "unstable"]`.

<Tabs groupId="server-language" defaultValue="typescript">
<TabItem value="typescript" label="TypeScript">

Create a procedure context with `test.procedureContext(auth)` and use `ctx.withTx(...)` for database work. For a view, use `test.viewContext(auth)` or `test.anonymousViewContext()`. If a view returns a typed query instead of rows, execute that query with `test.runQuery(...)`.

To test outbound HTTP, use `procedureContextBuilder`. Add this test to the TypeScript example above:

```typescript title="src/index.test.ts (continued)"
it('reads a mocked HTTP response body', () => {
  const test = createModuleTestHarness(spacetimedb, moduleExports);
  const ctx = test.procedureContextBuilder(TestAuth.internal())
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

  expect(ctx.http.fetch('https://example.invalid/status').text()).toBe('healthy');
});
```

The responder receives the harness, request metadata, and request body bytes. Return a `TestHttpResponse`: status `code`, `headers`, `version`, and an optional `body` (`string` or `Uint8Array`). An omitted body is empty. No real HTTP request is made; without a configured responder, test HTTP calls throw.

</TabItem>
<TabItem value="rust" label="Rust">

Create a procedure context with `test.procedure_context(auth)`. Use `ctx.with_tx(...)` or `ctx.try_with_tx(...)` for database work. For a view, use `test.view_context(auth)` or `test.anonymous_view_context()`. Execute typed queries with `test.run_query(...)`.

To test outbound HTTP, configure a response through `procedure_context_builder`. With `unstable` enabled, add this separate test module to `src/lib.rs`:

```rust title="src/lib.rs (HTTP test)"
#[cfg(test)]
mod http_tests {
    use spacetimedb::test_utils::{TestAuth, TestContext};

    #[test]
    fn reads_mocked_http_response_body() {
        let test = TestContext::new().expect("test database");
        let ctx = test.procedure_context_builder(TestAuth::internal())
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
}
```

No real HTTP request is made. Without a configured responder, test HTTP calls return an error.

</TabItem>
</Tabs>

For more procedure tests, use `createProcedureTestHooks()` in TypeScript or `ProcedureTestHooks` in Rust to interleave work after a transaction commits or when a procedure sleeps. These hooks let you test code that must re-check database state between transactions without relying on wall-clock delays.

## More examples

- [Rust chat module tests](https://github.com/clockworklabs/SpacetimeDB/blob/master/templates/chat-console-rs/spacetimedb/src/lib.rs): table registration, independent databases, authenticated reducer calls, procedures, typed queries, and HTTP mocks.
- [TypeScript chat module tests](https://github.com/clockworklabs/SpacetimeDB/blob/master/templates/chat-react-ts/spacetimedb/src/index.test.ts): connection lifecycle, validation failures, caller identity, and timestamps.

---
title: Server-Side TypeScript Gateways with Effect
slug: /how-to/effect-typescript-gateway
---

# Server-Side TypeScript Gateways with Effect

Effect is not required to use SpacetimeDB. It can be a good fit, however, when a
TypeScript server owns the SpacetimeDB SDK connection and exposes a different
transport to browsers or other clients.

This pattern is useful for applications where:

- A server process maintains a persistent SpacetimeDB WebSocket connection.
- Browser clients receive projected updates over Server-Sent Events, HTTP
  streaming, or another framework-specific transport.
- HTTP handlers, background jobs, and CLI commands all need to call the same
  reducers.
- Tests should be able to replace SpacetimeDB with an in-memory service layer.

The examples below use Effect v4 conventions:

- Define dependencies with `Context.Service`.
- Build live implementations with explicit `Layer.effect` values.
- Manage the SDK connection with `Effect.acquireRelease`.
- Adapt callback-based APIs with `Effect.callback`.
- Run Node.js entrypoints with `NodeRuntime.runMain`.

## Architecture

A server-side gateway normally has these pieces:

| Piece | Responsibility |
| --- | --- |
| SpacetimeDB module | Owns tables, reducers, views, procedures, and authorization checks. |
| Generated TypeScript bindings | Provide the typed `DbConnection`, tables, reducers, and row types. |
| Gateway service | Owns the SDK connection lifecycle and exposes application methods. |
| Web adapter | Converts HTTP requests into service calls and streams updates to clients. |
| CLI adapter | Reuses the same service layer for one-off commands and scripts. |
| Test layer | Replaces the live gateway with deterministic in-memory behavior. |

The important boundary is that route handlers and CLI commands depend on a
gateway interface, not directly on `DbConnection`. This keeps connection setup,
subscription callbacks, reducer invocation, retries, logging, and teardown in
one place.

## Gateway Service

This example assumes your module has a `note` table and a `create_note` reducer
whose generated TypeScript accessor is `createNote`.

```typescript
import { Context, Effect, Layer, Queue, Stream } from "effect";
import { DbConnection, tables, type Note } from "./module_bindings";

export interface CreateNoteInput {
  readonly body: string;
}

export interface NoteEvent {
  readonly type: "note_inserted";
  readonly note: {
    readonly id: string;
    readonly body: string;
  };
}

export class SpacetimeGatewayError {
  readonly _tag = "SpacetimeGatewayError";

  constructor(readonly cause: unknown) {}
}

const toGatewayError = (cause: unknown) => new SpacetimeGatewayError(cause);

const toNoteEvent = (note: Note): NoteEvent => ({
  type: "note_inserted",
  note: {
    id: note.id.toString(),
    body: note.body,
  },
});

export class SpacetimeConfig extends Context.Service<
  SpacetimeConfig,
  {
    readonly host: string;
    readonly databaseName: string;
    readonly token: string;
  }
>()("app/SpacetimeConfig") {}

export class SpacetimeGateway extends Context.Service<
  SpacetimeGateway,
  {
    readonly createNote: (
      input: CreateNoteInput
    ) => Effect.Effect<void, SpacetimeGatewayError>;
    readonly noteEvents: Stream.Stream<NoteEvent>;
  }
>()("app/SpacetimeGateway") {
  static readonly layer = Layer.effect(
    SpacetimeGateway,
    Effect.gen(function*() {
      const config = yield* SpacetimeConfig;

      const noteEvents = yield* Effect.acquireRelease(
        Queue.unbounded<NoteEvent>(),
        Queue.shutdown
      );

      const conn = yield* Effect.acquireRelease(
        Effect.callback<DbConnection, SpacetimeGatewayError>((resume) => {
          let settled = false;

          const complete = (
            effect: Effect.Effect<DbConnection, SpacetimeGatewayError>
          ) => {
            if (!settled) {
              settled = true;
              resume(effect);
            }
          };

          const conn = DbConnection.builder()
            .withUri(config.host)
            .withDatabaseName(config.databaseName)
            .withToken(config.token)
            .onConnect((conn) => {
              conn.db.note.onInsert((_ctx, note) => {
                Effect.runFork(Queue.offer(noteEvents, toNoteEvent(note)));
              });

              conn
                .subscriptionBuilder()
                .onApplied(() => complete(Effect.succeed(conn)))
                .onError((_ctx, error) =>
                  complete(Effect.fail(toGatewayError(error)))
                )
                .subscribe(tables.note);
            })
            .onConnectError((_ctx, error) =>
              complete(Effect.fail(toGatewayError(error)))
            )
            .build();

          return Effect.sync(() => conn.disconnect()).pipe(Effect.ignore);
        }),
        (conn) => Effect.sync(() => conn.disconnect()).pipe(Effect.ignore)
      );

      const createNote = Effect.fn("SpacetimeGateway.createNote")(
        (input: CreateNoteInput) =>
          Effect.tryPromise({
            try: () => conn.reducers.createNote(input),
            catch: toGatewayError,
          })
      );

      return SpacetimeGateway.of({
        createNote,
        noteEvents: Stream.fromQueue(noteEvents),
      });
    })
  );
}
```

The layer owns the WebSocket. When the layer is released, the connection is
closed and the queue backing the event stream is shut down. The rest of the
application only sees `createNote` and `noteEvents`.

## Server-Sent Events Adapter

A web framework adapter can turn the gateway stream into a standard SSE
response. The same shape can be used from TanStack Start server functions,
Analog server routes, Express, Hono, or any environment that can return a
standard `Response`.

```typescript
import { Effect, Stream } from "effect";
import { SpacetimeGateway } from "./SpacetimeGateway";

const encodeSse = (event: unknown) =>
  `data: ${JSON.stringify(event)}\n\n`;

export const makeNoteEventsResponse = SpacetimeGateway.use((gateway) =>
  Effect.sync(
    () =>
      new Response(
        gateway.noteEvents.pipe(
          Stream.map(encodeSse),
          Stream.encodeText,
          Stream.toReadableStream
        ),
        {
          headers: {
            "Content-Type": "text/event-stream",
            "Cache-Control": "no-cache",
            Connection: "keep-alive",
          },
        }
      )
  )
);
```

The browser-facing transport is independent from the SpacetimeDB connection.
Browsers can use SSE for read updates while POST requests, server actions, or
framework-specific server functions call gateway methods for writes.

## CLI Commands

Use the same layer for operational commands. This avoids maintaining a separate
script path that opens its own ad hoc connection and handles errors
differently.

```typescript
import { NodeRuntime } from "@effect/platform-node";
import { Effect, Layer } from "effect";
import { SpacetimeConfig, SpacetimeGateway } from "./SpacetimeGateway";

const ConfigLayer = Layer.succeed(SpacetimeConfig)({
  host: process.env.SPACETIMEDB_HOST!,
  databaseName: process.env.SPACETIMEDB_DATABASE!,
  token: process.env.SPACETIMEDB_TOKEN!,
});

const AppLayer = SpacetimeGateway.layer.pipe(
  Layer.provide(ConfigLayer)
);

const program = Effect.gen(function*() {
  const gateway = yield* SpacetimeGateway;
  const body = process.argv.slice(2).join(" ");

  yield* gateway.createNote({ body });
});

NodeRuntime.runMain(program.pipe(Effect.provide(AppLayer)));
```

In Effect v4, `NodeRuntime.runMain` is still the recommended Node.js process
entrypoint because it installs signal handling and interrupts the root fiber
gracefully.

## Testing

Tests should provide a fake gateway layer instead of opening a SpacetimeDB
connection. The same HTTP handlers, server functions, and CLI programs can then
be tested without a database process.

```typescript
import { assert, it, layer } from "@effect/vitest";
import { Context, Effect, Layer, Ref, Stream } from "effect";
import {
  type CreateNoteInput,
  SpacetimeGateway,
} from "./SpacetimeGateway";

class GatewayCalls extends Context.Service<
  GatewayCalls,
  Ref.Ref<ReadonlyArray<CreateNoteInput>>
>()("test/GatewayCalls") {
  static readonly layer = Layer.effect(
    GatewayCalls,
    Ref.make<ReadonlyArray<CreateNoteInput>>([])
  );
}

const SpacetimeGatewayTest = Layer.effect(
  SpacetimeGateway,
  Effect.gen(function*() {
    const calls = yield* GatewayCalls;

    return SpacetimeGateway.of({
      createNote: (input) =>
        Ref.update(calls, (all) => [...all, input]),
      noteEvents: Stream.empty,
    });
  })
).pipe(Layer.provideMerge(GatewayCalls.layer));

layer(SpacetimeGatewayTest)("create note command", (it) => {
  it.effect("records reducer intent without opening a WebSocket", () =>
    Effect.gen(function*() {
      const gateway = yield* SpacetimeGateway;
      const calls = yield* GatewayCalls;

      yield* gateway.createNote({ body: "ship docs" });

      assert.deepStrictEqual(yield* Ref.get(calls), [
        { body: "ship docs" },
      ]);
    }));
});
```

This test verifies application behavior at the gateway boundary. Separate
integration tests can still run against a published local SpacetimeDB module
when you need to verify generated bindings, reducer authorization, or
subscription behavior.

## Best Practices

- Keep `DbConnection` inside one service layer. Do not create new SDK
  connections from each route handler.
- Expose domain methods and streams from the service. Avoid passing the raw
  connection through the rest of the app.
- Use `Effect.acquireRelease` for SDK connections and queues so tests, servers,
  and CLI commands tear down cleanly.
- Use `Effect.callback` for SDK lifecycle callbacks in Effect v4.
- Reuse the same service layer from the web server and CLI entrypoints.
- Use fake layers for unit tests and reserve live SpacetimeDB connections for
  integration tests.
- Keep authorization in the SpacetimeDB module. The gateway can authenticate
  HTTP requests, but reducers should still validate the caller, actor, tenant,
  or service identity they expect.
- Treat the frontend framework as an adapter. TanStack Start, Analog, and other
  frameworks should call the same gateway service rather than each owning a
  different SpacetimeDB connection strategy.

## Related Docs

- [Connecting to SpacetimeDB](../../00200-core-concepts/00600-clients/00300-connection.md)
- [TypeScript SDK Reference](../../00200-core-concepts/00600-clients/00700-typescript-reference.md)
- [Subscription Semantics](../../00200-core-concepts/00400-subscriptions/00200-subscription-semantics.md)
- [Using Auth Claims](../../00200-core-concepts/00500-authentication/00500-usage.md)

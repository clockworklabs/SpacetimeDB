---
title: Server Gateway and SSE Relay
slug: /how-to/server-gateway-sse
---

This guide shows how to put a web application server between browsers and SpacetimeDB. The server owns the SpacetimeDB TypeScript SDK connection over WebSocket, subscribes to module data, calls reducers after application authorization, and relays live updates to browsers with Server-Sent Events (SSE).

Use this pattern when the browser should not connect to SpacetimeDB directly, or when your app already centralizes sessions, tenant selection, API keys, rate limits, impersonation, and audit policy in a web server.

```text
Browser
  |
  | HTTP routes, server functions, EventSource
  v
Application server
  |
  | app session, tenant context, input validation, rate limits
  | generated TypeScript SDK
  v
SpacetimeDB over WebSocket
```

The code below is intentionally framework-neutral. The same shape can be used from API routes, server functions, server handlers, CLI smoke tests, scheduled jobs, or a long-running Node process. Generated binding names depend on your module, so adapt table and reducer names to your generated TypeScript bindings.

## When to use it

This pattern is a good fit when you need to:

- Keep SpacetimeDB credentials, API keys, refresh tokens, and robot credentials off the browser.
- Reuse application-owned sessions, organizations, customer SSO, API keys, or tenant context before a reducer is called.
- Fan out one server-side subscription stream to many browser tabs.
- Use SSE because your browser UI mostly needs server-to-browser realtime updates over ordinary HTTP.
- Test reducer calls and subscription projection from CLI commands without opening a browser.

Direct browser connections are still simpler when every browser client should connect to SpacetimeDB with its own token and can enforce authorization entirely through module reducers, views, and subscriptions.

## Example module contract

Assume your SpacetimeDB module exposes these module-specific pieces:

- A table or view named `document`.
- A reducer named `update_document`.
- A tenant or organization field such as `tenant_id`.
- Reducer-side checks that verify the sender, actor, tenant, and current authorization state.

After generating TypeScript bindings, your application imports the generated `DbConnection` and generated row types:

```ts title="gateway/module-bindings.ts"
export { DbConnection } from "../module_bindings";
export type { Document } from "../module_bindings";
```

Generate bindings as usual:

```bash
spacetime generate --lang typescript \
  --out-dir app/module_bindings \
  --module-path server
```

For SDK details, see the [TypeScript reference](../../00200-core-concepts/00600-clients/00700-typescript-reference.md).

## Event envelopes

Use a small event envelope between the gateway and browsers. Keep it stable so the browser can reconnect and resume from the last event ID.

```ts title="gateway/events.ts"
export type SseEvent = {
  id: string;
  event: "document.inserted" | "document.updated" | "document.deleted";
  tenant_id: string;
  schema_version: 1;
  payload: unknown;
};

type Listener = (event: SseEvent) => void;

export class EventBus {
  #nextId = 0;
  #listeners = new Map<string, Set<Listener>>();
  #history: SseEvent[] = [];

  publish(event: Omit<SseEvent, "id" | "schema_version">) {
    const envelope: SseEvent = {
      ...event,
      id: String(++this.#nextId),
      schema_version: 1,
    };

    this.#history.push(envelope);
    if (this.#history.length > 1000) {
      this.#history.shift();
    }

    for (const listener of this.#listeners.get(envelope.tenant_id) ?? []) {
      listener(envelope);
    }
  }

  subscribe(tenantId: string, listener: Listener) {
    let listeners = this.#listeners.get(tenantId);
    if (listeners == null) {
      listeners = new Set();
      this.#listeners.set(tenantId, listeners);
    }

    listeners.add(listener);
    return () => listeners.delete(listener);
  }

  eventsAfter(tenantId: string, lastEventId?: string) {
    const lastSeen = lastEventId == null ? 0 : Number(lastEventId);
    return this.#history.filter(
      event => event.tenant_id === tenantId && Number(event.id) > lastSeen
    );
  }
}
```

For production systems, store replay state in durable storage if events must survive a process restart. In-memory history is enough for a minimal local example.

## Server-owned SpacetimeDB connection

The gateway owns the generated SDK connection. It subscribes to the data the server needs, converts row callbacks into SSE envelopes, and exposes reducer methods to request handlers.

```ts title="gateway/spacetime-gateway.ts"
import { DbConnection, type Document } from "./module-bindings";
import { EventBus } from "./events";

type GatewayConfig = {
  uri: string;
  databaseName: string;
  spacetimeToken: string;
};

type UpdateDocumentInput = {
  tenantId: string;
  documentId: string;
  title: string;
  body: string;
};

export class SpacetimeGateway {
  readonly events = new EventBus();
  #conn: DbConnection | undefined;

  constructor(readonly config: GatewayConfig) {}

  start() {
    if (this.#conn != null) {
      return;
    }

    this.#conn = DbConnection.builder()
      .withUri(this.config.uri)
      .withDatabaseName(this.config.databaseName)
      .withToken(this.config.spacetimeToken)
      .onConnect(conn => {
        conn.db.document.onInsert((_ctx, row) => this.#publishDocument("document.inserted", row));
        conn.db.document.onUpdate((_ctx, _oldRow, newRow) => this.#publishDocument("document.updated", newRow));
        conn.db.document.onDelete((_ctx, row) => this.#publishDocument("document.deleted", row));

        conn.subscriptionBuilder().subscribe("SELECT * FROM document");
      })
      .onConnectError((_ctx, error) => {
        console.error("SpacetimeDB connection failed", error);
      })
      .onDisconnect((_ctx, error) => {
        this.#conn = undefined;
        console.error("SpacetimeDB connection closed", error);
      })
      .build();
  }

  async updateDocument(input: UpdateDocumentInput) {
    const conn = this.#requireConnection();

    await conn.reducers.updateDocument({
      tenantId: input.tenantId,
      documentId: input.documentId,
      title: input.title,
      body: input.body,
    });
  }

  stop() {
    this.#conn?.disconnect();
    this.#conn = undefined;
  }

  #publishDocument(event: "document.inserted" | "document.updated" | "document.deleted", row: Document) {
    this.events.publish({
      event,
      tenant_id: row.tenantId,
      payload: row,
    });
  }

  #requireConnection() {
    if (this.#conn == null) {
      throw new Error("SpacetimeDB gateway has not connected yet");
    }

    return this.#conn;
  }
}
```

This example uses one gateway connection. Depending on your authorization model, you may instead use a per-tenant connection, per-user connection, service connection, or hybrid topology. If reducers need native user attribution in `ctx.sender`, use a user-scoped token for reducer calls. If a service connection calls reducers on behalf of users, pass an effective actor argument only after deriving it from trusted server-side session state.

## Shared runtime singleton

Create the gateway once per long-running server process. Frameworks that reload modules during development may need a small global cache to avoid opening duplicate WebSocket connections.

```ts title="gateway/runtime.ts"
import { SpacetimeGateway } from "./spacetime-gateway";

let gateway: SpacetimeGateway | undefined;

export function getGateway() {
  if (gateway == null) {
    gateway = new SpacetimeGateway({
      uri: mustGetEnv("SPACETIME_URI"),
      databaseName: mustGetEnv("SPACETIME_DATABASE"),
      spacetimeToken: mustGetEnv("SPACETIME_GATEWAY_TOKEN"),
    });
    gateway.start();
  }

  return gateway;
}

function mustGetEnv(name: string) {
  const value = process.env[name];
  if (value == null || value.length === 0) {
    throw new Error(`Missing required environment variable ${name}`);
  }
  return value;
}

process.once("SIGINT", () => getGateway().stop());
process.once("SIGTERM", () => getGateway().stop());
```

In serverless environments, prefer direct per-request connections only if your platform and workload can tolerate WebSocket startup and shutdown for each invocation. A server gateway is usually a better fit for long-running processes, containers, or runtimes with durable process state.

## SSE endpoint

An SSE endpoint authorizes the browser's web session, determines the active tenant, replays recent events after `Last-Event-ID`, and keeps the stream open for new events.

```ts title="routes/events.ts"
import { getGateway } from "../gateway/runtime";

export async function GET(request: Request) {
  const session = await requireWebSession(request);
  const tenantId = await requireActiveTenant(session);

  const gateway = getGateway();
  const lastEventId = request.headers.get("last-event-id") ?? undefined;
  const encoder = new TextEncoder();

  const stream = new ReadableStream<Uint8Array>({
    start(controller) {
      for (const event of gateway.events.eventsAfter(tenantId, lastEventId)) {
        controller.enqueue(encoder.encode(formatSse(event.event, event.id, event)));
      }

      const unsubscribe = gateway.events.subscribe(tenantId, event => {
        controller.enqueue(encoder.encode(formatSse(event.event, event.id, event)));
      });

      request.signal.addEventListener("abort", () => {
        unsubscribe();
        controller.close();
      });
    },
  });

  return new Response(stream, {
    headers: {
      "content-type": "text/event-stream; charset=utf-8",
      "cache-control": "no-cache, no-transform",
      connection: "keep-alive",
    },
  });
}

function formatSse(event: string, id: string, data: unknown) {
  return [
    `event: ${event}`,
    `id: ${id}`,
    `data: ${JSON.stringify(data)}`,
    "",
    "",
  ].join("\n");
}
```

SSE is one-way. Browser mutations should still go through server functions, API routes, or server handlers that validate input and call reducers through the gateway.

## Mutation endpoint

Every mutation endpoint should re-check the web session, active tenant, permissions, rate limits, and input shape before calling a reducer.

```ts title="routes/update-document.ts"
import { getGateway } from "../gateway/runtime";

export async function POST(request: Request) {
  const session = await requireWebSession(request);
  const tenantId = await requireActiveTenant(session);

  await requirePermission(session, tenantId, "document:update");

  const body = await request.json();
  const input = parseUpdateDocumentInput(body);

  await getGateway().updateDocument({
    tenantId,
    documentId: input.documentId,
    title: input.title,
    body: input.body,
  });

  return Response.json({ ok: true });
}
```

Do not accept `tenantId`, `actorId`, or impersonation state from the browser unless the server verifies those values against trusted session state. Reducers should also verify tenant membership and permissions from claims, tables, or reducer arguments derived by the server.

## Browser client

The browser consumes the stream with `EventSource`. It receives initial data through SSR, a loader, or a normal HTTP endpoint, then uses SSE for incremental updates.

```ts title="browser/events.ts"
export function subscribeToDocuments(onEvent: (event: MessageEvent) => void) {
  const source = new EventSource("/api/events");

  source.addEventListener("document.inserted", onEvent);
  source.addEventListener("document.updated", onEvent);
  source.addEventListener("document.deleted", onEvent);

  source.onerror = () => {
    console.warn("SSE disconnected; the browser will retry automatically");
  };

  return () => source.close();
}
```

The browser automatically sends `Last-Event-ID` when reconnecting after it has received SSE event IDs. The server can use that header to replay recent events from memory or durable storage.

## CLI smoke test

Because the gateway is plain server code, you can test it without a browser.

```ts title="scripts/smoke-gateway.ts"
import { getGateway } from "../gateway/runtime";

async function main() {
  const gateway = getGateway();
  const tenantId = mustGetEnv("TEST_TENANT_ID");

  const received = new Promise<void>((resolve, reject) => {
    const timeout = setTimeout(() => reject(new Error("Timed out waiting for document event")), 5000);

    const unsubscribe = gateway.events.subscribe(tenantId, event => {
      if (event.event === "document.updated") {
        clearTimeout(timeout);
        unsubscribe();
        resolve();
      }
    });
  });

  await gateway.updateDocument({
    tenantId,
    documentId: "doc_smoke_test",
    title: "Smoke test",
    body: "Updated from the gateway smoke test",
  });

  await received;
  gateway.stop();
}

function mustGetEnv(name: string) {
  const value = process.env[name];
  if (value == null || value.length === 0) {
    throw new Error(`Missing required environment variable ${name}`);
  }
  return value;
}

await main();
```

Run it with your normal TypeScript runner:

```bash
npx tsx scripts/smoke-gateway.ts
```

A useful CI smoke test should mint or load a short-lived gateway token, connect to a local or staging database, call one reducer, observe one projected event, and shut down cleanly.

## Security checklist

- Keep long-lived credentials and API keys on the server.
- Keep SpacetimeDB gateway tokens short-lived when they are derived from web sessions or API keys.
- Check `iss` and `aud` inside the module for authenticated workflows.
- Resolve user, tenant, service actor, and impersonation context from trusted server-side state.
- Treat SSE as a read stream. Use separate authorized server endpoints for writes.
- Do not rely on client-supplied `tenantId`, `actorId`, role, or permission values.
- Revoke or narrow SSE streams when the user's session, tenant membership, or permissions change.
- Add backpressure and replay limits so slow browsers cannot exhaust server memory.
- Dispose of WebSocket connections during process shutdown.

## Framework placement

For React full-stack apps, use loaders or SSR for the initial snapshot, server functions or API routes for mutations, and an API route for the SSE stream.

For Angular or Analog apps, use server-side data fetching for the initial snapshot, Nitro or h3 server routes for mutations and SSE, and an Angular service, signal, or RxJS adapter around `EventSource`.

The important boundary is the same in every framework: browser code should talk to your application server, and the server should be the component that owns auth checks, SpacetimeDB connections, reducer calls, subscription projection, and SSE fanout.

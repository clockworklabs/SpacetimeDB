---
title: Analog Gateway and SSE Relay
slug: /how-to/analog-gateway-sse
---

This guide shows how to use an Analog application server as a realtime gateway for SpacetimeDB. The browser talks to Analog over ordinary HTTP. Analog owns the SpacetimeDB TypeScript SDK connection over WebSocket, validates application sessions, calls reducers after authorization, and relays subscribed changes to the browser with Server-Sent Events (SSE).

This pattern is useful when you are building a multi-tenant web application where the application server owns auth, tenant membership, enterprise identity, billing, auditing, and other policy decisions. Browsers receive only the projected data they are allowed to see, and they never receive the SpacetimeDB service token.

## Architecture

```
Browser Angular page
  | load data with Analog server-side load
  | mutate through Analog API routes
  | receive events through EventSource
  v
Analog server
  | validate session, tenant, and permission
  | keep SDK token server-only
  | call reducers and fan out subscription changes
  v
SpacetimeDB TypeScript SDK over WebSocket
  | subscribe to module data
  | call reducers
  v
SpacetimeDB module
```

Use this pattern when:

- You want Analog server-side rendering or route loading for the initial snapshot.
- You want browser writes to pass through application authorization before reducers run.
- You want SSE because the browser mostly needs server-to-browser realtime updates.
- You want the same gateway code to be testable from CLI commands without launching a browser.
- You want a framework boundary that can later share the same gateway with other server runtimes.

This pattern is usually not the right fit when the browser needs full bidirectional realtime behavior with low latency and can safely connect directly to SpacetimeDB with user-scoped tokens. In that case, the browser SDK may be simpler.

## File layout

Analog API routes live under `src/server/routes`, and server-side page data can live in `.server.ts` files beside the page. Keep the SpacetimeDB SDK connection in server-only files and import it only from API routes, server-side loads, smoke tests, and other trusted server code.

```text
src/
  app/
    pages/
      documents.page.ts
      documents.server.ts
    services/
      document-events.service.ts
  server/
    routes/
      api/
        documents.get.ts
        documents.post.ts
        documents/
          events.get.ts
    spacetime/
      events.ts
      module-bindings.ts
      runtime.ts
      spacetime-gateway.ts
scripts/
  smoke-analog-gateway.ts
```

`module-bindings.ts` represents the generated TypeScript bindings for your module. Generate bindings as part of your application build or development setup, then import them only from server-owned gateway code.

## Event bus

SSE works best when every message has a stable ID, event name, schema version, tenant scope, and JSON payload. The ID lets the browser reconnect with `Last-Event-ID`; the tenant scope keeps fanout authorization explicit.

```ts title="src/server/spacetime/events.ts"
export type SseEvent = {
  id: string;
  event: "document.inserted" | "document.updated" | "document.deleted";
  schema_version: 1;
  tenant_id: string;
  payload: unknown;
};

type Listener = (event: SseEvent) => void;

export class EventBus {
  #nextId = 0;
  #history: SseEvent[] = [];
  #listeners = new Map<string, Set<Listener>>();

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

## SpacetimeDB gateway

The gateway owns the generated SDK connection. It subscribes to the data the server needs, converts row callbacks into SSE envelopes, and exposes methods that Analog routes can call after they have validated the web session.

```ts title="src/server/spacetime/spacetime-gateway.ts"
import { DbConnection, type Document } from "./module-bindings";
import { EventBus, type SseEvent } from "./events";

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

  listDocuments(tenantId: string) {
    const conn = this.#requireConnection();

    return Array.from(conn.db.document.iter()).filter(row => row.tenantId === tenantId);
  }

  stop() {
    this.#conn?.disconnect();
    this.#conn = undefined;
  }

  #publishDocument(event: SseEvent["event"], row: Document) {
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

## Runtime singleton

Create the gateway once per long-running server process. Analog development servers may reload modules during development, so a small global cache prevents duplicate WebSocket connections.

```ts title="src/server/spacetime/runtime.ts"
import { SpacetimeGateway } from "./spacetime-gateway";

declare global {
  var __spacetimeGateway: SpacetimeGateway | undefined;
}

export function getGateway() {
  if (globalThis.__spacetimeGateway == null) {
    globalThis.__spacetimeGateway = new SpacetimeGateway({
      uri: mustGetEnv("SPACETIME_URI"),
      databaseName: mustGetEnv("SPACETIME_DATABASE"),
      spacetimeToken: mustGetEnv("SPACETIME_GATEWAY_TOKEN"),
    });
    globalThis.__spacetimeGateway.start();
  }

  return globalThis.__spacetimeGateway;
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

This pattern assumes a runtime that can keep a process and WebSocket connection alive. If your deployment platform treats every request as an isolated serverless invocation, prefer a durable gateway process, a container, or another runtime that supports long-lived outbound WebSockets and long-lived HTTP streams.

## Server-side page load

Use an Analog `.server.ts` file for the initial snapshot. The load function runs on the server, so it can import the gateway, inspect cookies, and resolve the active tenant before returning data to the page.

```ts title="src/app/pages/documents.server.ts"
import type { PageServerLoad } from "@analogjs/router";
import { getGateway } from "../../server/spacetime/runtime";

export const load = async ({ event }: PageServerLoad) => {
  const session = await requireWebSession(event);
  const tenantId = await requireActiveTenant(session);

  return {
    documents: getGateway().listDocuments(tenantId),
  };
};
```

The browser receives the serialized load result, not the SpacetimeDB connection. Keep the session lookup, tenant lookup, SDK token, and gateway imports out of `.page.ts` and other browser-bundled files.

## API route for mutations

SSE is one-way, so browser writes should go through API routes. Each route should re-check the web session, active tenant, permissions, rate limits, and input shape before calling a reducer.

```ts title="src/server/routes/api/documents.post.ts"
import { createError, defineEventHandler, readBody } from "h3";
import { getGateway } from "../../spacetime/runtime";

type UpdateDocumentBody = {
  documentId?: unknown;
  title?: unknown;
  body?: unknown;
};

export default defineEventHandler(async event => {
  const session = await requireWebSession(event);
  const tenantId = await requireActiveTenant(session);
  await requirePermission(session, tenantId, "document:update");

  const body = await readBody<UpdateDocumentBody>(event);
  const input = parseUpdateDocumentBody(body);

  await getGateway().updateDocument({
    tenantId,
    documentId: input.documentId,
    title: input.title,
    body: input.body,
  });

  return { ok: true };
});

function parseUpdateDocumentBody(body: UpdateDocumentBody) {
  if (
    typeof body.documentId !== "string" ||
    typeof body.title !== "string" ||
    typeof body.body !== "string"
  ) {
    throw createError({
      statusCode: 400,
      statusMessage: "Invalid document update",
    });
  }

  return {
    documentId: body.documentId,
    title: body.title,
    body: body.body,
  };
}
```

Do not accept `tenantId`, `actorId`, or impersonation state from the browser unless the server verifies those values against trusted session state. Reducers should also verify tenant membership and permissions from claims, tables, or reducer arguments derived by the server.

## API route for SSE

Use a GET API route for the browser's `EventSource`. The route authorizes the request, replays events after `Last-Event-ID`, subscribes to new events for the active tenant, and returns a `text/event-stream` response. Analog's Nitro and h3 stack also provides `createEventStream`; this example returns a Web `Response` with a `ReadableStream` so named events and event IDs are explicit in one place.

```ts title="src/server/routes/api/documents/events.get.ts"
import { defineEventHandler } from "h3";
import { getGateway } from "../../../spacetime/runtime";

export default defineEventHandler(async event => {
  const session = await requireWebSession(event);
  const tenantId = await requireActiveTenant(session);
  const gateway = getGateway();
  const lastEventId = event.req.headers.get("last-event-id") ?? undefined;
  const encoder = new TextEncoder();

  const stream = new ReadableStream<Uint8Array>({
    start(controller) {
      for (const item of gateway.events.eventsAfter(tenantId, lastEventId)) {
        controller.enqueue(encoder.encode(formatSse(item.event, item.id, item)));
      }

      const unsubscribe = gateway.events.subscribe(tenantId, item => {
        controller.enqueue(encoder.encode(formatSse(item.event, item.id, item)));
      });

      event.req.signal.addEventListener(
        "abort",
        () => {
          unsubscribe();
          controller.close();
        },
        { once: true }
      );
    },
  });

  return new Response(stream, {
    headers: {
      "content-type": "text/event-stream; charset=utf-8",
      "cache-control": "no-cache, no-transform",
      connection: "keep-alive",
    },
  });
});

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

Browsers automatically send `Last-Event-ID` when reconnecting after receiving SSE event IDs. The server can use that header to replay recent events from memory or durable storage.

## Angular event service

Wrap `EventSource` in a small Angular service so components do not own protocol details. You can adapt this to signals, RxJS, or your application's state store.

```ts title="src/app/services/document-events.service.ts"
import { Injectable, NgZone, signal } from "@angular/core";

type DocumentRow = {
  tenantId: string;
  documentId: string;
  title: string;
  body: string;
};

type DocumentEvent = {
  event: "document.inserted" | "document.updated" | "document.deleted";
  payload: DocumentRow;
};

@Injectable({ providedIn: "root" })
export class DocumentEventsService {
  readonly documents = signal<DocumentRow[]>([]);
  #source: EventSource | undefined;

  constructor(private readonly zone: NgZone) {}

  connect(initialDocuments: DocumentRow[]) {
    this.documents.set(initialDocuments);
    this.#source?.close();

    const source = new EventSource("/api/documents/events");
    this.#source = source;

    source.addEventListener("document.inserted", event => this.#apply(event));
    source.addEventListener("document.updated", event => this.#apply(event));
    source.addEventListener("document.deleted", event => this.#apply(event));
  }

  disconnect() {
    this.#source?.close();
    this.#source = undefined;
  }

  #apply(message: MessageEvent) {
    const envelope = JSON.parse(message.data) as DocumentEvent;

    this.zone.run(() => {
      this.documents.update(current => {
        if (envelope.event === "document.deleted") {
          return current.filter(row => row.documentId !== envelope.payload.documentId);
        }

        const next = current.filter(row => row.documentId !== envelope.payload.documentId);
        next.push(envelope.payload);
        return next;
      });
    });
  }
}
```

The service should be the only browser code that opens the stream. Keep write methods separate so mutations continue to pass through authorized API routes.

## Analog page

The page reads the initial snapshot from the server-side load, starts the SSE service in the browser, and sends writes through the API route.

```ts title="src/app/pages/documents.page.ts"
import { CommonModule } from "@angular/common";
import { Component, OnDestroy, OnInit, computed, inject } from "@angular/core";
import { toSignal } from "@angular/core/rxjs-interop";
import { injectLoad } from "@analogjs/router";
import { DocumentEventsService } from "../services/document-events.service";
import { load } from "./documents.server";

@Component({
  standalone: true,
  imports: [CommonModule],
  template: `
    <section>
      <article *ngFor="let document of documents()">
        <h2>{{ document.title }}</h2>
        <p>{{ document.body }}</p>
        <button type="button" (click)="save(document.documentId, document.title, document.body)">
          Save
        </button>
      </article>
    </section>
  `,
})
export default class DocumentsPage implements OnInit, OnDestroy {
  private readonly loaded = toSignal(injectLoad<typeof load>(), { requireSync: true });
  private readonly documentEvents = inject(DocumentEventsService);
  readonly documents = computed(() => this.documentEvents.documents());

  ngOnInit() {
    this.documentEvents.connect(this.loaded().documents);
  }

  ngOnDestroy() {
    this.documentEvents.disconnect();
  }

  async save(documentId: string, title: string, body: string) {
    const response = await fetch("/api/documents", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ documentId, title, body }),
    });

    if (!response.ok) {
      throw new Error("Document update failed");
    }
  }
}
```

The important boundary is that the page can import the load type and consume serialized data, but it should not import the gateway runtime, generated SDK connection, service token, or server-only auth helpers.

## CLI smoke test

Because the gateway is plain server code, you can test it from a command in the same repository that owns the Analog app. This is useful for CI, local development, and server-side auth changes where launching a browser would make failures harder to diagnose.

```ts title="scripts/smoke-analog-gateway.ts"
import { getGateway } from "../src/server/spacetime/runtime";

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
    documentId: "smoke-test-document",
    title: "Smoke test",
    body: `Updated at ${new Date().toISOString()}`,
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

Run the script with the same environment variables used by the Analog server:

```sh
SPACETIME_URI=ws://localhost:3000 \
SPACETIME_DATABASE=my-database \
SPACETIME_GATEWAY_TOKEN=... \
TEST_TENANT_ID=tenant_123 \
pnpm tsx scripts/smoke-analog-gateway.ts
```

For HTTP-level smoke tests, start the Analog server, open the SSE endpoint with `curl -N`, and call the mutation route from another shell. This validates route wiring, session cookies, streaming headers, and browser-facing event formatting.

## Security checklist

- Keep `SPACETIME_GATEWAY_TOKEN` and generated SDK connections in server-only files.
- Resolve `tenantId` and `actorId` from trusted session state, not request JSON.
- Authorize every mutation route before calling a reducer.
- Keep reducer authorization checks in the SpacetimeDB module as a defense in depth.
- Project only browser-safe rows into SSE payloads.
- Store SSE replay history durably if missed events must survive process restarts.
- Revoke or narrow SSE streams when session, tenant membership, or permissions change.
- Choose a deployment runtime that supports long-lived WebSockets and long-lived HTTP streams.

## What to adapt

Replace the example `document` table, `updateDocument` reducer, and auth helper names with your module's generated bindings and application auth layer. The structure should stay the same: Analog owns the browser session, the server gateway owns SpacetimeDB connectivity, reducers enforce module invariants, and SSE carries authorized read updates back to the browser.

## Related Analog docs

- [API Routes](https://analogjs.org/docs/features/api/overview)
- [Server-Side Data Fetching](https://analogjs.org/docs/features/data-fetching/server-side-data-fetching)
- [WebSocket and Server-Sent Events support](https://analogjs.org/docs/features/api/websockets)

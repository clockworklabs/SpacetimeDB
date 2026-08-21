// SpacetimeDB adapter: implements the stack-bench ChatClient against the running
// `chat` database using the SpacetimeDB TypeScript SDK + generated bindings.
//
// VALIDATED against spacetimedb CLI/SDK 2.2.0 (see module_bindings/, pinned in
// package.json). Run via `tsx` — the generated bindings use extensionless ESM
// imports that tsc+node (NodeNext) reject but esbuild/tsx resolve. Node 22+
// supplies the global WebSocket the SDK needs.
//
// In Harbor SHARED verifier mode the grader runs in the agent's container, so we
// connect to the in-container instance on localhost.

import { DbConnection } from "./module_bindings/index.js";
import type { ChatClient, ChatMessage } from "./harness/src/chatClient.js";

const URI = process.env.STDB_URI ?? "ws://127.0.0.1:3000";
const DB = process.env.STDB_DB ?? "chat";

class SpacetimeChatClient implements ChatClient {
  private conn: any;
  private connected!: Promise<void>;

  async connect(): Promise<void> {
    this.connected = new Promise<void>((resolve, reject) => {
      this.conn = DbConnection.builder()
        .withUri(URI)
        .withDatabaseName(DB)
        .onConnect(() => resolve())
        .onConnectError((_ctx: any, err: any) => reject(err))
        .build();
    });
    await this.connected;
  }

  async subscribe(onMessage: (msg: ChatMessage) => void): Promise<void> {
    // Register the row callback BEFORE subscribing so initial-sync (history) rows
    // are delivered too — onInsert fires for both initial sync and live inserts.
    this.conn.db.message.onInsert((_ctx: any, row: any) => {
      onMessage({ sender: row.sender, text: row.text });
    });
    await new Promise<void>((resolve, reject) => {
      this.conn
        .subscriptionBuilder()
        .onApplied(() => resolve())
        .onError((ctx: any) => reject(new Error("subscription error: " + String(ctx))))
        .subscribe(["SELECT * FROM message"]);
    });
  }

  async send(sender: string, text: string): Promise<void> {
    // Reducer takes a single object arg, not positional.
    this.conn.reducers.sendMessage({ sender, text });
  }

  async close(): Promise<void> {
    try {
      this.conn?.disconnect();
    } catch {
      /* ignore */
    }
  }
}

export default function makeClient(): ChatClient {
  return new SpacetimeChatClient();
}

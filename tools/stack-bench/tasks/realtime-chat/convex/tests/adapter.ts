// Convex adapter: implements the stack-bench ChatClient against the running app.
//
// VALIDATED against self-hosted convex-backend (convex npm SDK). Uses `anyApi` so
// the grader references functions by name (messages.*) without importing the app's
// generated bindings. Run via tsx; Node 22+ supplies the global WebSocket.
//
// In Harbor SHARED mode the grader runs in `main` and reaches the backend service
// at http://convex-backend:3210 over the compose network.

import { ConvexClient } from "convex/browser";
import { anyApi } from "convex/server";
import type { ChatClient, ChatMessage } from "./harness/src/chatClient.js";

const CONVEX_URL = process.env.CONVEX_URL ?? "http://convex-backend:3210";

class ConvexChatClient implements ChatClient {
  private client: ConvexClient;
  private unsubscribe?: () => void;

  constructor(url: string) {
    this.client = new ConvexClient(url);
  }

  async connect(): Promise<void> {
    // ConvexClient connects lazily on the first query/mutation.
  }

  async subscribe(onMessage: (msg: ChatMessage) => void): Promise<void> {
    let seen = 0;
    // onUpdate delivers the full list immediately (history) and on every change
    // (real-time). Diff against `seen` to emit each message once, in order.
    this.unsubscribe = this.client.onUpdate(
      anyApi.messages.listMessages,
      {},
      (rows: Array<{ sender: string; text: string }>) => {
        for (let i = seen; i < rows.length; i++) {
          onMessage({ sender: rows[i].sender, text: rows[i].text });
        }
        seen = rows.length;
      },
    );
  }

  async send(sender: string, text: string): Promise<void> {
    await this.client.mutation(anyApi.messages.sendMessage, { sender, text });
  }

  async close(): Promise<void> {
    this.unsubscribe?.();
    await this.client.close();
  }
}

export default function makeClient(): ChatClient {
  return new ConvexChatClient(CONVEX_URL);
}

// Convex adapter for the stack-bench team-chat task.
//
// Implements the harness AppClient contract against the running self-hosted
// Convex backend via the official client SDK. Mutations reject with the
// mutation's thrown error (the contract's accept/reject semantics).
//
// Real-time: Convex pushes full reactive-query results on every change; the
// adapter diffs consecutive results to synthesize per-row events. The first
// delivery is the room's history in seq order (Convex query results are
// index-ordered), satisfying the history-then-live contract.

import { ConvexClient } from "convex/browser";
import { anyApi } from "convex/server";
import type {
  AppClient,
  MemberEventHandlers,
  MemberRecord,
  MessageRecord,
  RoomEventHandlers,
  UserEventHandlers,
  UserRecord,
} from "./harness/src/appClient.js";

const CONVEX_URL = process.env.CONVEX_URL ?? "http://127.0.0.1:3210";

class ConvexAppClient implements AppClient {
  private client!: ConvexClient;
  private unsubs: Array<() => void> = [];

  async connect(): Promise<void> {
    this.client = new ConvexClient(CONVEX_URL);
    // Fail fast if the backend is unreachable (ConvexClient connects lazily).
    await this.client.query(anyApi.users.get, { username: "__probe__" });
  }

  async close(): Promise<void> {
    for (const u of this.unsubs.splice(0)) {
      try {
        u();
      } catch {
        /* ignore */
      }
    }
    await this.client?.close();
  }

  // ---- mutations ----
  register(username: string) {
    return this.client.mutation(anyApi.users.register, { username }) as Promise<void>;
  }
  setStatus(username: string, status: string) {
    return this.client.mutation(anyApi.users.setStatus, { username, status }) as Promise<void>;
  }
  createRoom(username: string, room: string) {
    return this.client.mutation(anyApi.rooms.createRoom, { username, room }) as Promise<void>;
  }
  joinRoom(username: string, room: string) {
    return this.client.mutation(anyApi.rooms.joinRoom, { username, room }) as Promise<void>;
  }
  leaveRoom(username: string, room: string) {
    return this.client.mutation(anyApi.rooms.leaveRoom, { username, room }) as Promise<void>;
  }
  kick(actor: string, room: string, target: string) {
    return this.client.mutation(anyApi.rooms.kick, { actor, room, target }) as Promise<void>;
  }
  sendMessage(sender: string, room: string, text: string, clientMsgId: string) {
    return this.client.mutation(anyApi.messages.send, { sender, room, text, clientMsgId }) as Promise<void>;
  }
  editMessage(actor: string, room: string, clientMsgId: string, newText: string) {
    return this.client.mutation(anyApi.messages.edit, { actor, room, clientMsgId, newText }) as Promise<void>;
  }
  deleteMessage(actor: string, room: string, clientMsgId: string) {
    return this.client.mutation(anyApi.messages.remove, { actor, room, clientMsgId }) as Promise<void>;
  }
  markRead(user: string, room: string, upToSeq: number) {
    return this.client.mutation(anyApi.messages.markRead, { user, room, upToSeq }) as Promise<void>;
  }
  tip(fromUser: string, toUser: string, amount: number) {
    return this.client.mutation(anyApi.credits.tip, { fromUser, toUser, amount }) as Promise<void>;
  }

  // ---- subscriptions (diff full reactive results into per-row events) ----
  async subscribeRoom(room: string, handlers: RoomEventHandlers): Promise<void> {
    const seen = new Map<string, string>(); // clientMsgId -> serialized record
    const unsub = this.client.onUpdate(anyApi.messages.list, { room }, (rows: MessageRecord[]) => {
      const ordered = [...rows].sort((a, b) => a.seq - b.seq);
      for (const row of ordered) {
        const key = JSON.stringify(row);
        if (seen.get(row.clientMsgId) !== key) {
          seen.set(row.clientMsgId, key);
          handlers.onMessage(row);
        }
      }
    });
    this.unsubs.push(unsub);
  }

  async subscribeUsers(handlers: UserEventHandlers): Promise<void> {
    const seen = new Map<string, string>();
    const unsub = this.client.onUpdate(anyApi.users.list, {}, (rows: UserRecord[]) => {
      for (const row of rows) {
        const key = JSON.stringify(row);
        if (seen.get(row.username) !== key) {
          seen.set(row.username, key);
          handlers.onUser(row);
        }
      }
    });
    this.unsubs.push(unsub);
  }

  async subscribeMembers(room: string, handlers: MemberEventHandlers): Promise<void> {
    const seen = new Map<string, string>();
    const unsub = this.client.onUpdate(anyApi.rooms.listMembers, { room }, (rows: MemberRecord[]) => {
      const present = new Set(rows.map((r) => r.user));
      for (const user of [...seen.keys()]) {
        if (!present.has(user)) {
          seen.delete(user);
          handlers.onMemberRemoved?.(user);
        }
      }
      for (const row of rows) {
        const key = JSON.stringify(row);
        if (seen.get(row.user) !== key) {
          seen.set(row.user, key);
          handlers.onMember(row);
        }
      }
    });
    this.unsubs.push(unsub);
  }

  // ---- snapshots ----
  getUser(username: string): Promise<UserRecord | null> {
    return this.client.query(anyApi.users.get, { username }) as Promise<UserRecord | null>;
  }
  getRoomOwner(room: string): Promise<string | null> {
    return this.client.query(anyApi.rooms.getOwner, { room }) as Promise<string | null>;
  }
  getMembers(room: string): Promise<MemberRecord[]> {
    return this.client.query(anyApi.rooms.listMembers, { room }) as Promise<MemberRecord[]>;
  }
  async getMessages(room: string): Promise<MessageRecord[]> {
    const rows = (await this.client.query(anyApi.messages.list, { room })) as MessageRecord[];
    return rows.sort((a, b) => a.seq - b.seq);
  }
}

export default function makeClient(): AppClient {
  return new ConvexAppClient();
}

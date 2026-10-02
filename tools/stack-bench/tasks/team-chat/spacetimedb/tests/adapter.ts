// SpacetimeDB adapter for the stack-bench team-chat task.
//
// Implements the harness AppClient contract against the running `teamchat`
// database via the SpacetimeDB TypeScript SDK (2.5.x) + generated bindings.
//
// - Mutations use the SDK's promise-based reducer calls: they resolve when the
//   reducer commits and reject with the reducer's error message when it fails,
//   which is exactly the contract's accept/reject semantics.
// - connect() subscribes to user/room/member; message subscriptions are
//   created per-room with a server-side WHERE filter, so cross-room isolation
//   is graded against real backend routing, not a client-side filter.
// - History-then-live ordering: initial-sync message inserts are buffered
//   until the subscription is applied, sorted by seq, then emitted.

import { DbConnection } from "./module_bindings/index.js";
import type {
  AppClient,
  MemberRecord,
  MessageRecord,
  MemberEventHandlers,
  RoomEventHandlers,
  UserEventHandlers,
  UserRecord,
} from "./harness/src/appClient.js";

const URI = process.env.STDB_URI ?? "ws://127.0.0.1:3000";
const DB = process.env.STDB_DB ?? "teamchat";

type MessageRow = {
  seq: number;
  clientMsgId: string;
  sender: string;
  text: string;
  edited: boolean;
  deleted: boolean;
  room: string;
};

const msgRec = (r: MessageRow): MessageRecord => ({
  seq: Number(r.seq),
  clientMsgId: r.clientMsgId,
  sender: r.sender,
  text: r.text,
  edited: r.edited,
  deleted: r.deleted,
});

class SpacetimeAppClient implements AppClient {
  private conn: any;
  private roomSubs = new Set<string>();

  async connect(): Promise<void> {
    await new Promise<void>((resolve, reject) => {
      this.conn = DbConnection.builder()
        .withUri(URI)
        .withDatabaseName(DB)
        .onConnect(() => resolve())
        .onConnectError((_ctx: any, err: any) => reject(err ?? new Error("connect error")))
        .build();
    });
    // Base subscriptions (small tables). Message subs are per-room.
    await this.subscribeSql(["SELECT * FROM user", "SELECT * FROM room", "SELECT * FROM member"]);
  }

  private subscribeSql(queries: string[]): Promise<void> {
    return new Promise<void>((resolve, reject) => {
      this.conn
        .subscriptionBuilder()
        .onApplied(() => resolve())
        .onError((_ctx: any, err: any) => reject(err ?? new Error("subscription error")))
        .subscribe(queries);
    });
  }

  /** Server-side filtered subscription for one room's messages. */
  private async ensureRoomSub(room: string): Promise<void> {
    if (this.roomSubs.has(room)) return;
    this.roomSubs.add(room);
    await this.subscribeSql([`SELECT * FROM message WHERE room = '${room.replace(/'/g, "''")}'`]);
  }

  async close(): Promise<void> {
    try {
      this.conn?.disconnect();
    } catch {
      /* ignore */
    }
  }

  // ---- mutations ----
  register(username: string) {
    return this.conn.reducers.register({ username });
  }
  setStatus(username: string, status: string) {
    return this.conn.reducers.setStatus({ username, status });
  }
  createRoom(username: string, room: string) {
    return this.conn.reducers.createRoom({ username, room });
  }
  joinRoom(username: string, room: string) {
    return this.conn.reducers.joinRoom({ username, room });
  }
  leaveRoom(username: string, room: string) {
    return this.conn.reducers.leaveRoom({ username, room });
  }
  kick(actor: string, room: string, target: string) {
    return this.conn.reducers.kick({ actor, room, target });
  }
  sendMessage(sender: string, room: string, text: string, clientMsgId: string) {
    return this.conn.reducers.sendMessage({ sender, room, text, clientMsgId });
  }
  editMessage(actor: string, room: string, clientMsgId: string, newText: string) {
    return this.conn.reducers.editMessage({ actor, room, clientMsgId, newText });
  }
  deleteMessage(actor: string, room: string, clientMsgId: string) {
    return this.conn.reducers.deleteMessage({ actor, room, clientMsgId });
  }
  markRead(user: string, room: string, upToSeq: number) {
    return this.conn.reducers.markRead({ user, room, upToSeq });
  }
  tip(fromUser: string, toUser: string, amount: number) {
    return this.conn.reducers.tip({ fromUser, toUser, amount });
  }

  // ---- subscriptions ----
  async subscribeRoom(room: string, handlers: RoomEventHandlers): Promise<void> {
    // Register callbacks first, buffering until the room subscription applies;
    // then emit buffered history sorted by seq, then live events directly.
    let applied = false;
    const buffer: MessageRow[] = [];
    const emit = (row: MessageRow) => {
      if (row.room !== room) return;
      if (applied) handlers.onMessage(msgRec(row));
      else buffer.push(row);
    };
    this.conn.db.message.onInsert((_ctx: any, row: MessageRow) => emit(row));
    this.conn.db.message.onUpdate((_ctx: any, _old: MessageRow, row: MessageRow) => emit(row));
    await this.ensureRoomSub(room);
    buffer.sort((a, b) => Number(a.seq) - Number(b.seq));
    for (const row of buffer) handlers.onMessage(msgRec(row));
    applied = true;
  }

  async subscribeUsers(handlers: UserEventHandlers): Promise<void> {
    const emit = (row: any) =>
      handlers.onUser({ username: row.username, status: row.status, balance: Number(row.balance) });
    this.conn.db.user.onInsert((_ctx: any, row: any) => emit(row));
    this.conn.db.user.onUpdate((_ctx: any, _old: any, row: any) => emit(row));
    for (const row of this.conn.db.user.iter()) emit(row);
  }

  async subscribeMembers(room: string, handlers: MemberEventHandlers): Promise<void> {
    const rec = (row: any): MemberRecord => ({
      user: row.user,
      lastReadSeq: Number(row.lastReadSeq),
      unread: Number(row.unread),
    });
    this.conn.db.member.onInsert((_ctx: any, row: any) => {
      if (row.room === room) handlers.onMember(rec(row));
    });
    this.conn.db.member.onUpdate((_ctx: any, _old: any, row: any) => {
      if (row.room === room) handlers.onMember(rec(row));
    });
    this.conn.db.member.onDelete((_ctx: any, row: any) => {
      if (row.room === room) handlers.onMemberRemoved?.(row.user);
    });
    for (const row of this.conn.db.member.iter()) {
      if (row.room === room) handlers.onMember(rec(row));
    }
  }

  // ---- snapshots ----
  async getUser(username: string): Promise<UserRecord | null> {
    for (const row of this.conn.db.user.iter()) {
      if (row.username === username) {
        return { username: row.username, status: row.status, balance: Number(row.balance) };
      }
    }
    return null;
  }

  async getRoomOwner(room: string): Promise<string | null> {
    for (const row of this.conn.db.room.iter()) {
      if (row.name === room) return row.owner;
    }
    return null;
  }

  async getMembers(room: string): Promise<MemberRecord[]> {
    const out: MemberRecord[] = [];
    for (const row of this.conn.db.member.iter()) {
      if (row.room === room) {
        out.push({ user: row.user, lastReadSeq: Number(row.lastReadSeq), unread: Number(row.unread) });
      }
    }
    return out.sort((a, b) => a.user.localeCompare(b.user));
  }

  async getMessages(room: string): Promise<MessageRecord[]> {
    await this.ensureRoomSub(room);
    const out: MessageRecord[] = [];
    for (const row of this.conn.db.message.iter()) {
      if (row.room === room) out.push(msgRec(row));
    }
    return out.sort((a, b) => a.seq - b.seq);
  }
}

export default function makeClient(): AppClient {
  return new SpacetimeAppClient();
}

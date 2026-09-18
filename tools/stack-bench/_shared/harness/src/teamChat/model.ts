// Expected-state model for the team-chat scenario (τ-bench pattern: the
// verifier is the only writer, so it can maintain the exact goal state and
// compare backend snapshots against it at quiescent points).
//
// The scenario applies an operation to the model ONLY when the backend
// accepted it (or was required to). Expected-failure operations never touch
// the model.

import type { MemberRecord, MessageRecord, UserRecord } from "../appClient.js";

interface ModelMessage {
  seq: number;
  clientMsgId: string;
  sender: string;
  text: string;
  edited: boolean;
  deleted: boolean;
}

interface ModelMember {
  lastReadSeq: number;
  unread: number;
}

interface ModelRoom {
  owner: string;
  nextSeq: number; // seq the NEXT message will get
  members: Map<string, ModelMember>;
  messages: ModelMessage[]; // seq order
  seenClientMsgIds: Set<string>;
}

export class TeamChatModel {
  users = new Map<string, { status: string; balance: number }>();
  rooms = new Map<string, ModelRoom>();

  register(username: string) {
    if (!this.users.has(username)) {
      this.users.set(username, { status: "online", balance: 100 });
    }
  }

  setStatus(username: string, status: string) {
    this.users.get(username)!.status = status;
  }

  createRoom(username: string, room: string) {
    this.rooms.set(room, {
      owner: username,
      nextSeq: 1,
      members: new Map([[username, { lastReadSeq: 0, unread: 0 }]]),
      messages: [],
      seenClientMsgIds: new Set(),
    });
  }

  joinRoom(username: string, room: string) {
    const r = this.rooms.get(room)!;
    if (r.members.has(username)) return;
    const unread = r.messages.filter((m) => m.sender !== username).length;
    r.members.set(username, { lastReadSeq: 0, unread });
  }

  removeMember(username: string, room: string) {
    this.rooms.get(room)!.members.delete(username);
  }

  /** Returns the seq assigned, or null if deduped (idempotent resend). */
  sendMessage(sender: string, room: string, text: string, clientMsgId: string): number | null {
    const r = this.rooms.get(room)!;
    if (r.seenClientMsgIds.has(clientMsgId)) return null;
    r.seenClientMsgIds.add(clientMsgId);
    const seq = r.nextSeq++;
    r.messages.push({ seq, clientMsgId, sender, text, edited: false, deleted: false });
    for (const [user, m] of r.members) {
      if (user !== sender) m.unread += 1;
    }
    return seq;
  }

  editMessage(room: string, clientMsgId: string, newText: string) {
    const m = this.msg(room, clientMsgId);
    m.text = newText;
    m.edited = true;
  }

  deleteMessage(room: string, clientMsgId: string) {
    const m = this.msg(room, clientMsgId);
    m.deleted = true;
    m.text = "";
  }

  markRead(user: string, room: string, upToSeq: number) {
    const r = this.rooms.get(room)!;
    const mem = r.members.get(user)!;
    mem.lastReadSeq = Math.max(mem.lastReadSeq, upToSeq);
    mem.unread = r.messages.filter((m) => m.seq > mem.lastReadSeq && m.sender !== user).length;
  }

  tip(fromUser: string, toUser: string, amount: number) {
    this.users.get(fromUser)!.balance -= amount;
    this.users.get(toUser)!.balance += amount;
  }

  // ---- expected snapshots ----

  expectedUser(username: string): UserRecord {
    const u = this.users.get(username)!;
    return { username, status: u.status, balance: u.balance };
  }

  expectedMembers(room: string): MemberRecord[] {
    const r = this.rooms.get(room)!;
    return [...r.members.entries()]
      .map(([user, m]) => ({ user, lastReadSeq: m.lastReadSeq, unread: m.unread }))
      .sort((a, b) => a.user.localeCompare(b.user));
  }

  expectedMessages(room: string): MessageRecord[] {
    return this.rooms.get(room)!.messages.map((m) => ({ ...m }));
  }

  nextSeq(room: string): number {
    return this.rooms.get(room)!.nextSeq;
  }

  totalBalance(...users: string[]): number {
    return users.reduce((sum, u) => sum + this.users.get(u)!.balance, 0);
  }

  private msg(room: string, clientMsgId: string): ModelMessage {
    return this.rooms.get(room)!.messages.find((m) => m.clientMsgId === clientMsgId)!;
  }
}

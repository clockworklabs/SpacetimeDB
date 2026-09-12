// The cross-backend contract for the `team-chat` task.
//
// Every backend ships an *adapter* implementing this interface with that
// backend's REAL real-time client SDK. The behavioral scenario is written only
// against this interface, so the identical grader runs on every backend.
//
// Semantics the adapter must honor (the scenario depends on these):
//   - Mutation methods resolve when the backend ACCEPTS the operation and
//     REJECT (throw) when the backend refuses it (validation/permission error).
//   - subscribeRoom delivers the room's full message history as `message` events
//     (in seq order, reflecting current edited/deleted state), then live events
//     thereafter. Message edits/deletes arrive as `message` events for the same
//     clientMsgId with updated fields.
//   - subscribeUsers mirrors the user table: one `user` event per user at
//     subscribe time, then one per change (status/balance) thereafter.
//   - subscribeMembers mirrors a room's membership: `member` events on join /
//     read-state change, and a `memberRemoved` event on leave/kick.

export interface UserRecord {
  username: string;
  status: string; // "online" | "away" | "offline"
  balance: number;
}

export interface MessageRecord {
  seq: number;
  clientMsgId: string;
  sender: string;
  text: string;
  edited: boolean;
  deleted: boolean;
}

export interface MemberRecord {
  user: string;
  lastReadSeq: number;
  unread: number;
}

export interface RoomEventHandlers {
  onMessage: (msg: MessageRecord) => void;
}

export interface UserEventHandlers {
  onUser: (user: UserRecord) => void;
}

export interface MemberEventHandlers {
  onMember: (member: MemberRecord) => void;
  onMemberRemoved?: (user: string) => void;
}

export interface AppClient {
  /** Establish a real-time connection. Rejects if the backend is unreachable. */
  connect(): Promise<void>;
  /** Tear down the connection (idempotent). */
  close(): Promise<void>;

  // ---- mutations (resolve on accept, throw on reject) ----
  register(username: string): Promise<void>;
  setStatus(username: string, status: string): Promise<void>;
  createRoom(username: string, room: string): Promise<void>;
  joinRoom(username: string, room: string): Promise<void>;
  leaveRoom(username: string, room: string): Promise<void>;
  kick(actor: string, room: string, target: string): Promise<void>;
  sendMessage(sender: string, room: string, text: string, clientMsgId: string): Promise<void>;
  editMessage(actor: string, room: string, clientMsgId: string, newText: string): Promise<void>;
  deleteMessage(actor: string, room: string, clientMsgId: string): Promise<void>;
  markRead(user: string, room: string, upToSeq: number): Promise<void>;
  tip(fromUser: string, toUser: string, amount: number): Promise<void>;

  // ---- subscriptions (push; history-then-live) ----
  subscribeRoom(room: string, handlers: RoomEventHandlers): Promise<void>;
  subscribeUsers(handlers: UserEventHandlers): Promise<void>;
  subscribeMembers(room: string, handlers: MemberEventHandlers): Promise<void>;

  // ---- point-in-time snapshot queries (used for end-state verification) ----
  getUser(username: string): Promise<UserRecord | null>;
  getRoomOwner(room: string): Promise<string | null>;
  getMembers(room: string): Promise<MemberRecord[]>;
  getMessages(room: string): Promise<MessageRecord[]>; // seq ascending
}

/** Each adapter module must default-export a factory that builds a fresh client. */
export type AppClientFactory = () => AppClient;

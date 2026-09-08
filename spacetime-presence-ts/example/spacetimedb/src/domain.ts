import { Identity } from 'spacetimedb';
import {
  SenderError,
  type ProcedureCtx,
  type ReducerCtx,
  type TransactionCtx,
} from 'spacetimedb/server';
import { getCallerUserId } from '@spacetimedb/auth/submodule';
import { consumeRateLimit } from '@spacetimedb/rate-limit/submodule';
import { removePresence, upsertPresence } from '@spacetimedb/presence';
import { PRESENCE_SCOPE_GLOBAL, typingScope } from './chat-policy';
import { ChatUserStatus } from './model';
import type { DbSchema } from './schema';

const ONE_SECOND_MICROS = 1_000_000n;
const GLOBAL_PRESENCE_TTL_SECONDS = 35;
const ACTIVITY_WINDOW_SECONDS = 5 * 60;
const ROOM_ACTIVITY_HOT_THRESHOLD = 20;
const ROOM_ACTIVITY_ACTIVE_THRESHOLD = 5;

export type Tx = ReducerCtx<DbSchema>;
type CallerCtx = ProcedureCtx<DbSchema> | ReducerCtx<DbSchema>;

export function senderError(message: string): never {
  throw new SenderError(message);
}

export function normalizeText(
  name: string,
  value: string,
  maxLength: number
): string {
  const normalized = value.trim().replace(/\s+/g, ' ');
  if (!normalized) senderError(`chat.invalid_${name}`);
  if (normalized.length > maxLength) senderError(`chat.${name}_too_long`);
  return normalized;
}

export function identityHex(identity: Identity): string {
  return identity.toHexString();
}

export function identitiesEqual(left: Identity, right: Identity): boolean {
  return left.isEqual(right);
}

export function requireAuthenticatedUserId(ctx: CallerCtx): string {
  const userId = getCallerUserId(ctx.as.auth);
  if (!userId) senderError('auth.not_authenticated');
  return userId;
}

export function enforceChatRateLimit(
  tx: Tx,
  userId: string,
  scope: string,
  limit: number,
  windowSeconds: number,
  cost = 1
): void {
  const result = consumeRateLimit(tx.as.rateLimit, {
    key: `${scope}:user:${userId}`,
    scope,
    limit,
    windowSeconds,
    cost,
  });
  if (!result.allowed)
    senderError(`chat.rate_limited:${result.retryAfterSeconds}`);
}

export function chatStatusToString(status: { tag: string }): string {
  return status.tag.toLowerCase();
}

export function ensureUser(tx: Tx, userId: string) {
  const existing = tx.db.chatUser.identity.find(tx.sender);
  const authUser = tx.db.auth.authUser.userId.find(userId);
  if (existing) {
    if (existing.userId !== userId && authUser) {
      tx.db.chatUser.identity.update({
        ...existing,
        userId,
        displayName: authUser.name ?? existing.displayName,
      });
    }
    return tx.db.chatUser.identity.find(tx.sender) ?? existing;
  }

  const hex = identityHex(tx.sender);
  const suffix = hex.slice(Math.max(0, hex.length - 6));
  const row = tx.db.chatUser.insert({
    identity: tx.sender,
    userId,
    displayName: authUser?.name ?? authUser?.email ?? `User-${suffix}`,
    status: ChatUserStatus.Online,
    createdAt: tx.timestamp,
    lastActiveAt: tx.timestamp,
    lastMessageAt: tx.timestamp,
  });
  updateGlobalPresence(tx, row);
  return row;
}

export function updateGlobalPresence(
  tx: Tx,
  user: ReturnType<typeof ensureUser>
): void {
  upsertPresence(tx, {
    scope: PRESENCE_SCOPE_GLOBAL,
    subject: identityHex(user.identity),
    status: chatStatusToString(user.status),
    payloadJson: JSON.stringify({
      displayName: user.displayName,
      userId: user.userId,
    }),
    ttlSeconds: GLOBAL_PRESENCE_TTL_SECONDS,
  });
}

export function requireRoom(tx: Tx, roomId: bigint) {
  const row = tx.db.room.id.find(roomId);
  if (!row) senderError('chat.room_not_found');
  return row;
}

export function findMembership(tx: Tx, roomId: bigint, userId: string) {
  for (const membership of tx.db.roomMember.roomId.filter(roomId)) {
    if (membership.userId === userId) return membership;
  }
  return undefined;
}

export function requireMembership(tx: Tx, roomId: bigint, userId: string) {
  const membership = findMembership(tx, roomId, userId);
  if (!membership) senderError('chat.not_room_member');
  return membership;
}

function countRecentActivityEvents(tx: Tx, roomId: bigint): number {
  const cutoff =
    tx.timestamp.microsSinceUnixEpoch -
    BigInt(ACTIVITY_WINDOW_SECONDS) * ONE_SECOND_MICROS;
  let count = 0;
  for (const event of tx.db.roomActivityEvent.roomId.filter(roomId)) {
    if (event.createdAt.microsSinceUnixEpoch >= cutoff) count++;
  }
  return count;
}

function activityLabel(score: number): string {
  if (score >= ROOM_ACTIVITY_HOT_THRESHOLD) return 'hot';
  if (score >= ROOM_ACTIVITY_ACTIVE_THRESHOLD) return 'active';
  return score > 0 ? 'warm' : 'quiet';
}

export function updateRoomActivity(tx: Tx, roomId: bigint): void {
  const room = tx.db.room.id.find(roomId);
  if (!room) return;
  const score = countRecentActivityEvents(tx, roomId);
  tx.db.room.id.update({
    ...room,
    activityScore: score,
    activityLabel: activityLabel(score),
    lastActivityAt: score > 0 ? tx.timestamp : room.lastActivityAt,
  });
}

export function deleteAttachmentWithFile(
  tx: Tx,
  attachment: { id: bigint; fileId: bigint }
): void {
  const blob = tx.db.files.fileBlob.fileId.find(attachment.fileId);
  if (blob) tx.db.files.fileBlob.delete(blob);
  const file = tx.db.files.file.id.find(attachment.fileId);
  if (file) tx.db.files.file.delete(file);
  tx.db.attachment.id.delete(attachment.id);
}

export function deleteThread(tx: Tx, threadId: bigint): void {
  for (const message of [...tx.db.threadMessage.threadId.filter(threadId)]) {
    tx.db.threadMessage.id.delete(message.id);
  }
  const thread = tx.db.messageThread.id.find(threadId);
  if (thread) tx.db.messageThread.id.delete(thread.id);
}

export function deleteMessageTree(tx: Tx, message: { id: bigint }): void {
  const thread = tx.db.messageThread.rootMessageId.find(message.id);
  if (thread) deleteThread(tx, thread.id);
  for (const attachment of [...tx.db.attachment.messageId.filter(message.id)]) {
    deleteAttachmentWithFile(tx, attachment);
  }
  for (const reaction of [
    ...tx.db.messageReaction.messageId.filter(message.id),
  ]) {
    tx.db.messageReaction.id.delete(reaction.id);
  }
  tx.db.message.id.delete(message.id);
}

export function upsertRoomReadCursor(
  tx: Tx,
  roomId: bigint,
  lastReadMessageId: bigint
): void {
  for (const cursor of tx.db.roomReadCursor.identity.filter(tx.sender)) {
    if (cursor.roomId !== roomId) continue;
    if (cursor.lastReadMessageId >= lastReadMessageId) return;
    tx.db.roomReadCursor.id.update({
      ...cursor,
      lastReadMessageId,
      lastReadAt: tx.timestamp,
    });
    return;
  }
  tx.db.roomReadCursor.insert({
    id: 0n,
    roomId,
    identity: tx.sender,
    lastReadMessageId,
    lastReadAt: tx.timestamp,
  });
}

export function removeTypingPresence(
  tx: Tx,
  roomId: bigint,
  identity: Identity
): void {
  removePresence(tx, typingScope(roomId), identityHex(identity));
}

export function insertRoom(
  tx: Tx,
  options: {
    serverId: bigint;
    name: string;
    category?: string;
    isPrivate: boolean;
    createdByUserId: string;
    role: string;
  }
) {
  const room = tx.db.room.insert({
    id: 0n,
    serverId: options.serverId,
    name: options.name,
    category: options.category,
    createdByUserId: options.createdByUserId,
    createdAt: tx.timestamp,
    isPrivate: options.isPrivate,
    activityLabel: 'quiet',
    activityScore: 0,
    lastActivityAt: undefined,
  });
  tx.db.roomMember.insert({
    id: 0n,
    roomId: room.id,
    userId: options.createdByUserId,
    role: options.role,
    joinedAt: tx.timestamp,
  });
  return room;
}

export function requireServer(tx: Tx, serverId: bigint) {
  const server = tx.db.server.id.find(serverId);
  if (!server) senderError('chat.server_not_found');
  return server;
}

export function findServerMembership(tx: Tx, serverId: bigint, userId: string) {
  for (const membership of tx.db.serverMember.serverId.filter(serverId)) {
    if (membership.userId === userId) return membership;
  }
  return undefined;
}

export function requireServerMembership(
  tx: Tx,
  serverId: bigint,
  userId: string
) {
  const membership = findServerMembership(tx, serverId, userId);
  if (!membership) senderError('chat.not_server_member');
  return membership;
}

export function requireRoomAdminOrOwner(
  tx: Tx,
  roomId: bigint,
  userId: string
) {
  const targetRoom = requireRoom(tx, roomId);
  const server = tx.db.server.id.find(targetRoom.serverId);
  if (
    server?.createdByUserId === userId ||
    targetRoom.createdByUserId === userId
  ) {
    return targetRoom;
  }
  senderError('chat.not_room_admin');
}

export function canModerateRoom(
  tx: Tx,
  roomId: bigint,
  userId: string
): boolean {
  const targetRoom = requireRoom(tx, roomId);
  const server = tx.db.server.id.find(targetRoom.serverId);
  return (
    server?.createdByUserId === userId || targetRoom.createdByUserId === userId
  );
}

export function canReadAttachmentFile(
  tx: TransactionCtx<DbSchema>,
  userId: string,
  fileId: bigint
): boolean {
  for (const attachment of tx.db.attachment.fileId.filter(fileId)) {
    const message = tx.db.message.id.find(attachment.messageId);
    if (!message) continue;
    for (const member of tx.db.roomMember.roomId.filter(message.roomId)) {
      if (member.userId === userId) return true;
    }
  }
  return false;
}

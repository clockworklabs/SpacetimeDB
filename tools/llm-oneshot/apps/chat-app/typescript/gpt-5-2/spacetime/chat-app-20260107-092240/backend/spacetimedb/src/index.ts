import { ScheduleAt } from 'spacetimedb';
import { SenderError, t } from 'spacetimedb/server';
import {
  EphemeralMessageCleanup,
  ScheduledMessageJob,
  TypingIndicatorJob,
  spacetimedb,
} from './schema';

const LIMITS = {
  displayNameMax: 32,
  roomNameMax: 50,
  messageMax: 2000,
  minSendGapMicros: 400_000n, // 0.4s
  minTypingGapMicros: 1_000_000n, // 1s
  typingTtlMicros: 4_000_000n, // 4s
  minScheduleLeadMicros: 1_000_000n, // 1s
  ephemeralMinSeconds: 10,
  ephemeralMaxSeconds: 60 * 60, // 1h
};

function nowMicros(ctx: {
  timestamp: { microsSinceUnixEpoch: bigint };
}): bigint {
  return ctx.timestamp.microsSinceUnixEpoch;
}

function normalizeText(s: string): string {
  return s.replace(/\s+/g, ' ').trim();
}

function clampInt(n: number, min: number, max: number): number {
  if (!Number.isFinite(n)) return min;
  return Math.min(max, Math.max(min, Math.trunc(n)));
}

function identityEq(
  a: { toHexString(): string },
  b: { toHexString(): string }
): boolean {
  return a.toHexString() === b.toHexString();
}

function ensureUser(ctx: any) {
  const existing = ctx.db.user.identity.find(ctx.sender);
  if (existing) return existing;

  const hex = ctx.sender.toHexString();
  const suffix = hex.slice(Math.max(0, hex.length - 6));
  const displayName = `User-${suffix}`;
  const now = nowMicros(ctx);

  return ctx.db.user.insert({
    id: 0n,
    identity: ctx.sender,
    displayName,
    online: true,
    createdAtMicros: now,
    lastSeenMicros: now,
    lastMessageMicros: 0n,
    lastTypingMicros: 0n,
  });
}

function requireRoom(ctx: any, roomId: bigint) {
  const room = ctx.db.room.id.find(roomId);
  if (!room) throw new SenderError('Room not found');
  return room;
}

function findMembership(ctx: any, roomId: bigint, identity: any) {
  for (const mem of ctx.db.roomMember.by_identity.filter(identity)) {
    if (mem.roomId === roomId) return mem;
  }
  return undefined;
}

function requireMembership(ctx: any, roomId: bigint, identity: any) {
  const mem = findMembership(ctx, roomId, identity);
  if (!mem) throw new SenderError('You must join the room first');
  return mem;
}

function upsertRoomReadPosition(
  ctx: any,
  roomId: bigint,
  messageId: bigint,
  atMicros: bigint
) {
  for (const pos of ctx.db.roomReadPosition.by_identity.filter(ctx.sender)) {
    if (pos.roomId === roomId) {
      if (
        pos.lastReadAtMicros >= atMicros &&
        pos.lastReadMessageId >= messageId
      )
        return;
      ctx.db.roomReadPosition.id.update({
        ...pos,
        lastReadMessageId: messageId,
        lastReadAtMicros: atMicros,
      });
      return;
    }
  }

  ctx.db.roomReadPosition.insert({
    id: 0n,
    roomId,
    identity: ctx.sender,
    lastReadMessageId: messageId,
    lastReadAtMicros: atMicros,
  });
}

function deleteMessageAdjuncts(ctx: any, messageId: bigint) {
  for (const rxn of ctx.db.reaction.by_message_id.filter(messageId)) {
    ctx.db.reaction.id.delete(rxn.id);
  }
  for (const rr of ctx.db.readReceipt.by_message_id.filter(messageId)) {
    ctx.db.readReceipt.id.delete(rr.id);
  }
  for (const edit of ctx.db.messageEdit.by_message_id.filter(messageId)) {
    ctx.db.messageEdit.id.delete(edit.id);
  }
}

spacetimedb.clientConnected((ctx: any) => {
  const now = nowMicros(ctx);
  const user = ensureUser(ctx);
  ctx.db.user.id.update({ ...user, online: true, lastSeenMicros: now });
});

spacetimedb.clientDisconnected((ctx: any) => {
  const now = nowMicros(ctx);
  const user = ctx.db.user.identity.find(ctx.sender);
  if (user)
    ctx.db.user.id.update({ ...user, online: false, lastSeenMicros: now });

  // Clean up typing indicators for this user.
  for (const ti of ctx.db.typingIndicator.by_identity.filter(ctx.sender)) {
    ctx.db.typingIndicator.id.delete(ti.id);
  }
});

spacetimedb.reducer(
  'set_name',
  { name: t.string() },
  (ctx: any, { name }: { name: string }) => {
    const user = ensureUser(ctx);
    const next = normalizeText(name);
    if (!next) throw new SenderError('Display name is required');
    if (next.length > LIMITS.displayNameMax)
      throw new SenderError('Display name too long');

    ctx.db.user.id.update({ ...user, displayName: next });
  }
);

spacetimedb.reducer(
  'create_room',
  { name: t.string() },
  (ctx: any, { name }: { name: string }) => {
    ensureUser(ctx);
    const roomName = normalizeText(name);
    if (!roomName) throw new SenderError('Room name is required');
    if (roomName.length > LIMITS.roomNameMax)
      throw new SenderError('Room name too long');

    const now = nowMicros(ctx);
    const room = ctx.db.room.insert({
      id: 0n,
      name: roomName,
      createdBy: ctx.sender,
      createdAtMicros: now,
    });

    ctx.db.roomMember.insert({
      id: 0n,
      roomId: room.id,
      identity: ctx.sender,
      joinedAtMicros: now,
      isAdmin: true,
    });
  }
);

spacetimedb.reducer(
  'join_room',
  { roomId: t.u64() },
  (ctx: any, { roomId }: { roomId: bigint }) => {
    ensureUser(ctx);
    requireRoom(ctx, roomId);
    const existing = findMembership(ctx, roomId, ctx.sender);
    if (existing) return;

    const now = nowMicros(ctx);
    ctx.db.roomMember.insert({
      id: 0n,
      roomId,
      identity: ctx.sender,
      joinedAtMicros: now,
      isAdmin: false,
    });
  }
);

spacetimedb.reducer(
  'leave_room',
  { roomId: t.u64() },
  (ctx: any, { roomId }: { roomId: bigint }) => {
    ensureUser(ctx);
    for (const mem of ctx.db.roomMember.by_identity.filter(ctx.sender)) {
      if (mem.roomId === roomId) ctx.db.roomMember.id.delete(mem.id);
    }
  }
);

spacetimedb.reducer(
  'send_message',
  { roomId: t.u64(), content: t.string() },
  (ctx: any, { roomId, content }: { roomId: bigint; content: string }) => {
    const user = ensureUser(ctx);
    requireRoom(ctx, roomId);
    requireMembership(ctx, roomId, ctx.sender);

    const now = nowMicros(ctx);
    if (now - user.lastMessageMicros < LIMITS.minSendGapMicros) {
      throw new SenderError('You are sending messages too quickly');
    }

    const text = normalizeText(content);
    if (!text) throw new SenderError('Message is empty');
    if (text.length > LIMITS.messageMax)
      throw new SenderError('Message too long');

    ctx.db.user.id.update({ ...user, lastMessageMicros: now });

    ctx.db.message.insert({
      id: 0n,
      roomId,
      author: ctx.sender,
      content: text,
      createdAtMicros: now,
      editedAtMicros: undefined,
      isEphemeral: false,
      expiresAtMicros: undefined,
    });
  }
);

spacetimedb.reducer(
  'send_ephemeral_message',
  { roomId: t.u64(), content: t.string(), ttlSeconds: t.u64() },
  (
    ctx: any,
    {
      roomId,
      content,
      ttlSeconds,
    }: { roomId: bigint; content: string; ttlSeconds: bigint }
  ) => {
    const user = ensureUser(ctx);
    requireRoom(ctx, roomId);
    requireMembership(ctx, roomId, ctx.sender);

    const now = nowMicros(ctx);
    if (now - user.lastMessageMicros < LIMITS.minSendGapMicros) {
      throw new SenderError('You are sending messages too quickly');
    }

    const text = normalizeText(content);
    if (!text) throw new SenderError('Message is empty');
    if (text.length > LIMITS.messageMax)
      throw new SenderError('Message too long');

    const ttl = clampInt(
      Number(ttlSeconds),
      LIMITS.ephemeralMinSeconds,
      LIMITS.ephemeralMaxSeconds
    );
    const expiresAtMicros = now + BigInt(ttl) * 1_000_000n;

    ctx.db.user.id.update({ ...user, lastMessageMicros: now });

    const msg = ctx.db.message.insert({
      id: 0n,
      roomId,
      author: ctx.sender,
      content: text,
      createdAtMicros: now,
      editedAtMicros: undefined,
      isEphemeral: true,
      expiresAtMicros,
    });

    ctx.db.ephemeralMessageCleanup.insert({
      scheduledId: 0n,
      scheduledAt: ScheduleAt.time(expiresAtMicros),
      messageId: msg.id,
    });
  }
);

spacetimedb.reducer(
  'edit_message',
  { messageId: t.u64(), newContent: t.string() },
  (
    ctx: any,
    { messageId, newContent }: { messageId: bigint; newContent: string }
  ) => {
    ensureUser(ctx);
    const msg = ctx.db.message.id.find(messageId);
    if (!msg) throw new SenderError('Message not found');
    if (!identityEq(msg.author, ctx.sender))
      throw new SenderError('You can only edit your own messages');
    if (msg.isEphemeral)
      throw new SenderError('Ephemeral messages cannot be edited');

    const text = normalizeText(newContent);
    if (!text) throw new SenderError('Message is empty');
    if (text.length > LIMITS.messageMax)
      throw new SenderError('Message too long');

    if (text === msg.content) return;
    const now = nowMicros(ctx);

    ctx.db.messageEdit.insert({
      id: 0n,
      messageId: msg.id,
      editor: ctx.sender,
      oldContent: msg.content,
      newContent: text,
      editedAtMicros: now,
    });

    ctx.db.message.id.update({ ...msg, content: text, editedAtMicros: now });
  }
);

spacetimedb.reducer(
  'schedule_message',
  { roomId: t.u64(), content: t.string(), scheduledAtMicros: t.u64() },
  (
    ctx: any,
    {
      roomId,
      content,
      scheduledAtMicros,
    }: { roomId: bigint; content: string; scheduledAtMicros: bigint }
  ) => {
    ensureUser(ctx);
    requireRoom(ctx, roomId);
    requireMembership(ctx, roomId, ctx.sender);

    const now = nowMicros(ctx);
    if (scheduledAtMicros <= now + LIMITS.minScheduleLeadMicros) {
      throw new SenderError('Scheduled time must be in the future');
    }

    const text = normalizeText(content);
    if (!text) throw new SenderError('Message is empty');
    if (text.length > LIMITS.messageMax)
      throw new SenderError('Message too long');

    const sm = ctx.db.scheduledMessage.insert({
      id: 0n,
      roomId,
      author: ctx.sender,
      content: text,
      createdAtMicros: now,
      scheduledAtMicros,
      jobId: 0n,
    });

    const job = ctx.db.scheduledMessageJob.insert({
      scheduledId: 0n,
      scheduledAt: ScheduleAt.time(scheduledAtMicros),
      scheduledMessageId: sm.id,
    });

    ctx.db.scheduledMessage.id.update({ ...sm, jobId: job.scheduledId });
  }
);

spacetimedb.reducer(
  'cancel_scheduled_message',
  { scheduledMessageId: t.u64() },
  (ctx: any, { scheduledMessageId }: { scheduledMessageId: bigint }) => {
    ensureUser(ctx);
    const sm = ctx.db.scheduledMessage.id.find(scheduledMessageId);
    if (!sm) return;
    if (!identityEq(sm.author, ctx.sender))
      throw new SenderError('You can only cancel your own scheduled messages');

    ctx.db.scheduledMessageJob.scheduledId.delete(sm.jobId);
    ctx.db.scheduledMessage.id.delete(sm.id);
  }
);

spacetimedb.reducer(
  'send_scheduled_message',
  { arg: ScheduledMessageJob.rowType },
  (ctx: any, { arg }: { arg: any }) => {
    const now = nowMicros(ctx);
    const sm = ctx.db.scheduledMessage.id.find(arg.scheduledMessageId);
    if (!sm) return;

    // If room was deleted, just drop the scheduled message.
    const room = ctx.db.room.id.find(sm.roomId);
    if (!room) {
      ctx.db.scheduledMessage.id.delete(sm.id);
      return;
    }

    ctx.db.message.insert({
      id: 0n,
      roomId: sm.roomId,
      author: sm.author,
      content: sm.content,
      createdAtMicros: now,
      editedAtMicros: undefined,
      isEphemeral: false,
      expiresAtMicros: undefined,
    });

    ctx.db.scheduledMessage.id.delete(sm.id);
  }
);

spacetimedb.reducer(
  'delete_ephemeral_message',
  { arg: EphemeralMessageCleanup.rowType },
  (ctx: any, { arg }: { arg: any }) => {
    const msg = ctx.db.message.id.find(arg.messageId);
    if (!msg) return;
    if (!msg.isEphemeral) return;

    deleteMessageAdjuncts(ctx, msg.id);
    ctx.db.message.id.delete(msg.id);
  }
);

spacetimedb.reducer(
  'start_typing',
  { roomId: t.u64() },
  (ctx: any, { roomId }: { roomId: bigint }) => {
    const user = ensureUser(ctx);
    requireRoom(ctx, roomId);
    requireMembership(ctx, roomId, ctx.sender);

    const now = nowMicros(ctx);
    if (now - user.lastTypingMicros < LIMITS.minTypingGapMicros) return;
    ctx.db.user.id.update({ ...user, lastTypingMicros: now });

    for (const ti of ctx.db.typingIndicator.by_identity.filter(ctx.sender)) {
      ctx.db.typingIndicator.id.delete(ti.id);
    }

    const expiresAtMicros = now + LIMITS.typingTtlMicros;
    const ti = ctx.db.typingIndicator.insert({
      id: 0n,
      roomId,
      identity: ctx.sender,
      expiresAtMicros,
    });

    ctx.db.typingIndicatorJob.insert({
      scheduledId: 0n,
      scheduledAt: ScheduleAt.time(expiresAtMicros),
      typingIndicatorId: ti.id,
      expiresAtMicros,
    });
  }
);

spacetimedb.reducer(
  'expire_typing_indicator',
  { arg: TypingIndicatorJob.rowType },
  (ctx: any, { arg }: { arg: any }) => {
    const ti = ctx.db.typingIndicator.id.find(arg.typingIndicatorId);
    if (!ti) return;
    if (ti.expiresAtMicros !== arg.expiresAtMicros) return;
    ctx.db.typingIndicator.id.delete(ti.id);
  }
);

spacetimedb.reducer(
  'mark_message_read',
  { messageId: t.u64() },
  (ctx: any, { messageId }: { messageId: bigint }) => {
    ensureUser(ctx);
    const msg = ctx.db.message.id.find(messageId);
    if (!msg) throw new SenderError('Message not found');
    requireMembership(ctx, msg.roomId, ctx.sender);
    const now = nowMicros(ctx);

    for (const rr of ctx.db.readReceipt.by_message_id.filter(messageId)) {
      if (identityEq(rr.identity, ctx.sender)) {
        upsertRoomReadPosition(ctx, msg.roomId, msg.id, now);
        return;
      }
    }

    ctx.db.readReceipt.insert({
      id: 0n,
      messageId: msg.id,
      identity: ctx.sender,
      readAtMicros: now,
    });

    upsertRoomReadPosition(ctx, msg.roomId, msg.id, now);
  }
);

spacetimedb.reducer(
  'mark_room_read',
  { roomId: t.u64() },
  (ctx: any, { roomId }: { roomId: bigint }) => {
    ensureUser(ctx);
    requireRoom(ctx, roomId);
    requireMembership(ctx, roomId, ctx.sender);
    const now = nowMicros(ctx);

    let latestId = 0n;
    let latestAt = 0n;
    for (const msg of ctx.db.message.by_room_id.filter(roomId)) {
      if (msg.createdAtMicros > latestAt) {
        latestAt = msg.createdAtMicros;
        latestId = msg.id;
      }
    }

    // Record per-message read receipts so "Seen by" works without the client
    // needing to call a reducer for every single message.
    for (const msg of ctx.db.message.by_room_id.filter(roomId)) {
      let already = false;
      for (const rr of ctx.db.readReceipt.by_message_id.filter(msg.id)) {
        if (identityEq(rr.identity, ctx.sender)) {
          already = true;
          break;
        }
      }
      if (!already) {
        ctx.db.readReceipt.insert({
          id: 0n,
          messageId: msg.id,
          identity: ctx.sender,
          readAtMicros: now,
        });
      }
    }

    upsertRoomReadPosition(ctx, roomId, latestId, latestAt ? latestAt : now);
  }
);

const ALLOWED_EMOJIS = new Set(['👍', '❤️', '😂', '😮', '😢']);

spacetimedb.reducer(
  'toggle_reaction',
  { messageId: t.u64(), emoji: t.string() },
  (ctx: any, { messageId, emoji }: { messageId: bigint; emoji: string }) => {
    ensureUser(ctx);
    const msg = ctx.db.message.id.find(messageId);
    if (!msg) throw new SenderError('Message not found');
    requireMembership(ctx, msg.roomId, ctx.sender);

    const e = emoji.trim();
    if (!ALLOWED_EMOJIS.has(e)) throw new SenderError('Invalid emoji');

    for (const rxn of ctx.db.reaction.by_message_id.filter(messageId)) {
      if (identityEq(rxn.identity, ctx.sender) && rxn.emoji === e) {
        ctx.db.reaction.id.delete(rxn.id);
        return;
      }
    }

    const now = nowMicros(ctx);
    ctx.db.reaction.insert({
      id: 0n,
      messageId: msg.id,
      identity: ctx.sender,
      emoji: e,
      createdAtMicros: now,
    });
  }
);

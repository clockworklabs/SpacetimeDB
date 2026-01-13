import http from 'node:http';
import crypto from 'node:crypto';
import express from 'express';
import cors from 'cors';
import { Server } from 'socket.io';
import { and, desc, eq, inArray, isNull } from 'drizzle-orm';

import { authMiddleware, signToken, tokenFromSocketAuth, verifyToken } from './auth';
import { db } from './db';
import {
  messageEdits,
  messages,
  reactions,
  roomMembers,
  roomReadPositions,
  rooms,
  scheduledMessages,
  users,
} from './db/schema';
import { startJobs } from './jobs';
import { createRealtime } from './realtime';
import { ClientError, assertInt, nonEmptyTrimmed } from './validate';

const PORT = Number(process.env.PORT || 3001);
const CLIENT_ORIGIN = process.env.CLIENT_ORIGIN || 'http://localhost:5173';

const app = express();
app.use(
  cors({
    origin: CLIENT_ORIGIN,
    credentials: true,
  }),
);
app.use(express.json({ limit: '1mb' }));

app.get('/health', (_req, res) => res.json({ ok: true }));

app.post('/auth/login', async (req, res) => {
  try {
    const displayName = nonEmptyTrimmed('displayName', req.body?.displayName, 40);
    const userId = crypto.randomUUID();
    await db.insert(users).values({ id: userId, displayName, isOnline: false });
    const token = signToken({ userId });
    return res.json({ token, user: { id: userId, displayName } });
  } catch (e) {
    const err = e instanceof ClientError ? e : new ClientError(500, 'Server error');
    return res.status(err.status).json({ error: err.message });
  }
});

app.get('/me', authMiddleware, async (req, res) => {
  const userId = (req as any).userId as string;
  const user = await db.select().from(users).where(eq(users.id, userId)).limit(1);
  if (!user[0]) return res.status(401).json({ error: 'Unauthorized' });
  return res.json({ user: { id: user[0].id, displayName: user[0].displayName } });
});

app.patch('/me', authMiddleware, async (req, res) => {
  try {
    const userId = (req as any).userId as string;
    const displayName = nonEmptyTrimmed('displayName', req.body?.displayName, 40);
    await db.update(users).set({ displayName, lastActiveAt: new Date() }).where(eq(users.id, userId));
    return res.json({ ok: true });
  } catch (e) {
    const err = e instanceof ClientError ? e : new ClientError(500, 'Server error');
    return res.status(err.status).json({ error: err.message });
  }
});

app.get('/rooms', authMiddleware, async (req, res) => {
  const userId = (req as any).userId as string;
  const memberships = await db
    .select({ roomId: roomMembers.roomId })
    .from(roomMembers)
    .where(eq(roomMembers.userId, userId));
  const roomIds = memberships.map((m) => m.roomId);
  const roomRows =
    roomIds.length === 0
      ? []
      : await db
          .select()
          .from(rooms)
          .where(inArray(rooms.id, roomIds))
          .orderBy(desc(rooms.id));

  const readRows =
    roomIds.length === 0
      ? []
      : await db
          .select()
          .from(roomReadPositions)
          .where(and(eq(roomReadPositions.userId, userId), inArray(roomReadPositions.roomId, roomIds)));

  const lastReadByRoom = new Map<number, number | null>();
  for (const r of readRows) lastReadByRoom.set(r.roomId, r.lastReadMessageId ?? null);

  return res.json({
    rooms: roomRows.map((r) => ({
      id: r.id,
      name: r.name,
      lastReadMessageId: lastReadByRoom.get(r.id) ?? null,
    })),
  });
});

app.post('/rooms', authMiddleware, async (req, res) => {
  try {
    const userId = (req as any).userId as string;
    const name = nonEmptyTrimmed('name', req.body?.name, 64);
    const [room] = await db.insert(rooms).values({ name, createdBy: userId }).returning();
    await db.insert(roomMembers).values({ roomId: room.id, userId });
    realtime.broadcastRoomsChanged();
    realtime.broadcastRoomMembersChanged(room.id);
    return res.json({ room: { id: room.id, name: room.name } });
  } catch (e) {
    const err = e instanceof ClientError ? e : new ClientError(500, 'Server error');
    return res.status(err.status).json({ error: err.message });
  }
});

app.post('/rooms/:roomId/join', authMiddleware, async (req, res) => {
  const userId = (req as any).userId as string;
  const roomId = assertInt('roomId', req.params.roomId);
  const existing = await db
    .select()
    .from(roomMembers)
    .where(and(eq(roomMembers.userId, userId), eq(roomMembers.roomId, roomId)))
    .limit(1);
  if (existing[0]) return res.json({ ok: true });

  await db.insert(roomMembers).values({ roomId, userId });
  realtime.broadcastRoomsChanged();
  realtime.broadcastRoomMembersChanged(roomId);
  return res.json({ ok: true });
});

app.post('/rooms/:roomId/leave', authMiddleware, async (req, res) => {
  const userId = (req as any).userId as string;
  const roomId = assertInt('roomId', req.params.roomId);
  await db
    .delete(roomMembers)
    .where(and(eq(roomMembers.userId, userId), eq(roomMembers.roomId, roomId)));
  realtime.broadcastRoomsChanged();
  realtime.broadcastRoomMembersChanged(roomId);
  return res.json({ ok: true });
});

app.get('/rooms/:roomId/members', authMiddleware, async (req, res) => {
  const userId = (req as any).userId as string;
  const roomId = assertInt('roomId', req.params.roomId);

  // Must be a member to view.
  const myMembership = await db
    .select()
    .from(roomMembers)
    .where(and(eq(roomMembers.userId, userId), eq(roomMembers.roomId, roomId)))
    .limit(1);
  if (!myMembership[0]) return res.status(403).json({ error: 'Forbidden' });

  const memberRows = await db
    .select({ userId: roomMembers.userId, displayName: users.displayName, isOnline: users.isOnline })
    .from(roomMembers)
    .innerJoin(users, eq(users.id, roomMembers.userId))
    .where(eq(roomMembers.roomId, roomId));

  const readRows = await db
    .select()
    .from(roomReadPositions)
    .where(eq(roomReadPositions.roomId, roomId));
  const lastReadByUser = new Map<string, number | null>();
  for (const r of readRows) lastReadByUser.set(r.userId, r.lastReadMessageId ?? null);

  return res.json({
    members: memberRows.map((m) => ({
      id: m.userId,
      displayName: m.displayName,
      isOnline: m.isOnline,
      lastReadMessageId: lastReadByUser.get(m.userId) ?? null,
    })),
  });
});

app.get('/rooms/:roomId/messages', authMiddleware, async (req, res) => {
  const userId = (req as any).userId as string;
  const roomId = assertInt('roomId', req.params.roomId);

  const myMembership = await db
    .select()
    .from(roomMembers)
    .where(and(eq(roomMembers.userId, userId), eq(roomMembers.roomId, roomId)))
    .limit(1);
  if (!myMembership[0]) return res.status(403).json({ error: 'Forbidden' });

  const rows = await db
    .select({
      id: messages.id,
      roomId: messages.roomId,
      authorId: messages.authorId,
      content: messages.content,
      createdAt: messages.createdAt,
      updatedAt: messages.updatedAt,
      expiresAt: messages.expiresAt,
      authorName: users.displayName,
    })
    .from(messages)
    .innerJoin(users, eq(users.id, messages.authorId))
    .where(eq(messages.roomId, roomId))
    .orderBy(desc(messages.id))
    .limit(100);

  const msgs = [...rows].reverse();
  const messageIds = msgs.map((m) => m.id);

  const reactionRows =
    messageIds.length === 0
      ? []
      : await db
          .select({ messageId: reactions.messageId, emoji: reactions.emoji, userId: reactions.userId })
          .from(reactions)
          .where(inArray(reactions.messageId, messageIds));

  const editRows =
    messageIds.length === 0
      ? []
      : await db
          .select({ messageId: messageEdits.messageId })
          .from(messageEdits)
          .where(inArray(messageEdits.messageId, messageIds));

  const editsCount = new Map<number, number>();
  for (const e of editRows) editsCount.set(e.messageId, (editsCount.get(e.messageId) || 0) + 1);

  const reactionsByMessage = new Map<number, { emoji: string; userIds: string[] }[]>();
  for (const r of reactionRows) {
    const arr = reactionsByMessage.get(r.messageId) || [];
    const existing = arr.find((x) => x.emoji === r.emoji);
    if (existing) existing.userIds.push(r.userId);
    else arr.push({ emoji: r.emoji, userIds: [r.userId] });
    reactionsByMessage.set(r.messageId, arr);
  }

  return res.json({
    messages: msgs.map((m) => ({
      id: m.id,
      roomId: m.roomId,
      authorId: m.authorId,
      authorName: m.authorName,
      content: m.content,
      createdAt: m.createdAt,
      updatedAt: m.updatedAt,
      expiresAt: m.expiresAt,
      edited: !!m.updatedAt,
      editCount: editsCount.get(m.id) || 0,
      reactions: reactionsByMessage.get(m.id) || [],
    })),
  });
});

app.post('/rooms/:roomId/messages', authMiddleware, async (req, res) => {
  try {
    const userId = (req as any).userId as string;
    const roomId = assertInt('roomId', req.params.roomId);

    const myMembership = await db
      .select()
      .from(roomMembers)
      .where(and(eq(roomMembers.userId, userId), eq(roomMembers.roomId, roomId)))
      .limit(1);
    if (!myMembership[0]) return res.status(403).json({ error: 'Forbidden' });

    const content = nonEmptyTrimmed('content', req.body?.content, 2000);

    // simple spam prevention: 1 message per second
    const me = await db.select().from(users).where(eq(users.id, userId)).limit(1);
    const lastMessageAt = me[0]?.lastMessageAt ? new Date(me[0].lastMessageAt) : null;
    if (lastMessageAt && Date.now() - lastMessageAt.getTime() < 900) {
      return res.status(429).json({ error: 'Too many messages' });
    }

    const scheduleInSeconds =
      typeof req.body?.scheduleInSeconds === 'number' ? req.body.scheduleInSeconds : null;
    const scheduleAt =
      typeof req.body?.scheduleAt === 'string' ? new Date(req.body.scheduleAt) : null;

    if (scheduleInSeconds || (scheduleAt && Number.isFinite(scheduleAt.getTime()))) {
      const sendAt = scheduleAt && Number.isFinite(scheduleAt.getTime())
        ? scheduleAt
        : new Date(Date.now() + Math.max(5, Number(scheduleInSeconds || 30)) * 1000);

      const [scheduled] = await db
        .insert(scheduledMessages)
        .values({ roomId, authorId: userId, content, sendAt })
        .returning();
      realtime.broadcastRoomsChanged();
      io.emit('scheduled:changed', { userId });
      return res.json({ scheduled });
    }

    const ephemeralSeconds =
      typeof req.body?.ephemeralSeconds === 'number' ? req.body.ephemeralSeconds : null;

    const expiresAt =
      ephemeralSeconds && ephemeralSeconds > 0
        ? new Date(Date.now() + Math.min(60 * 60, ephemeralSeconds) * 1000)
        : null;

    const [msg] = await db
      .insert(messages)
      .values({ roomId, authorId: userId, content, expiresAt })
      .returning();
    await db.update(users).set({ lastMessageAt: new Date() }).where(eq(users.id, userId));

    realtime.broadcastMessageCreated(roomId, msg.id);
    return res.json({ messageId: msg.id });
  } catch (e) {
    const err = e instanceof ClientError ? e : new ClientError(500, 'Server error');
    return res.status(err.status).json({ error: err.message });
  }
});

app.post('/rooms/:roomId/read', authMiddleware, async (req, res) => {
  try {
    const userId = (req as any).userId as string;
    const roomId = assertInt('roomId', req.params.roomId);
    const lastReadMessageId = req.body?.lastReadMessageId === null ? null : assertInt('lastReadMessageId', req.body?.lastReadMessageId);

    await db
      .insert(roomReadPositions)
      .values({ roomId, userId, lastReadMessageId, updatedAt: new Date() })
      .onConflictDoUpdate({
        target: [roomReadPositions.roomId, roomReadPositions.userId],
        set: { lastReadMessageId, updatedAt: new Date() },
      });
    realtime.broadcastReadPositionChanged(roomId, userId);
    return res.json({ ok: true });
  } catch (e) {
    const err = e instanceof ClientError ? e : new ClientError(500, 'Server error');
    return res.status(err.status).json({ error: err.message });
  }
});

app.patch('/messages/:messageId', authMiddleware, async (req, res) => {
  try {
    const userId = (req as any).userId as string;
    const messageId = assertInt('messageId', req.params.messageId);
    const newContent = nonEmptyTrimmed('content', req.body?.content, 2000);

    const [msg] = await db.select().from(messages).where(eq(messages.id, messageId)).limit(1);
    if (!msg) return res.status(404).json({ error: 'Not found' });
    if (msg.authorId !== userId) return res.status(403).json({ error: 'Forbidden' });

    await db
      .insert(messageEdits)
      .values({ messageId: msg.id, editorId: userId, oldContent: msg.content, newContent })
      .returning();

    await db.update(messages).set({ content: newContent, updatedAt: new Date() }).where(eq(messages.id, msg.id));
    realtime.broadcastMessageUpdated(msg.roomId, msg.id);
    return res.json({ ok: true });
  } catch (e) {
    const err = e instanceof ClientError ? e : new ClientError(500, 'Server error');
    return res.status(err.status).json({ error: err.message });
  }
});

app.get('/messages/:messageId/history', authMiddleware, async (req, res) => {
  const userId = (req as any).userId as string;
  const messageId = assertInt('messageId', req.params.messageId);

  const [msg] = await db.select().from(messages).where(eq(messages.id, messageId)).limit(1);
  if (!msg) return res.status(404).json({ error: 'Not found' });

  const myMembership = await db
    .select()
    .from(roomMembers)
    .where(and(eq(roomMembers.userId, userId), eq(roomMembers.roomId, msg.roomId)))
    .limit(1);
  if (!myMembership[0]) return res.status(403).json({ error: 'Forbidden' });

  const edits = await db
    .select({ oldContent: messageEdits.oldContent, newContent: messageEdits.newContent, editedAt: messageEdits.editedAt })
    .from(messageEdits)
    .where(eq(messageEdits.messageId, msg.id))
    .orderBy(messageEdits.id);

  const versions: { content: string; label: string }[] = [];
  if (edits.length === 0) versions.push({ content: msg.content, label: 'Original' });
  else {
    versions.push({ content: edits[0].oldContent, label: 'Original' });
    for (let i = 0; i < edits.length; i++) {
      versions.push({ content: edits[i].newContent, label: `Edit ${i + 1}` });
    }
  }

  return res.json({ versions });
});

app.post('/messages/:messageId/reactions', authMiddleware, async (req, res) => {
  try {
    const userId = (req as any).userId as string;
    const messageId = assertInt('messageId', req.params.messageId);
    const emoji = nonEmptyTrimmed('emoji', req.body?.emoji, 16);

    const [msg] = await db.select().from(messages).where(eq(messages.id, messageId)).limit(1);
    if (!msg) return res.status(404).json({ error: 'Not found' });

    const myMembership = await db
      .select()
      .from(roomMembers)
      .where(and(eq(roomMembers.userId, userId), eq(roomMembers.roomId, msg.roomId)))
      .limit(1);
    if (!myMembership[0]) return res.status(403).json({ error: 'Forbidden' });

    const existing = await db
      .select()
      .from(reactions)
      .where(and(eq(reactions.messageId, messageId), eq(reactions.userId, userId), eq(reactions.emoji, emoji)))
      .limit(1);

    if (existing[0]) {
      await db
        .delete(reactions)
        .where(and(eq(reactions.messageId, messageId), eq(reactions.userId, userId), eq(reactions.emoji, emoji)));
    } else {
      await db.insert(reactions).values({ messageId, userId, emoji });
    }

    realtime.broadcastReactionsChanged(messageId);
    return res.json({ ok: true });
  } catch (e) {
    const err = e instanceof ClientError ? e : new ClientError(500, 'Server error');
    return res.status(err.status).json({ error: err.message });
  }
});

app.get('/scheduled', authMiddleware, async (req, res) => {
  const userId = (req as any).userId as string;
  const rows = await db
    .select({
      id: scheduledMessages.id,
      roomId: scheduledMessages.roomId,
      content: scheduledMessages.content,
      sendAt: scheduledMessages.sendAt,
      roomName: rooms.name,
    })
    .from(scheduledMessages)
    .innerJoin(rooms, eq(rooms.id, scheduledMessages.roomId))
    .where(
      and(eq(scheduledMessages.authorId, userId), isNull(scheduledMessages.cancelledAt), isNull(scheduledMessages.sentAt)),
    )
    .orderBy(desc(scheduledMessages.id))
    .limit(50);
  return res.json({ scheduled: rows });
});

app.delete('/scheduled/:scheduledId', authMiddleware, async (req, res) => {
  try {
    const userId = (req as any).userId as string;
    const scheduledId = assertInt('scheduledId', req.params.scheduledId);

    const existing = await db
      .select()
      .from(scheduledMessages)
      .where(and(eq(scheduledMessages.id, scheduledId), eq(scheduledMessages.authorId, userId)))
      .limit(1);
    if (!existing[0]) return res.status(404).json({ error: 'Not found' });

    await db.update(scheduledMessages).set({ cancelledAt: new Date() }).where(eq(scheduledMessages.id, scheduledId));
    io.emit('scheduled:changed', { userId });
    return res.json({ ok: true });
  } catch (e) {
    const err = e instanceof ClientError ? e : new ClientError(500, 'Server error');
    return res.status(err.status).json({ error: err.message });
  }
});

const httpServer = http.createServer(app);
const io = new Server(httpServer, {
  cors: { origin: CLIENT_ORIGIN, credentials: true },
});

const realtime = createRealtime(io, db);
startJobs(db, realtime);

io.use(async (socket, next) => {
  try {
    const token = tokenFromSocketAuth(socket.handshake.auth);
    if (!token) return next(new Error('Unauthorized'));
    const payload = verifyToken(token);
    (socket as any).userId = payload.userId;
    return next();
  } catch {
    return next(new Error('Unauthorized'));
  }
});

io.on('connection', async (socket) => {
  const userId = (socket as any).userId as string;
  const [user] = await db.select().from(users).where(eq(users.id, userId)).limit(1);
  if (!user) {
    socket.disconnect(true);
    return;
  }
  realtime.onSocketAuthed(socket, userId, user.displayName);
});

httpServer.listen(PORT, () => {
  // eslint-disable-next-line no-console
  console.log(`server listening on http://localhost:${PORT}`);
});


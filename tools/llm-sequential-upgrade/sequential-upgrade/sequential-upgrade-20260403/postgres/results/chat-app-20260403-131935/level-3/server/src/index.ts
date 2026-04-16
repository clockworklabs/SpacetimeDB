import 'dotenv/config';
import express from 'express';
import { createServer } from 'http';
import { Server } from 'socket.io';
import cors from 'cors';
import { drizzle } from 'drizzle-orm/node-postgres';
import pg from 'pg';
import { eq, and, desc, gt, count, sql, isNull, lte, isNotNull } from 'drizzle-orm';
import * as schema from './schema.js';

const { Pool } = pg;

const pool = new Pool({ connectionString: process.env.DATABASE_URL });
const db = drizzle(pool, { schema });

const app = express();
const httpServer = createServer(app);

const io = new Server(httpServer, {
  cors: { origin: 'http://localhost:6273', credentials: true },
});

app.use(cors({ origin: 'http://localhost:6273', credentials: true }));
app.use(express.json());

// In-memory: socket -> user mapping, and per-room typing state
const socketToUser = new Map<string, { userId: number; username: string }>();
const onlineUsers = new Map<number, { username: string; socketId: string }>();
// roomId -> Map<userId, timeout>
const typingTimers = new Map<number, Map<number, ReturnType<typeof setTimeout>>>();

// ── REST Routes ─────────────────────────────────────────────────────────────

// Users
app.post('/api/users', async (req, res) => {
  const { username } = req.body as { username?: string };
  if (!username || username.trim().length < 1 || username.trim().length > 32) {
    return res.status(400).json({ error: 'Username must be 1-32 characters' });
  }
  const name = username.trim();
  try {
    const existing = await db.select().from(schema.users).where(eq(schema.users.username, name));
    if (existing.length > 0) return res.json(existing[0]);
    const [user] = await db.insert(schema.users).values({ username: name }).returning();
    return res.json(user);
  } catch (err) {
    return res.status(500).json({ error: 'Failed to create user' });
  }
});

app.get('/api/users', async (_req, res) => {
  const users = await db.select().from(schema.users);
  return res.json(users);
});

// Rooms
app.get('/api/rooms', async (_req, res) => {
  const rooms = await db.select().from(schema.rooms).orderBy(schema.rooms.createdAt);
  return res.json(rooms);
});

app.post('/api/rooms', async (req, res) => {
  const { name } = req.body as { name?: string };
  if (!name || name.trim().length < 1 || name.trim().length > 64) {
    return res.status(400).json({ error: 'Room name must be 1-64 characters' });
  }
  const roomName = name.trim();
  try {
    const existing = await db.select().from(schema.rooms).where(eq(schema.rooms.name, roomName));
    if (existing.length > 0) return res.json(existing[0]);
    const [room] = await db.insert(schema.rooms).values({ name: roomName }).returning();
    io.emit('room_created', room);
    return res.json(room);
  } catch (err) {
    return res.status(500).json({ error: 'Failed to create room' });
  }
});

// Join / Leave room
app.post('/api/rooms/:roomId/join', async (req, res) => {
  const roomId = parseInt(req.params.roomId);
  const { userId } = req.body as { userId?: number };
  if (!userId) return res.status(400).json({ error: 'userId required' });
  try {
    const existing = await db.select().from(schema.roomMembers)
      .where(and(eq(schema.roomMembers.userId, userId), eq(schema.roomMembers.roomId, roomId)));
    if (existing.length === 0) {
      await db.insert(schema.roomMembers).values({ userId, roomId });
    }
    return res.json({ ok: true });
  } catch (err) {
    return res.status(500).json({ error: 'Failed to join room' });
  }
});

app.post('/api/rooms/:roomId/leave', async (req, res) => {
  const roomId = parseInt(req.params.roomId);
  const { userId } = req.body as { userId?: number };
  if (!userId) return res.status(400).json({ error: 'userId required' });
  try {
    await db.delete(schema.roomMembers)
      .where(and(eq(schema.roomMembers.userId, userId), eq(schema.roomMembers.roomId, roomId)));
    return res.json({ ok: true });
  } catch (err) {
    return res.status(500).json({ error: 'Failed to leave room' });
  }
});

app.get('/api/rooms/:roomId/members', async (req, res) => {
  const roomId = parseInt(req.params.roomId);
  const members = await db.select({ userId: schema.roomMembers.userId })
    .from(schema.roomMembers)
    .where(eq(schema.roomMembers.roomId, roomId));
  return res.json(members.map(m => m.userId));
});

// Messages
app.get('/api/rooms/:roomId/messages', async (req, res) => {
  const roomId = parseInt(req.params.roomId);
  const now = new Date();
  const msgs = await db.select({
    id: schema.messages.id,
    roomId: schema.messages.roomId,
    userId: schema.messages.userId,
    content: schema.messages.content,
    createdAt: schema.messages.createdAt,
    expiresAt: schema.messages.expiresAt,
    username: schema.users.username,
  })
    .from(schema.messages)
    .innerJoin(schema.users, eq(schema.messages.userId, schema.users.id))
    .where(and(
      eq(schema.messages.roomId, roomId),
      sql`(${schema.messages.expiresAt} IS NULL OR ${schema.messages.expiresAt} > ${now})`
    ))
    .orderBy(schema.messages.createdAt)
    .limit(200);
  return res.json(msgs);
});

app.post('/api/rooms/:roomId/messages', async (req, res) => {
  const roomId = parseInt(req.params.roomId);
  const { userId, content, expiresInSeconds } = req.body as { userId?: number; content?: string; expiresInSeconds?: number };
  if (!userId || !content || content.trim().length === 0) {
    return res.status(400).json({ error: 'userId and content required' });
  }
  if (content.trim().length > 2000) {
    return res.status(400).json({ error: 'Message too long (max 2000 chars)' });
  }
  let expiresAt: Date | null = null;
  if (expiresInSeconds && expiresInSeconds > 0) {
    expiresAt = new Date(Date.now() + expiresInSeconds * 1000);
  }
  try {
    const [msg] = await db.insert(schema.messages)
      .values({ roomId, userId, content: content.trim(), ...(expiresAt ? { expiresAt } : {}) })
      .returning();
    const [user] = await db.select().from(schema.users).where(eq(schema.users.id, userId));
    const fullMsg = { ...msg, username: user.username };
    io.to(`room:${roomId}`).emit('new_message', fullMsg);
    return res.json(fullMsg);
  } catch (err) {
    return res.status(500).json({ error: 'Failed to send message' });
  }
});

// Read receipts
app.post('/api/messages/read', async (req, res) => {
  const { userId, roomId, messageId } = req.body as { userId?: number; roomId?: number; messageId?: number };
  if (!userId || !roomId || !messageId) {
    return res.status(400).json({ error: 'userId, roomId, messageId required' });
  }
  try {
    // Upsert last read position
    await db.insert(schema.userRoomLastRead)
      .values({ userId, roomId, lastReadMessageId: messageId })
      .onConflictDoUpdate({
        target: [schema.userRoomLastRead.userId, schema.userRoomLastRead.roomId],
        set: { lastReadMessageId: messageId, updatedAt: new Date() },
      });

    // Get all messages up to messageId in this room
    const msgs = await db.select({ id: schema.messages.id })
      .from(schema.messages)
      .where(and(eq(schema.messages.roomId, roomId), sql`${schema.messages.id} <= ${messageId}`));

    // Insert read receipts for all unread messages
    for (const msg of msgs) {
      await db.insert(schema.messageReads)
        .values({ userId, messageId: msg.id })
        .onConflictDoNothing();
    }

    // Fetch who read the latest message
    const readers = await db.select({ userId: schema.messageReads.userId, username: schema.users.username })
      .from(schema.messageReads)
      .innerJoin(schema.users, eq(schema.messageReads.userId, schema.users.id))
      .where(eq(schema.messageReads.messageId, messageId));

    io.to(`room:${roomId}`).emit('read_receipt_update', { messageId, readers });

    return res.json({ ok: true });
  } catch (err) {
    return res.status(500).json({ error: 'Failed to mark as read' });
  }
});

app.get('/api/rooms/:roomId/read-receipts', async (req, res) => {
  const roomId = parseInt(req.params.roomId);
  const { userId } = req.query as { userId?: string };
  if (!userId) return res.status(400).json({ error: 'userId required' });

  // For each message in the room, who has read it
  const msgs = await db.select({ id: schema.messages.id })
    .from(schema.messages)
    .where(eq(schema.messages.roomId, roomId));

  const result: Record<number, { userId: number; username: string }[]> = {};
  for (const msg of msgs) {
    const readers = await db.select({ userId: schema.messageReads.userId, username: schema.users.username })
      .from(schema.messageReads)
      .innerJoin(schema.users, eq(schema.messageReads.userId, schema.users.id))
      .where(eq(schema.messageReads.messageId, msg.id));
    result[msg.id] = readers;
  }
  return res.json(result);
});

// Unread counts per room for a user
app.get('/api/users/:userId/unread', async (req, res) => {
  const userId = parseInt(req.params.userId);

  // Get all rooms user is a member of
  const memberships = await db.select({ roomId: schema.roomMembers.roomId })
    .from(schema.roomMembers)
    .where(eq(schema.roomMembers.userId, userId));

  const unread: Record<number, number> = {};
  for (const { roomId } of memberships) {
    const lastRead = await db.select().from(schema.userRoomLastRead)
      .where(and(eq(schema.userRoomLastRead.userId, userId), eq(schema.userRoomLastRead.roomId, roomId)));
    const lastReadId = lastRead[0]?.lastReadMessageId ?? 0;
    const [result] = await db.select({ count: count() })
      .from(schema.messages)
      .where(and(eq(schema.messages.roomId, roomId), gt(schema.messages.id, lastReadId)));
    unread[roomId] = Number(result.count);
  }
  return res.json(unread);
});

// Scheduled Messages
app.post('/api/rooms/:roomId/scheduled-messages', async (req, res) => {
  const roomId = parseInt(req.params.roomId);
  const { userId, content, scheduledAt } = req.body as { userId?: number; content?: string; scheduledAt?: string };
  if (!userId || !content || !scheduledAt) {
    return res.status(400).json({ error: 'userId, content, and scheduledAt required' });
  }
  if (content.trim().length === 0 || content.trim().length > 2000) {
    return res.status(400).json({ error: 'Content must be 1-2000 characters' });
  }
  const scheduledTime = new Date(scheduledAt);
  if (isNaN(scheduledTime.getTime()) || scheduledTime <= new Date()) {
    return res.status(400).json({ error: 'scheduledAt must be a future time' });
  }
  try {
    const [msg] = await db.insert(schema.scheduledMessages)
      .values({ roomId, userId, content: content.trim(), scheduledAt: scheduledTime })
      .returning();
    return res.json(msg);
  } catch (err) {
    return res.status(500).json({ error: 'Failed to schedule message' });
  }
});

app.get('/api/users/:userId/scheduled-messages', async (req, res) => {
  const userId = parseInt(req.params.userId);
  const pending = await db.select({
    id: schema.scheduledMessages.id,
    roomId: schema.scheduledMessages.roomId,
    content: schema.scheduledMessages.content,
    scheduledAt: schema.scheduledMessages.scheduledAt,
    createdAt: schema.scheduledMessages.createdAt,
    roomName: schema.rooms.name,
  })
    .from(schema.scheduledMessages)
    .innerJoin(schema.rooms, eq(schema.scheduledMessages.roomId, schema.rooms.id))
    .where(and(eq(schema.scheduledMessages.userId, userId), isNull(schema.scheduledMessages.sentAt)));
  return res.json(pending);
});

app.delete('/api/scheduled-messages/:id', async (req, res) => {
  const id = parseInt(req.params.id);
  const { userId } = req.body as { userId?: number };
  if (!userId) return res.status(400).json({ error: 'userId required' });
  try {
    const [deleted] = await db.delete(schema.scheduledMessages)
      .where(and(eq(schema.scheduledMessages.id, id), eq(schema.scheduledMessages.userId, userId), isNull(schema.scheduledMessages.sentAt)))
      .returning();
    if (!deleted) return res.status(404).json({ error: 'Scheduled message not found or already sent' });
    return res.json({ ok: true });
  } catch (err) {
    return res.status(500).json({ error: 'Failed to cancel scheduled message' });
  }
});

// Background job: send scheduled messages
setInterval(async () => {
  try {
    const due = await db.select()
      .from(schema.scheduledMessages)
      .where(and(isNull(schema.scheduledMessages.sentAt), lte(schema.scheduledMessages.scheduledAt, new Date())));

    for (const scheduled of due) {
      // Insert actual message
      const [msg] = await db.insert(schema.messages)
        .values({ roomId: scheduled.roomId, userId: scheduled.userId, content: scheduled.content })
        .returning();
      const [user] = await db.select().from(schema.users).where(eq(schema.users.id, scheduled.userId));
      const fullMsg = { ...msg, username: user.username };
      io.to(`room:${scheduled.roomId}`).emit('new_message', fullMsg);

      // Mark as sent
      await db.update(schema.scheduledMessages)
        .set({ sentAt: new Date() })
        .where(eq(schema.scheduledMessages.id, scheduled.id));

      // Notify the author
      const authorSocket = onlineUsers.get(scheduled.userId);
      if (authorSocket) {
        io.to(authorSocket.socketId).emit('scheduled_message_sent', { id: scheduled.id, roomId: scheduled.roomId });
      }
    }
  } catch (err) {
    console.error('Scheduled message error:', err);
  }
}, 5000);

// Background job: delete expired ephemeral messages
setInterval(async () => {
  try {
    const expired = await db.select({ id: schema.messages.id, roomId: schema.messages.roomId })
      .from(schema.messages)
      .where(and(isNotNull(schema.messages.expiresAt), lte(schema.messages.expiresAt, new Date())));

    for (const msg of expired) {
      await db.delete(schema.messages).where(eq(schema.messages.id, msg.id));
      io.to(`room:${msg.roomId}`).emit('message_deleted', { messageId: msg.id, roomId: msg.roomId });
    }
  } catch (err) {
    console.error('Ephemeral message cleanup error:', err);
  }
}, 3000);

// ── Socket.io ────────────────────────────────────────────────────────────────

io.on('connection', (socket) => {
  socket.on('user_connected', (data: { userId: number; username: string }) => {
    socketToUser.set(socket.id, data);
    onlineUsers.set(data.userId, { username: data.username, socketId: socket.id });
    io.emit('online_users', Array.from(onlineUsers.entries()).map(([id, u]) => ({ userId: id, username: u.username })));
  });

  socket.on('join_room', (roomId: number) => {
    socket.join(`room:${roomId}`);
  });

  socket.on('leave_room', (roomId: number) => {
    socket.leave(`room:${roomId}`);
    clearTyping(roomId, socket);
  });

  socket.on('typing_start', (data: { roomId: number }) => {
    const user = socketToUser.get(socket.id);
    if (!user) return;
    const { roomId } = data;

    if (!typingTimers.has(roomId)) typingTimers.set(roomId, new Map());
    const roomTimers = typingTimers.get(roomId)!;

    // Broadcast that user started typing
    socket.to(`room:${roomId}`).emit('user_typing', { userId: user.userId, username: user.username, roomId });

    // Auto-expire after 4 seconds
    if (roomTimers.has(user.userId)) clearTimeout(roomTimers.get(user.userId)!);
    roomTimers.set(user.userId, setTimeout(() => {
      roomTimers.delete(user.userId);
      io.to(`room:${roomId}`).emit('user_stopped_typing', { userId: user.userId, username: user.username, roomId });
    }, 4000));
  });

  socket.on('typing_stop', (data: { roomId: number }) => {
    const user = socketToUser.get(socket.id);
    if (!user) return;
    clearTypingForUser(data.roomId, user.userId, user.username);
  });

  socket.on('disconnect', () => {
    const user = socketToUser.get(socket.id);
    if (user) {
      onlineUsers.delete(user.userId);
      socketToUser.delete(socket.id);
      // Clear all typing timers for this user
      typingTimers.forEach((roomTimers, roomId) => {
        if (roomTimers.has(user.userId)) {
          clearTimeout(roomTimers.get(user.userId)!);
          roomTimers.delete(user.userId);
          io.to(`room:${roomId}`).emit('user_stopped_typing', { userId: user.userId, username: user.username, roomId });
        }
      });
      io.emit('online_users', Array.from(onlineUsers.entries()).map(([id, u]) => ({ userId: id, username: u.username })));
    }
  });
});

function clearTyping(roomId: number, socket: import('socket.io').Socket) {
  const user = socketToUser.get(socket.id);
  if (!user) return;
  clearTypingForUser(roomId, user.userId, user.username);
}

function clearTypingForUser(roomId: number, userId: number, username: string) {
  const roomTimers = typingTimers.get(roomId);
  if (roomTimers?.has(userId)) {
    clearTimeout(roomTimers.get(userId)!);
    roomTimers.delete(userId);
    io.to(`room:${roomId}`).emit('user_stopped_typing', { userId, username, roomId });
  }
}

const PORT = parseInt(process.env.PORT || '6001');
httpServer.listen(PORT, () => {
  console.log(`Server running on http://localhost:${PORT}`);
});

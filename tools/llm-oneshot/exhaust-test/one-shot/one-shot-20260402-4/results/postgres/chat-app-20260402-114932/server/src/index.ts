import 'dotenv/config';
import express from 'express';
import { createServer } from 'http';
import { Server } from 'socket.io';
import cors from 'cors';
import { drizzle } from 'drizzle-orm/node-postgres';
import { eq, and, desc, lt, inArray, sql, isNull } from 'drizzle-orm';
import pkg from 'pg';
const { Pool } = pkg;
import * as schema from './schema.js';

const {
  users, rooms, roomMembers, messages, messageEdits,
  readReceipts, lastRead, scheduledMessages, reactions
} = schema;

const pool = new Pool({ connectionString: process.env.DATABASE_URL });
const db = drizzle(pool, { schema });

const app = express();
const httpServer = createServer(app);
const io = new Server(httpServer, {
  cors: { origin: '*', methods: ['GET', 'POST'] }
});

app.use(cors());
app.use(express.json());

const PORT = parseInt(process.env.PORT || '3301');

// Track connected sockets: socketId -> userId
const socketUsers = new Map<string, number>();
// Track user sockets: userId -> Set of socketIds
const userSockets = new Map<number, Set<string>>();
// Track last activity for auto-away
const userLastActivity = new Map<number, number>();

function addSocketUser(socketId: string, userId: number) {
  socketUsers.set(socketId, userId);
  if (!userSockets.has(userId)) userSockets.set(userId, new Set());
  userSockets.get(userId)!.add(socketId);
  userLastActivity.set(userId, Date.now());
}

function removeSocketUser(socketId: string) {
  const userId = socketUsers.get(socketId);
  if (userId) {
    socketUsers.delete(socketId);
    userSockets.get(userId)?.delete(socketId);
    if (userSockets.get(userId)?.size === 0) {
      userSockets.delete(userId);
    }
  }
  return userId;
}

function isUserOnline(userId: number): boolean {
  return (userSockets.get(userId)?.size ?? 0) > 0;
}

// ============ REST API ============

// Register/get user
app.post('/api/users/register', async (req, res) => {
  const { username } = req.body;
  if (!username || username.trim().length === 0) {
    return res.status(400).json({ error: 'Username required' });
  }
  const name = username.trim().slice(0, 32);
  try {
    let [user] = await db.select().from(users).where(eq(users.username, name));
    if (!user) {
      [user] = await db.insert(users).values({ username: name, status: 'online', lastActive: new Date() }).returning();
    }
    return res.json(user);
  } catch (e) {
    return res.status(500).json({ error: String(e) });
  }
});

app.get('/api/users', async (_req, res) => {
  const all = await db.select().from(users);
  const withOnline = all.map(u => ({ ...u, isOnline: isUserOnline(u.id) }));
  return res.json(withOnline);
});

app.patch('/api/users/:id/status', async (req, res) => {
  const id = parseInt(req.params.id);
  const { status } = req.body;
  if (!['online', 'away', 'dnd', 'invisible'].includes(status)) {
    return res.status(400).json({ error: 'Invalid status' });
  }
  const [user] = await db.update(users).set({ status, lastActive: new Date() }).where(eq(users.id, id)).returning();
  io.emit('user:status', { userId: id, status, isOnline: isUserOnline(id), lastActive: user.lastActive });
  return res.json(user);
});

// Rooms
app.get('/api/rooms', async (_req, res) => {
  const all = await db.select().from(rooms);
  return res.json(all);
});

app.post('/api/rooms', async (req, res) => {
  const { name, userId } = req.body;
  if (!name || !userId) return res.status(400).json({ error: 'name and userId required' });
  const trimmed = name.trim().slice(0, 64);
  if (!trimmed) return res.status(400).json({ error: 'Invalid room name' });
  try {
    const [room] = await db.insert(rooms).values({ name: trimmed, createdBy: userId }).returning();
    await db.insert(roomMembers).values({ roomId: room.id, userId, isAdmin: true });
    io.emit('room:created', room);
    return res.json(room);
  } catch (e) {
    return res.status(400).json({ error: 'Room name already taken' });
  }
});

app.post('/api/rooms/:id/join', async (req, res) => {
  const roomId = parseInt(req.params.id);
  const { userId } = req.body;
  // Check if banned
  const [existing] = await db.select().from(roomMembers)
    .where(and(eq(roomMembers.roomId, roomId), eq(roomMembers.userId, userId)));
  if (existing?.isBanned) return res.status(403).json({ error: 'You are banned from this room' });
  if (!existing) {
    await db.insert(roomMembers).values({ roomId, userId }).onConflictDoNothing();
  }
  io.to(`room:${roomId}`).emit('room:userJoined', { roomId, userId });
  return res.json({ ok: true });
});

app.post('/api/rooms/:id/leave', async (req, res) => {
  const roomId = parseInt(req.params.id);
  const { userId } = req.body;
  await db.delete(roomMembers).where(and(eq(roomMembers.roomId, roomId), eq(roomMembers.userId, userId)));
  io.to(`room:${roomId}`).emit('room:userLeft', { roomId, userId });
  return res.json({ ok: true });
});

app.get('/api/rooms/:id/members', async (req, res) => {
  const roomId = parseInt(req.params.id);
  const members = await db.select().from(roomMembers).where(eq(roomMembers.roomId, roomId));
  return res.json(members);
});

// Admin: kick/ban/promote
app.post('/api/rooms/:id/kick', async (req, res) => {
  const roomId = parseInt(req.params.id);
  const { adminId, targetUserId } = req.body;
  const [admin] = await db.select().from(roomMembers)
    .where(and(eq(roomMembers.roomId, roomId), eq(roomMembers.userId, adminId)));
  if (!admin?.isAdmin) return res.status(403).json({ error: 'Not admin' });
  await db.delete(roomMembers).where(and(eq(roomMembers.roomId, roomId), eq(roomMembers.userId, targetUserId)));
  // Force disconnect from room
  const sockets = userSockets.get(targetUserId);
  if (sockets) {
    for (const sid of sockets) {
      io.sockets.sockets.get(sid)?.leave(`room:${roomId}`);
    }
  }
  io.to(`room:${roomId}`).emit('room:userKicked', { roomId, userId: targetUserId });
  const targetSockets = userSockets.get(targetUserId);
  if (targetSockets) {
    for (const sid of targetSockets) {
      io.to(sid).emit('room:kicked', { roomId });
    }
  }
  return res.json({ ok: true });
});

app.post('/api/rooms/:id/ban', async (req, res) => {
  const roomId = parseInt(req.params.id);
  const { adminId, targetUserId } = req.body;
  const [admin] = await db.select().from(roomMembers)
    .where(and(eq(roomMembers.roomId, roomId), eq(roomMembers.userId, adminId)));
  if (!admin?.isAdmin) return res.status(403).json({ error: 'Not admin' });
  await db.update(roomMembers)
    .set({ isBanned: true })
    .where(and(eq(roomMembers.roomId, roomId), eq(roomMembers.userId, targetUserId)));
  const sockets = userSockets.get(targetUserId);
  if (sockets) {
    for (const sid of sockets) {
      io.sockets.sockets.get(sid)?.leave(`room:${roomId}`);
    }
  }
  io.to(`room:${roomId}`).emit('room:userKicked', { roomId, userId: targetUserId });
  const targetSockets = userSockets.get(targetUserId);
  if (targetSockets) {
    for (const sid of targetSockets) {
      io.to(sid).emit('room:banned', { roomId });
    }
  }
  return res.json({ ok: true });
});

app.post('/api/rooms/:id/promote', async (req, res) => {
  const roomId = parseInt(req.params.id);
  const { adminId, targetUserId } = req.body;
  const [admin] = await db.select().from(roomMembers)
    .where(and(eq(roomMembers.roomId, roomId), eq(roomMembers.userId, adminId)));
  if (!admin?.isAdmin) return res.status(403).json({ error: 'Not admin' });
  await db.update(roomMembers).set({ isAdmin: true })
    .where(and(eq(roomMembers.roomId, roomId), eq(roomMembers.userId, targetUserId)));
  io.to(`room:${roomId}`).emit('room:promoted', { roomId, userId: targetUserId });
  return res.json({ ok: true });
});

// Messages
app.get('/api/rooms/:id/messages', async (req, res) => {
  const roomId = parseInt(req.params.id);
  const msgs = await db.select().from(messages)
    .where(eq(messages.roomId, roomId))
    .orderBy(messages.createdAt);
  return res.json(msgs);
});

app.post('/api/rooms/:id/messages', async (req, res) => {
  const roomId = parseInt(req.params.id);
  const { userId, content, isEphemeral, ephemeralMinutes } = req.body;
  if (!content?.trim()) return res.status(400).json({ error: 'Content required' });
  // Check membership
  const [member] = await db.select().from(roomMembers)
    .where(and(eq(roomMembers.roomId, roomId), eq(roomMembers.userId, userId)));
  if (!member || member.isBanned) return res.status(403).json({ error: 'Not a member' });

  let ephExpiresAt: Date | undefined;
  if (isEphemeral && ephemeralMinutes) {
    ephExpiresAt = new Date(Date.now() + ephemeralMinutes * 60 * 1000);
  }

  const [msg] = await db.insert(messages).values({
    roomId,
    userId,
    content: content.trim().slice(0, 2000),
    isEphemeral: !!isEphemeral,
    ephemeralExpiresAt: ephExpiresAt ?? null,
  }).returning();

  // Update last activity
  await db.update(users).set({ lastActive: new Date() }).where(eq(users.id, userId));

  io.to(`room:${roomId}`).emit('message:new', msg);
  return res.json(msg);
});

// Edit message
app.patch('/api/messages/:id', async (req, res) => {
  const msgId = parseInt(req.params.id);
  const { userId, content } = req.body;
  const [msg] = await db.select().from(messages).where(eq(messages.id, msgId));
  if (!msg) return res.status(404).json({ error: 'Not found' });
  if (msg.userId !== userId) return res.status(403).json({ error: 'Not your message' });

  await db.insert(messageEdits).values({ messageId: msgId, oldContent: msg.content, newContent: content.trim() });
  const [updated] = await db.update(messages)
    .set({ content: content.trim().slice(0, 2000), isEdited: true, updatedAt: new Date() })
    .where(eq(messages.id, msgId))
    .returning();

  io.to(`room:${msg.roomId}`).emit('message:edited', updated);
  return res.json(updated);
});

// Edit history
app.get('/api/messages/:id/history', async (req, res) => {
  const msgId = parseInt(req.params.id);
  const edits = await db.select().from(messageEdits)
    .where(eq(messageEdits.messageId, msgId))
    .orderBy(messageEdits.editedAt);
  return res.json(edits);
});

// Read receipts
app.post('/api/rooms/:id/read', async (req, res) => {
  const roomId = parseInt(req.params.id);
  const { userId, messageIds } = req.body;
  if (!messageIds?.length) return res.json({ ok: true });

  for (const messageId of messageIds) {
    await db.insert(readReceipts).values({ messageId, userId }).onConflictDoNothing();
  }

  // Update last read
  const maxId = Math.max(...messageIds);
  await db.insert(lastRead)
    .values({ roomId, userId, lastMessageId: maxId, updatedAt: new Date() })
    .onConflictDoUpdate({
      target: [lastRead.roomId, lastRead.userId],
      set: { lastMessageId: maxId, updatedAt: new Date() }
    });

  io.to(`room:${roomId}`).emit('message:read', { userId, messageIds, roomId });
  return res.json({ ok: true });
});

app.get('/api/messages/:id/receipts', async (req, res) => {
  const msgId = parseInt(req.params.id);
  const receipts = await db.select().from(readReceipts).where(eq(readReceipts.messageId, msgId));
  return res.json(receipts);
});

app.get('/api/rooms/:id/unread/:userId', async (req, res) => {
  const roomId = parseInt(req.params.id);
  const userId = parseInt(req.params.userId);
  const [lr] = await db.select().from(lastRead)
    .where(and(eq(lastRead.roomId, roomId), eq(lastRead.userId, userId)));
  const lastMsgId = lr?.lastMessageId ?? 0;
  const count = await db.select({ count: sql<number>`count(*)` }).from(messages)
    .where(and(eq(messages.roomId, roomId), sql`${messages.id} > ${lastMsgId}`));
  return res.json({ unread: Number(count[0]?.count ?? 0) });
});

// Scheduled messages
app.post('/api/rooms/:id/schedule', async (req, res) => {
  const roomId = parseInt(req.params.id);
  const { userId, content, scheduledFor } = req.body;
  if (!content?.trim() || !scheduledFor) return res.status(400).json({ error: 'content and scheduledFor required' });
  const schedDate = new Date(scheduledFor);
  if (schedDate <= new Date()) return res.status(400).json({ error: 'scheduledFor must be in the future' });

  const [sched] = await db.insert(scheduledMessages).values({
    roomId, userId, content: content.trim(), scheduledFor: schedDate
  }).returning();
  io.to(`room:${roomId}`).emit('scheduled:new', sched);
  return res.json(sched);
});

app.delete('/api/scheduled/:id', async (req, res) => {
  const id = parseInt(req.params.id);
  const { userId } = req.body;
  const [sched] = await db.select().from(scheduledMessages).where(eq(scheduledMessages.id, id));
  if (!sched) return res.status(404).json({ error: 'Not found' });
  if (sched.userId !== userId) return res.status(403).json({ error: 'Not yours' });
  const [updated] = await db.update(scheduledMessages)
    .set({ isCancelled: true })
    .where(eq(scheduledMessages.id, id))
    .returning();
  io.to(`room:${sched.roomId}`).emit('scheduled:cancelled', { id });
  return res.json(updated);
});

app.get('/api/rooms/:id/scheduled/:userId', async (req, res) => {
  const roomId = parseInt(req.params.id);
  const userId = parseInt(req.params.userId);
  const pending = await db.select().from(scheduledMessages)
    .where(and(
      eq(scheduledMessages.roomId, roomId),
      eq(scheduledMessages.userId, userId),
      eq(scheduledMessages.isSent, false),
      eq(scheduledMessages.isCancelled, false)
    ))
    .orderBy(scheduledMessages.scheduledFor);
  return res.json(pending);
});

// Reactions
app.post('/api/messages/:id/reactions', async (req, res) => {
  const messageId = parseInt(req.params.id);
  const { userId, emoji } = req.body;
  const [msg] = await db.select().from(messages).where(eq(messages.id, messageId));
  if (!msg) return res.status(404).json({ error: 'Not found' });

  // Toggle
  const [existing] = await db.select().from(reactions)
    .where(and(eq(reactions.messageId, messageId), eq(reactions.userId, userId), eq(reactions.emoji, emoji)));
  let added: boolean;
  if (existing) {
    await db.delete(reactions).where(eq(reactions.id, existing.id));
    added = false;
  } else {
    await db.insert(reactions).values({ messageId, userId, emoji });
    added = true;
  }

  const allReactions = await db.select().from(reactions).where(eq(reactions.messageId, messageId));
  io.to(`room:${msg.roomId}`).emit('reaction:updated', { messageId, reactions: allReactions });
  return res.json({ added, reactions: allReactions });
});

app.get('/api/messages/:id/reactions', async (req, res) => {
  const messageId = parseInt(req.params.id);
  const all = await db.select().from(reactions).where(eq(reactions.messageId, messageId));
  return res.json(all);
});

// ============ Socket.io ============

io.on('connection', (socket) => {
  console.log('Socket connected:', socket.id);

  socket.on('user:identify', async (userId: number) => {
    addSocketUser(socket.id, userId);
    await db.update(users).set({ status: 'online', lastActive: new Date() }).where(eq(users.id, userId));
    io.emit('user:online', { userId, isOnline: true });
  });

  socket.on('room:join', async (roomId: number) => {
    socket.join(`room:${roomId}`);
  });

  socket.on('room:leave', (roomId: number) => {
    socket.leave(`room:${roomId}`);
  });

  socket.on('typing:start', (data: { roomId: number; userId: number; username: string }) => {
    socket.to(`room:${data.roomId}`).emit('typing:update', { ...data, typing: true });
  });

  socket.on('typing:stop', (data: { roomId: number; userId: number }) => {
    socket.to(`room:${data.roomId}`).emit('typing:update', { ...data, typing: false });
  });

  socket.on('user:activity', async (userId: number) => {
    userLastActivity.set(userId, Date.now());
    const [user] = await db.select().from(users).where(eq(users.id, userId));
    if (user?.status === 'away') {
      await db.update(users).set({ status: 'online', lastActive: new Date() }).where(eq(users.id, userId));
      io.emit('user:status', { userId, status: 'online', isOnline: true, lastActive: new Date() });
    }
  });

  socket.on('disconnect', async () => {
    const userId = removeSocketUser(socket.id);
    if (userId && !isUserOnline(userId)) {
      await db.update(users).set({ lastActive: new Date() }).where(eq(users.id, userId));
      io.emit('user:online', { userId, isOnline: false });
    }
  });
});

// ============ Background Jobs ============

// Send scheduled messages
setInterval(async () => {
  try {
    const due = await db.select().from(scheduledMessages)
      .where(and(
        eq(scheduledMessages.isSent, false),
        eq(scheduledMessages.isCancelled, false),
        lt(scheduledMessages.scheduledFor, new Date())
      ));

    for (const sched of due) {
      // Check member still valid
      const [member] = await db.select().from(roomMembers)
        .where(and(eq(roomMembers.roomId, sched.roomId), eq(roomMembers.userId, sched.userId)));
      if (member && !member.isBanned) {
        const [msg] = await db.insert(messages).values({
          roomId: sched.roomId,
          userId: sched.userId,
          content: sched.content,
        }).returning();
        io.to(`room:${sched.roomId}`).emit('message:new', msg);
      }
      await db.update(scheduledMessages).set({ isSent: true }).where(eq(scheduledMessages.id, sched.id));
      io.to(`room:${sched.roomId}`).emit('scheduled:sent', { id: sched.id, message: sched });
    }
  } catch (e) {
    console.error('Scheduled messages error:', e);
  }
}, 5000);

// Delete expired ephemeral messages
setInterval(async () => {
  try {
    const expired = await db.select().from(messages)
      .where(and(
        eq(messages.isEphemeral, true),
        lt(messages.ephemeralExpiresAt, new Date())
      ));
    if (expired.length === 0) return;

    const ids = expired.map(m => m.id);
    await db.delete(messages).where(inArray(messages.id, ids));

    for (const msg of expired) {
      io.to(`room:${msg.roomId}`).emit('message:deleted', { messageId: msg.id, roomId: msg.roomId });
    }
  } catch (e) {
    console.error('Ephemeral cleanup error:', e);
  }
}, 10000);

// Auto-away after 5 minutes of inactivity
setInterval(async () => {
  try {
    const now = Date.now();
    const AWAY_THRESHOLD = 5 * 60 * 1000;
    for (const [userId, lastActivity] of userLastActivity.entries()) {
      if (isUserOnline(userId) && now - lastActivity > AWAY_THRESHOLD) {
        const [user] = await db.select().from(users).where(eq(users.id, userId));
        if (user && user.status === 'online') {
          await db.update(users).set({ status: 'away' }).where(eq(users.id, userId));
          io.emit('user:status', { userId, status: 'away', isOnline: true, lastActive: user.lastActive });
        }
      }
    }
  } catch (e) {
    console.error('Auto-away error:', e);
  }
}, 60000);

httpServer.listen(PORT, () => {
  console.log(`Server running on port ${PORT}`);
});

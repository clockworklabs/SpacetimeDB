import 'dotenv/config';
import express from 'express';
import { createServer } from 'http';
import { Server } from 'socket.io';
import cors from 'cors';
import { drizzle } from 'drizzle-orm/node-postgres';
import pg from 'pg';
import { eq, and, desc, gt, lt, inArray, sql, isNull } from 'drizzle-orm';
import * as schema from './schema.js';

const { Pool } = pg;

const pool = new Pool({ connectionString: process.env.DATABASE_URL });
const db = drizzle(pool, { schema });

const app = express();
const httpServer = createServer(app);

const CLIENT_ORIGIN = 'http://localhost:5274';

const io = new Server(httpServer, {
  cors: { origin: CLIENT_ORIGIN, methods: ['GET', 'POST'] },
});

app.use(cors({ origin: CLIENT_ORIGIN }));
app.use(express.json());

const PORT = parseInt(process.env.PORT || '3101');

// Track socket -> userId mappings and typing timers
const socketUserMap = new Map<string, number>(); // socketId -> userId
const typingTimers = new Map<string, ReturnType<typeof setTimeout>>(); // `roomId:userId` -> timer

// ─── REST ROUTES ──────────────────────────────────────────────────────────────

// Register / login user
app.post('/api/users/register', async (req, res) => {
  const { name } = req.body as { name: string };
  if (!name || name.trim().length < 1 || name.trim().length > 32) {
    return res.status(400).json({ error: 'Name must be 1-32 characters' });
  }
  try {
    const existing = await db.select().from(schema.users).where(eq(schema.users.name, name.trim())).limit(1);
    if (existing.length > 0) {
      await db.update(schema.users)
        .set({ status: 'online', lastActive: new Date() })
        .where(eq(schema.users.id, existing[0].id));
      return res.json(existing[0]);
    }
    const [user] = await db.insert(schema.users).values({ name: name.trim(), status: 'online' }).returning();
    return res.json(user);
  } catch (e) {
    return res.status(500).json({ error: 'Failed to register' });
  }
});

// Get all users
app.get('/api/users', async (_req, res) => {
  const users = await db.select().from(schema.users).orderBy(schema.users.name);
  res.json(users);
});

// Update user status
app.patch('/api/users/:id/status', async (req, res) => {
  const { status } = req.body as { status: schema.User['status'] };
  const id = parseInt(req.params.id);
  await db.update(schema.users).set({ status, lastActive: new Date() }).where(eq(schema.users.id, id));
  const [user] = await db.select().from(schema.users).where(eq(schema.users.id, id));
  io.emit('user:status', user);
  res.json(user);
});

// Get all rooms
app.get('/api/rooms', async (_req, res) => {
  const rooms = await db.select().from(schema.rooms).orderBy(schema.rooms.name);
  res.json(rooms);
});

// Create room
app.post('/api/rooms', async (req, res) => {
  const { name, userId } = req.body as { name: string; userId: number };
  if (!name || name.trim().length < 1) return res.status(400).json({ error: 'Room name required' });
  try {
    const [room] = await db.insert(schema.rooms).values({ name: name.trim(), creatorId: userId }).returning();
    // Auto-join creator as admin
    await db.insert(schema.roomMembers).values({ roomId: room.id, userId, isAdmin: true });
    io.emit('room:created', room);
    res.json(room);
  } catch (e) {
    res.status(400).json({ error: 'Room already exists' });
  }
});

// Join room
app.post('/api/rooms/:id/join', async (req, res) => {
  const roomId = parseInt(req.params.id);
  const { userId } = req.body as { userId: number };
  const banned = await db.select().from(schema.roomMembers)
    .where(and(eq(schema.roomMembers.roomId, roomId), eq(schema.roomMembers.userId, userId), eq(schema.roomMembers.isBanned, true)))
    .limit(1);
  if (banned.length > 0) return res.status(403).json({ error: 'You are banned from this room' });

  await db.insert(schema.roomMembers)
    .values({ roomId, userId })
    .onConflictDoUpdate({ target: [schema.roomMembers.roomId, schema.roomMembers.userId], set: { isBanned: false } });
  const [room] = await db.select().from(schema.rooms).where(eq(schema.rooms.id, roomId));
  io.to(`room:${roomId}`).emit('room:member_joined', { roomId, userId });
  res.json(room);
});

// Leave room
app.post('/api/rooms/:id/leave', async (req, res) => {
  const roomId = parseInt(req.params.id);
  const { userId } = req.body as { userId: number };
  await db.delete(schema.roomMembers)
    .where(and(eq(schema.roomMembers.roomId, roomId), eq(schema.roomMembers.userId, userId)));
  io.to(`room:${roomId}`).emit('room:member_left', { roomId, userId });
  res.json({ ok: true });
});

// Get room members
app.get('/api/rooms/:id/members', async (req, res) => {
  const roomId = parseInt(req.params.id);
  const members = await db
    .select({ member: schema.roomMembers, user: schema.users })
    .from(schema.roomMembers)
    .innerJoin(schema.users, eq(schema.roomMembers.userId, schema.users.id))
    .where(and(eq(schema.roomMembers.roomId, roomId), eq(schema.roomMembers.isBanned, false)));
  res.json(members.map(m => ({ ...m.user, isAdmin: m.member.isAdmin, isBanned: m.member.isBanned })));
});

// Kick user
app.post('/api/rooms/:id/kick', async (req, res) => {
  const roomId = parseInt(req.params.id);
  const { adminId, targetUserId } = req.body as { adminId: number; targetUserId: number };
  const [admin] = await db.select().from(schema.roomMembers)
    .where(and(eq(schema.roomMembers.roomId, roomId), eq(schema.roomMembers.userId, adminId), eq(schema.roomMembers.isAdmin, true)));
  if (!admin) return res.status(403).json({ error: 'Not an admin' });
  await db.update(schema.roomMembers)
    .set({ isBanned: true })
    .where(and(eq(schema.roomMembers.roomId, roomId), eq(schema.roomMembers.userId, targetUserId)));
  io.to(`room:${roomId}`).emit('room:kicked', { roomId, userId: targetUserId });
  res.json({ ok: true });
});

// Promote user
app.post('/api/rooms/:id/promote', async (req, res) => {
  const roomId = parseInt(req.params.id);
  const { adminId, targetUserId } = req.body as { adminId: number; targetUserId: number };
  const [admin] = await db.select().from(schema.roomMembers)
    .where(and(eq(schema.roomMembers.roomId, roomId), eq(schema.roomMembers.userId, adminId), eq(schema.roomMembers.isAdmin, true)));
  if (!admin) return res.status(403).json({ error: 'Not an admin' });
  await db.update(schema.roomMembers)
    .set({ isAdmin: true })
    .where(and(eq(schema.roomMembers.roomId, roomId), eq(schema.roomMembers.userId, targetUserId)));
  io.to(`room:${roomId}`).emit('room:promoted', { roomId, userId: targetUserId });
  res.json({ ok: true });
});

// Get messages for room
app.get('/api/rooms/:id/messages', async (req, res) => {
  const roomId = parseInt(req.params.id);
  const msgs = await db
    .select({ message: schema.messages, user: schema.users })
    .from(schema.messages)
    .innerJoin(schema.users, eq(schema.messages.userId, schema.users.id))
    .where(and(
      eq(schema.messages.roomId, roomId),
      // exclude already expired
      sql`(${schema.messages.expiresAt} IS NULL OR ${schema.messages.expiresAt} > NOW())`
    ))
    .orderBy(schema.messages.createdAt);
  res.json(msgs.map(m => ({ ...m.message, userName: m.user.name })));
});

// Send message
app.post('/api/rooms/:id/messages', async (req, res) => {
  const roomId = parseInt(req.params.id);
  const { userId, content, isEphemeral, ephemeralDuration } = req.body as {
    userId: number; content: string; isEphemeral?: boolean; ephemeralDuration?: number;
  };
  if (!content || content.trim().length === 0) return res.status(400).json({ error: 'Content required' });

  const member = await db.select().from(schema.roomMembers)
    .where(and(eq(schema.roomMembers.roomId, roomId), eq(schema.roomMembers.userId, userId), eq(schema.roomMembers.isBanned, false)))
    .limit(1);
  if (member.length === 0) return res.status(403).json({ error: 'Not a member of this room' });

  let expiresAt: Date | undefined;
  if (isEphemeral && ephemeralDuration) {
    expiresAt = new Date(Date.now() + ephemeralDuration * 1000);
  }

  const [message] = await db.insert(schema.messages).values({
    roomId, userId, content: content.trim(),
    isEphemeral: !!isEphemeral,
    expiresAt: expiresAt || null,
  }).returning();

  const [user] = await db.select().from(schema.users).where(eq(schema.users.id, userId));
  const fullMsg = { ...message, userName: user.name };
  io.to(`room:${roomId}`).emit('message:new', fullMsg);

  // Schedule ephemeral deletion
  if (expiresAt) {
    const delay = expiresAt.getTime() - Date.now();
    setTimeout(async () => {
      await db.delete(schema.messages).where(eq(schema.messages.id, message.id));
      io.to(`room:${roomId}`).emit('message:deleted', { messageId: message.id, roomId });
    }, delay);
  }

  // Update last active
  await db.update(schema.users).set({ lastActive: new Date() }).where(eq(schema.users.id, userId));

  res.json(fullMsg);
});

// Edit message
app.patch('/api/messages/:id', async (req, res) => {
  const messageId = parseInt(req.params.id);
  const { userId, content } = req.body as { userId: number; content: string };

  const [msg] = await db.select().from(schema.messages).where(eq(schema.messages.id, messageId));
  if (!msg) return res.status(404).json({ error: 'Message not found' });
  if (msg.userId !== userId) return res.status(403).json({ error: 'Not your message' });

  // Save to history
  await db.insert(schema.messageEdits).values({ messageId, content: msg.content });

  const [updated] = await db.update(schema.messages)
    .set({ content: content.trim(), isEdited: true, updatedAt: new Date() })
    .where(eq(schema.messages.id, messageId))
    .returning();

  const [user] = await db.select().from(schema.users).where(eq(schema.users.id, userId));
  const fullMsg = { ...updated, userName: user.name };
  io.to(`room:${msg.roomId}`).emit('message:edited', fullMsg);
  res.json(fullMsg);
});

// Get message edit history
app.get('/api/messages/:id/history', async (req, res) => {
  const messageId = parseInt(req.params.id);
  const edits = await db.select().from(schema.messageEdits)
    .where(eq(schema.messageEdits.messageId, messageId))
    .orderBy(desc(schema.messageEdits.editedAt));
  res.json(edits);
});

// Get reactions for message
app.get('/api/messages/:id/reactions', async (req, res) => {
  const messageId = parseInt(req.params.id);
  const rxns = await db
    .select({ reaction: schema.reactions, user: schema.users })
    .from(schema.reactions)
    .innerJoin(schema.users, eq(schema.reactions.userId, schema.users.id))
    .where(eq(schema.reactions.messageId, messageId));
  res.json(rxns.map(r => ({ ...r.reaction, userName: r.user.name })));
});

// Toggle reaction
app.post('/api/messages/:id/reactions', async (req, res) => {
  const messageId = parseInt(req.params.id);
  const { userId, emoji } = req.body as { userId: number; emoji: string };

  const existing = await db.select().from(schema.reactions)
    .where(and(eq(schema.reactions.messageId, messageId), eq(schema.reactions.userId, userId), eq(schema.reactions.emoji, emoji)))
    .limit(1);

  const [msg] = await db.select().from(schema.messages).where(eq(schema.messages.id, messageId));
  if (!msg) return res.status(404).json({ error: 'Message not found' });

  if (existing.length > 0) {
    await db.delete(schema.reactions)
      .where(and(eq(schema.reactions.messageId, messageId), eq(schema.reactions.userId, userId), eq(schema.reactions.emoji, emoji)));
  } else {
    await db.insert(schema.reactions).values({ messageId, userId, emoji });
  }

  // Fetch all reactions for this message
  const rxns = await db
    .select({ reaction: schema.reactions, user: schema.users })
    .from(schema.reactions)
    .innerJoin(schema.users, eq(schema.reactions.userId, schema.users.id))
    .where(eq(schema.reactions.messageId, messageId));
  const reactionData = rxns.map(r => ({ ...r.reaction, userName: r.user.name }));

  io.to(`room:${msg.roomId}`).emit('reaction:updated', { messageId, reactions: reactionData });
  res.json(reactionData);
});

// Mark room as read
app.post('/api/rooms/:id/read', async (req, res) => {
  const roomId = parseInt(req.params.id);
  const { userId, lastMessageId } = req.body as { userId: number; lastMessageId: number | null };
  await db.insert(schema.readReceipts)
    .values({ roomId, userId, lastReadMessageId: lastMessageId, lastReadAt: new Date() })
    .onConflictDoUpdate({
      target: [schema.readReceipts.roomId, schema.readReceipts.userId],
      set: { lastReadMessageId: lastMessageId, lastReadAt: new Date() },
    });

  const [user] = await db.select().from(schema.users).where(eq(schema.users.id, userId));
  io.to(`room:${roomId}`).emit('read:updated', { roomId, userId, userName: user.name, lastMessageId });
  res.json({ ok: true });
});

// Get read receipts for room
app.get('/api/rooms/:id/receipts', async (req, res) => {
  const roomId = parseInt(req.params.id);
  const receipts = await db
    .select({ receipt: schema.readReceipts, user: schema.users })
    .from(schema.readReceipts)
    .innerJoin(schema.users, eq(schema.readReceipts.userId, schema.users.id))
    .where(eq(schema.readReceipts.roomId, roomId));
  res.json(receipts.map(r => ({ ...r.receipt, userName: r.user.name })));
});

// Get scheduled messages for user
app.get('/api/users/:id/scheduled', async (req, res) => {
  const userId = parseInt(req.params.id);
  const scheduled = await db.select().from(schema.scheduledMessages)
    .where(and(eq(schema.scheduledMessages.userId, userId), eq(schema.scheduledMessages.sent, false)))
    .orderBy(schema.scheduledMessages.scheduledAt);
  res.json(scheduled);
});

// Create scheduled message
app.post('/api/rooms/:id/schedule', async (req, res) => {
  const roomId = parseInt(req.params.id);
  const { userId, content, scheduledAt } = req.body as { userId: number; content: string; scheduledAt: string };
  const schedTime = new Date(scheduledAt);
  if (schedTime <= new Date()) return res.status(400).json({ error: 'Scheduled time must be in the future' });

  const [scheduled] = await db.insert(schema.scheduledMessages)
    .values({ roomId, userId, content, scheduledAt: schedTime })
    .returning();

  // Set timer to send
  const delay = schedTime.getTime() - Date.now();
  setTimeout(async () => {
    const [still] = await db.select().from(schema.scheduledMessages)
      .where(and(eq(schema.scheduledMessages.id, scheduled.id), eq(schema.scheduledMessages.sent, false)));
    if (!still) return;

    const member = await db.select().from(schema.roomMembers)
      .where(and(eq(schema.roomMembers.roomId, roomId), eq(schema.roomMembers.userId, userId)))
      .limit(1);
    if (member.length === 0) return; // user left

    const [msg] = await db.insert(schema.messages)
      .values({ roomId, userId, content })
      .returning();
    await db.update(schema.scheduledMessages).set({ sent: true }).where(eq(schema.scheduledMessages.id, scheduled.id));

    const [user] = await db.select().from(schema.users).where(eq(schema.users.id, userId));
    io.to(`room:${roomId}`).emit('message:new', { ...msg, userName: user.name });
    io.to(`user:${userId}`).emit('scheduled:sent', { id: scheduled.id });
  }, delay);

  res.json(scheduled);
});

// Cancel scheduled message
app.delete('/api/scheduled/:id', async (req, res) => {
  const id = parseInt(req.params.id);
  const { userId } = req.body as { userId: number };
  await db.delete(schema.scheduledMessages)
    .where(and(eq(schema.scheduledMessages.id, id), eq(schema.scheduledMessages.userId, userId)));
  res.json({ ok: true });
});

// ─── SOCKET.IO ────────────────────────────────────────────────────────────────

io.on('connection', (socket) => {
  socket.on('user:connect', async (userId: number) => {
    socketUserMap.set(socket.id, userId);
    socket.join(`user:${userId}`);
    await db.update(schema.users).set({ status: 'online', lastActive: new Date() }).where(eq(schema.users.id, userId));
    const [user] = await db.select().from(schema.users).where(eq(schema.users.id, userId));
    io.emit('user:status', user);
  });

  socket.on('room:join', (roomId: number) => {
    socket.join(`room:${roomId}`);
  });

  socket.on('room:leave', (roomId: number) => {
    socket.leave(`room:${roomId}`);
  });

  socket.on('typing:start', async (data: { roomId: number; userId: number }) => {
    const key = `${data.roomId}:${data.userId}`;
    if (typingTimers.has(key)) clearTimeout(typingTimers.get(key)!);
    const [user] = await db.select().from(schema.users).where(eq(schema.users.id, data.userId));
    socket.to(`room:${data.roomId}`).emit('typing:update', { roomId: data.roomId, userId: data.userId, userName: user.name, isTyping: true });

    const timer = setTimeout(() => {
      socket.to(`room:${data.roomId}`).emit('typing:update', { roomId: data.roomId, userId: data.userId, userName: user.name, isTyping: false });
      typingTimers.delete(key);
    }, 4000);
    typingTimers.set(key, timer);
  });

  socket.on('typing:stop', async (data: { roomId: number; userId: number }) => {
    const key = `${data.roomId}:${data.userId}`;
    if (typingTimers.has(key)) {
      clearTimeout(typingTimers.get(key)!);
      typingTimers.delete(key);
    }
    const [user] = await db.select().from(schema.users).where(eq(schema.users.id, data.userId)).limit(1);
    if (user) {
      socket.to(`room:${data.roomId}`).emit('typing:update', { roomId: data.roomId, userId: data.userId, userName: user.name, isTyping: false });
    }
  });

  socket.on('disconnect', async () => {
    const userId = socketUserMap.get(socket.id);
    if (userId) {
      socketUserMap.delete(socket.id);
      // Check if user has other connections
      const otherSockets = [...socketUserMap.values()].filter(id => id === userId);
      if (otherSockets.length === 0) {
        await db.update(schema.users).set({ lastActive: new Date() }).where(eq(schema.users.id, userId));
        // Don't set offline immediately — let status remain as-is but update lastActive
        const [user] = await db.select().from(schema.users).where(eq(schema.users.id, userId));
        if (user && user.status !== 'invisible') {
          io.emit('user:status', user);
        }
      }
    }
  });
});

// ─── BACKGROUND JOBS ──────────────────────────────────────────────────────────

// Clean up expired ephemeral messages every 10 seconds
setInterval(async () => {
  try {
    const expired = await db.select().from(schema.messages)
      .where(and(
        eq(schema.messages.isEphemeral, true),
        lt(schema.messages.expiresAt, new Date()),
        sql`${schema.messages.expiresAt} IS NOT NULL`
      ));
    for (const msg of expired) {
      await db.delete(schema.messages).where(eq(schema.messages.id, msg.id));
      io.to(`room:${msg.roomId}`).emit('message:deleted', { messageId: msg.id, roomId: msg.roomId });
    }
  } catch (_e) { /* ignore */ }
}, 10_000);

// Auto-set inactive users to "away" after 5 minutes
setInterval(async () => {
  try {
    const fiveMinutesAgo = new Date(Date.now() - 5 * 60 * 1000);
    const inactiveUsers = await db.select().from(schema.users)
      .where(and(
        eq(schema.users.status, 'online'),
        lt(schema.users.lastActive, fiveMinutesAgo)
      ));
    for (const user of inactiveUsers) {
      const [updated] = await db.update(schema.users)
        .set({ status: 'away' })
        .where(eq(schema.users.id, user.id))
        .returning();
      io.emit('user:status', updated);
    }
  } catch (_e) { /* ignore */ }
}, 60_000);

// Restore scheduled messages on startup (in case server restarted)
async function restoreScheduledMessages() {
  try {
    const pending = await db.select().from(schema.scheduledMessages)
      .where(and(eq(schema.scheduledMessages.sent, false), gt(schema.scheduledMessages.scheduledAt, new Date())));
    for (const scheduled of pending) {
      const delay = scheduled.scheduledAt.getTime() - Date.now();
      if (delay > 0) {
        setTimeout(async () => {
          const [still] = await db.select().from(schema.scheduledMessages)
            .where(and(eq(schema.scheduledMessages.id, scheduled.id), eq(schema.scheduledMessages.sent, false)));
          if (!still) return;
          const [msg] = await db.insert(schema.messages)
            .values({ roomId: scheduled.roomId, userId: scheduled.userId, content: scheduled.content })
            .returning();
          await db.update(schema.scheduledMessages).set({ sent: true }).where(eq(schema.scheduledMessages.id, scheduled.id));
          const [user] = await db.select().from(schema.users).where(eq(schema.users.id, scheduled.userId));
          io.to(`room:${scheduled.roomId}`).emit('message:new', { ...msg, userName: user.name });
          io.to(`user:${scheduled.userId}`).emit('scheduled:sent', { id: scheduled.id });
        }, delay);
      }
    }
  } catch (_e) { /* ignore */ }
}

httpServer.listen(PORT, async () => {
  console.log(`Server running on port ${PORT}`);
  await restoreScheduledMessages();
});

// Type helper
type User = typeof schema.users.$inferSelect;
declare module './schema.js' {
  interface User {
    status: 'online' | 'away' | 'dnd' | 'invisible';
  }
}

import 'dotenv/config';
import express from 'express';
import { createServer } from 'http';
import { Server } from 'socket.io';
import cors from 'cors';
import { drizzle } from 'drizzle-orm/node-postgres';
import pkg from 'pg';
const { Pool } = pkg;
import { eq, and, inArray, lt, isNull, desc, sql } from 'drizzle-orm';
import { users, rooms, roomMembers, messages, messageHistory, readReceipts, messageReactions, scheduledMessages } from './schema.js';
import { randomUUID } from 'crypto';

const PORT = parseInt(process.env.PORT || '3101');
const DATABASE_URL = process.env.DATABASE_URL || 'postgresql://spacetime:spacetime@localhost:5433/spacetime_run1_run1_run1';
const CLIENT_ORIGIN = `http://localhost:5274`;

const pool = new Pool({ connectionString: DATABASE_URL });
const db = drizzle(pool);

const app = express();
app.use(cors({ origin: CLIENT_ORIGIN, credentials: true }));
app.use(express.json());

const httpServer = createServer(app);
const io = new Server(httpServer, {
  cors: { origin: CLIENT_ORIGIN, credentials: true },
});

// Track typing state: roomId -> Map<userId, timeout>
const typingTimers = new Map<string, Map<string, ReturnType<typeof setTimeout>>>();

// Track inactivity timers: userId -> timeout
const inactivityTimers = new Map<string, ReturnType<typeof setTimeout>>();
const INACTIVITY_TIMEOUT = 5 * 60 * 1000; // 5 minutes

function setInactivityTimer(userId: string) {
  const existing = inactivityTimers.get(userId);
  if (existing) clearTimeout(existing);
  const timer = setTimeout(async () => {
    await db.update(users).set({ status: 'away', lastActive: new Date() }).where(eq(users.id, userId));
    const user = await db.select().from(users).where(eq(users.id, userId)).limit(1);
    if (user[0]) io.emit('user:status', { userId, status: 'away', lastActive: user[0].lastActive });
  }, INACTIVITY_TIMEOUT);
  inactivityTimers.set(userId, timer);
}

// Scheduled message processor
async function processScheduledMessages() {
  const now = new Date();
  const pending = await db.select().from(scheduledMessages)
    .where(and(eq(scheduledMessages.isSent, false), isNull(scheduledMessages.cancelledAt), lt(scheduledMessages.scheduledAt, now)));

  for (const sm of pending) {
    const msgId = randomUUID();
    await db.insert(messages).values({
      id: msgId,
      roomId: sm.roomId,
      senderId: sm.senderId,
      content: sm.content,
      createdAt: now,
    });
    await db.update(scheduledMessages).set({ isSent: true }).where(eq(scheduledMessages.id, sm.id));

    const sender = await db.select().from(users).where(eq(users.id, sm.senderId)).limit(1);
    const msg = {
      id: msgId,
      roomId: sm.roomId,
      senderId: sm.senderId,
      senderName: sender[0]?.username ?? 'Unknown',
      content: sm.content,
      createdAt: now.toISOString(),
      isEphemeral: false,
      expiresAt: null,
      reactions: [],
    };
    io.to(`room:${sm.roomId}`).emit('message:new', msg);
  }
}
setInterval(processScheduledMessages, 5000);

// Ephemeral message cleanup
async function cleanupEphemeral() {
  const now = new Date();
  const expired = await db.select().from(messages)
    .where(and(eq(messages.isEphemeral, true), lt(messages.expiresAt!, now)));

  for (const msg of expired) {
    await db.delete(readReceipts).where(eq(readReceipts.messageId, msg.id));
    await db.delete(messageReactions).where(eq(messageReactions.messageId, msg.id));
    await db.delete(messageHistory).where(eq(messageHistory.messageId, msg.id));
    await db.delete(messages).where(eq(messages.id, msg.id));
    io.to(`room:${msg.roomId}`).emit('message:deleted', { messageId: msg.id });
  }
}
setInterval(cleanupEphemeral, 5000);

// Helper to get full message with reactions
async function getFullMessage(msgId: string) {
  const [msg] = await db.select().from(messages).where(eq(messages.id, msgId)).limit(1);
  if (!msg) return null;
  const sender = await db.select().from(users).where(eq(users.id, msg.senderId)).limit(1);
  const reactions = await db.select().from(messageReactions).where(eq(messageReactions.messageId, msgId));
  const receipts = await db.select().from(readReceipts).where(eq(readReceipts.messageId, msgId));
  const readerIds = receipts.map(r => r.userId);
  let readerNames: string[] = [];
  if (readerIds.length > 0) {
    const readers = await db.select().from(users).where(inArray(users.id, readerIds));
    readerNames = readers.map(u => u.username);
  }

  const reactionMap: Record<string, { count: number; users: string[] }> = {};
  for (const r of reactions) {
    if (!reactionMap[r.emoji]) reactionMap[r.emoji] = { count: 0, users: [] };
    reactionMap[r.emoji].count++;
    const u = await db.select().from(users).where(eq(users.id, r.userId)).limit(1);
    if (u[0]) reactionMap[r.emoji].users.push(u[0].username);
  }

  return {
    id: msg.id,
    roomId: msg.roomId,
    senderId: msg.senderId,
    senderName: sender[0]?.username ?? 'Unknown',
    content: msg.content,
    createdAt: msg.createdAt.toISOString(),
    editedAt: msg.editedAt?.toISOString() ?? null,
    isEphemeral: msg.isEphemeral,
    expiresAt: msg.expiresAt?.toISOString() ?? null,
    reactions: reactionMap,
    seenBy: readerNames,
  };
}

// ─── REST API ─────────────────────────────────────────────────────────────

// Register/login user
app.post('/api/users', async (req, res) => {
  const { username } = req.body as { username: string };
  if (!username || username.trim().length < 1 || username.trim().length > 30) {
    return res.status(400).json({ error: 'Invalid username' });
  }
  const name = username.trim();
  try {
    const existing = await db.select().from(users).where(eq(users.username, name)).limit(1);
    if (existing[0]) {
      await db.update(users).set({ lastActive: new Date() }).where(eq(users.id, existing[0].id));
      return res.json(existing[0]);
    }
    const id = randomUUID();
    await db.insert(users).values({ id, username: name, status: 'online', lastActive: new Date() });
    const [user] = await db.select().from(users).where(eq(users.id, id)).limit(1);
    return res.json(user);
  } catch (e: unknown) {
    const err = e as Error;
    return res.status(500).json({ error: err.message });
  }
});

// Get all rooms
app.get('/api/rooms', async (_req, res) => {
  const all = await db.select().from(rooms).orderBy(desc(rooms.createdAt));
  return res.json(all);
});

// Create room
app.post('/api/rooms', async (req, res) => {
  const { name, creatorId } = req.body as { name: string; creatorId: string };
  if (!name || name.trim().length < 1 || name.trim().length > 50) {
    return res.status(400).json({ error: 'Invalid room name' });
  }
  const rname = name.trim();
  try {
    const id = randomUUID();
    await db.insert(rooms).values({ id, name: rname, creatorId });
    await db.insert(roomMembers).values({ roomId: id, userId: creatorId, role: 'admin' });
    const [room] = await db.select().from(rooms).where(eq(rooms.id, id)).limit(1);
    io.emit('room:new', room);
    return res.json(room);
  } catch (e: unknown) {
    const err = e as Error;
    return res.status(500).json({ error: err.message });
  }
});

// Get room messages
app.get('/api/rooms/:roomId/messages', async (req, res) => {
  const { roomId } = req.params;
  const msgs = await db.select().from(messages)
    .where(eq(messages.roomId, roomId))
    .orderBy(messages.createdAt);

  const result = await Promise.all(msgs.map(async (msg) => {
    const sender = await db.select().from(users).where(eq(users.id, msg.senderId)).limit(1);
    const reactions = await db.select().from(messageReactions).where(eq(messageReactions.messageId, msg.id));
    const receipts = await db.select().from(readReceipts).where(eq(readReceipts.messageId, msg.id));
    const readerIds = receipts.map(r => r.userId);
    let readerNames: string[] = [];
    if (readerIds.length > 0) {
      const readers = await db.select().from(users).where(inArray(users.id, readerIds));
      readerNames = readers.map(u => u.username);
    }
    const reactionMap: Record<string, { count: number; users: string[] }> = {};
    for (const r of reactions) {
      if (!reactionMap[r.emoji]) reactionMap[r.emoji] = { count: 0, users: [] };
      reactionMap[r.emoji].count++;
      const u = await db.select().from(users).where(eq(users.id, r.userId)).limit(1);
      if (u[0]) reactionMap[r.emoji].users.push(u[0].username);
    }
    return {
      id: msg.id,
      roomId: msg.roomId,
      senderId: msg.senderId,
      senderName: sender[0]?.username ?? 'Unknown',
      content: msg.content,
      createdAt: msg.createdAt.toISOString(),
      editedAt: msg.editedAt?.toISOString() ?? null,
      isEphemeral: msg.isEphemeral,
      expiresAt: msg.expiresAt?.toISOString() ?? null,
      reactions: reactionMap,
      seenBy: readerNames,
    };
  }));
  return res.json(result);
});

// Get room members
app.get('/api/rooms/:roomId/members', async (req, res) => {
  const { roomId } = req.params;
  const members = await db.select().from(roomMembers)
    .where(and(eq(roomMembers.roomId, roomId), eq(roomMembers.isBanned, false)));
  const result = await Promise.all(members.map(async (m) => {
    const [u] = await db.select().from(users).where(eq(users.id, m.userId)).limit(1);
    return { ...m, username: u?.username ?? 'Unknown', status: u?.status ?? 'offline', lastActive: u?.lastActive?.toISOString() ?? null };
  }));
  return res.json(result);
});

// Get online users
app.get('/api/users/online', async (_req, res) => {
  const all = await db.select().from(users).orderBy(users.username);
  return res.json(all.map(u => ({
    id: u.id,
    username: u.username,
    status: u.status,
    lastActive: u.lastActive.toISOString(),
  })));
});

// Get message history
app.get('/api/messages/:messageId/history', async (req, res) => {
  const { messageId } = req.params;
  const history = await db.select().from(messageHistory)
    .where(eq(messageHistory.messageId, messageId))
    .orderBy(messageHistory.editedAt);
  return res.json(history.map(h => ({ content: h.content, editedAt: h.editedAt.toISOString() })));
});

// Get scheduled messages for user
app.get('/api/users/:userId/scheduled', async (req, res) => {
  const { userId } = req.params;
  const pending = await db.select().from(scheduledMessages)
    .where(and(eq(scheduledMessages.senderId, userId), eq(scheduledMessages.isSent, false), isNull(scheduledMessages.cancelledAt)))
    .orderBy(scheduledMessages.scheduledAt);
  return res.json(pending.map(s => ({
    id: s.id,
    roomId: s.roomId,
    content: s.content,
    scheduledAt: s.scheduledAt.toISOString(),
  })));
});

// ─── Socket.io ─────────────────────────────────────────────────────────────

io.on('connection', (socket) => {
  let currentUserId: string | null = null;

  socket.on('user:connect', async ({ userId }: { userId: string }) => {
    currentUserId = userId;
    await db.update(users).set({ socketId: socket.id, status: 'online', lastActive: new Date() }).where(eq(users.id, userId));
    setInactivityTimer(userId);
    const [u] = await db.select().from(users).where(eq(users.id, userId)).limit(1);
    io.emit('user:status', { userId, status: 'online', username: u?.username, lastActive: u?.lastActive?.toISOString() });
  });

  socket.on('user:set-status', async ({ userId, status }: { userId: string; status: 'online' | 'away' | 'dnd' | 'invisible' }) => {
    await db.update(users).set({ status, lastActive: new Date() }).where(eq(users.id, userId));
    setInactivityTimer(userId);
    io.emit('user:status', { userId, status, lastActive: new Date().toISOString() });
  });

  socket.on('room:join', async ({ roomId, userId }: { roomId: string; userId: string }) => {
    // Check if banned
    const membership = await db.select().from(roomMembers)
      .where(and(eq(roomMembers.roomId, roomId), eq(roomMembers.userId, userId))).limit(1);

    if (membership[0]?.isBanned) {
      socket.emit('error', { message: 'You have been kicked from this room' });
      return;
    }

    if (!membership[0]) {
      await db.insert(roomMembers).values({ roomId, userId, role: 'member' });
    }

    socket.join(`room:${roomId}`);
    const [u] = await db.select().from(users).where(eq(users.id, userId)).limit(1);
    io.to(`room:${roomId}`).emit('room:member-joined', { roomId, userId, username: u?.username });
  });

  socket.on('room:leave', async ({ roomId, userId }: { roomId: string; userId: string }) => {
    socket.leave(`room:${roomId}`);
    io.to(`room:${roomId}`).emit('room:member-left', { roomId, userId });
  });

  socket.on('message:send', async ({ roomId, userId, content, isEphemeral, ephemeralDuration }: {
    roomId: string; userId: string; content: string; isEphemeral?: boolean; ephemeralDuration?: number;
  }) => {
    if (!content || content.trim().length === 0 || content.length > 2000) return;

    // Rate limiting: check last message
    const lastMsg = await db.select().from(messages)
      .where(and(eq(messages.roomId, roomId), eq(messages.senderId, userId)))
      .orderBy(desc(messages.createdAt)).limit(1);
    if (lastMsg[0] && (Date.now() - lastMsg[0].createdAt.getTime()) < 500) return;

    // Check membership
    const membership = await db.select().from(roomMembers)
      .where(and(eq(roomMembers.roomId, roomId), eq(roomMembers.userId, userId))).limit(1);
    if (!membership[0] || membership[0].isBanned) return;

    const msgId = randomUUID();
    const now = new Date();
    let expiresAt: Date | null = null;
    if (isEphemeral && ephemeralDuration) {
      expiresAt = new Date(now.getTime() + ephemeralDuration * 1000);
    }

    await db.insert(messages).values({
      id: msgId,
      roomId,
      senderId: userId,
      content: content.trim(),
      createdAt: now,
      isEphemeral: !!isEphemeral,
      expiresAt,
    });

    if (currentUserId) {
      await db.update(users).set({ lastActive: now }).where(eq(users.id, userId));
      setInactivityTimer(userId);
    }

    const fullMsg = await getFullMessage(msgId);
    if (fullMsg) io.to(`room:${roomId}`).emit('message:new', fullMsg);
  });

  socket.on('message:edit', async ({ messageId, userId, newContent }: { messageId: string; userId: string; newContent: string }) => {
    if (!newContent || newContent.trim().length === 0) return;

    const [msg] = await db.select().from(messages).where(eq(messages.id, messageId)).limit(1);
    if (!msg || msg.senderId !== userId) return;

    // Save history
    await db.insert(messageHistory).values({ id: randomUUID(), messageId, content: msg.content, editedAt: new Date() });

    await db.update(messages).set({ content: newContent.trim(), editedAt: new Date() }).where(eq(messages.id, messageId));

    const fullMsg = await getFullMessage(messageId);
    if (fullMsg) io.to(`room:${msg.roomId}`).emit('message:edited', fullMsg);
  });

  socket.on('message:react', async ({ messageId, userId, emoji }: { messageId: string; userId: string; emoji: string }) => {
    const [msg] = await db.select().from(messages).where(eq(messages.id, messageId)).limit(1);
    if (!msg) return;

    const existing = await db.select().from(messageReactions)
      .where(and(eq(messageReactions.messageId, messageId), eq(messageReactions.userId, userId), eq(messageReactions.emoji, emoji)))
      .limit(1);

    if (existing[0]) {
      await db.delete(messageReactions).where(eq(messageReactions.id, existing[0].id));
    } else {
      await db.insert(messageReactions).values({ id: randomUUID(), messageId, userId, emoji });
    }

    const fullMsg = await getFullMessage(messageId);
    if (fullMsg) io.to(`room:${msg.roomId}`).emit('message:reactions-updated', fullMsg);
  });

  socket.on('message:read', async ({ roomId, userId, messageIds }: { roomId: string; userId: string; messageIds: string[] }) => {
    for (const messageId of messageIds) {
      const existing = await db.select().from(readReceipts)
        .where(and(eq(readReceipts.messageId, messageId), eq(readReceipts.userId, userId))).limit(1);
      if (!existing[0]) {
        await db.insert(readReceipts).values({ messageId, userId, readAt: new Date() });
        const fullMsg = await getFullMessage(messageId);
        if (fullMsg) io.to(`room:${roomId}`).emit('message:read-receipt', fullMsg);
      }
    }
  });

  socket.on('typing:start', ({ roomId, userId, username }: { roomId: string; userId: string; username: string }) => {
    if (!typingTimers.has(roomId)) typingTimers.set(roomId, new Map());
    const roomTimers = typingTimers.get(roomId)!;

    const existing = roomTimers.get(userId);
    if (existing) clearTimeout(existing);

    socket.to(`room:${roomId}`).emit('typing:update', { roomId, userId, username, isTyping: true });

    const timer = setTimeout(() => {
      roomTimers.delete(userId);
      socket.to(`room:${roomId}`).emit('typing:update', { roomId, userId, username, isTyping: false });
    }, 5000);
    roomTimers.set(userId, timer);
  });

  socket.on('typing:stop', ({ roomId, userId, username }: { roomId: string; userId: string; username: string }) => {
    const roomTimers = typingTimers.get(roomId);
    if (roomTimers) {
      const t = roomTimers.get(userId);
      if (t) clearTimeout(t);
      roomTimers.delete(userId);
    }
    socket.to(`room:${roomId}`).emit('typing:update', { roomId, userId, username, isTyping: false });
  });

  socket.on('message:schedule', async ({ roomId, userId, content, scheduledAt }: {
    roomId: string; userId: string; content: string; scheduledAt: string;
  }) => {
    const membership = await db.select().from(roomMembers)
      .where(and(eq(roomMembers.roomId, roomId), eq(roomMembers.userId, userId))).limit(1);
    if (!membership[0] || membership[0].isBanned) return;

    const id = randomUUID();
    await db.insert(scheduledMessages).values({
      id, roomId, senderId: userId, content: content.trim(), scheduledAt: new Date(scheduledAt),
    });
    const [sm] = await db.select().from(scheduledMessages).where(eq(scheduledMessages.id, id)).limit(1);
    socket.emit('scheduled:created', { id: sm.id, roomId: sm.roomId, content: sm.content, scheduledAt: sm.scheduledAt.toISOString() });
  });

  socket.on('message:cancel-scheduled', async ({ scheduledId, userId }: { scheduledId: string; userId: string }) => {
    const [sm] = await db.select().from(scheduledMessages).where(eq(scheduledMessages.id, scheduledId)).limit(1);
    if (!sm || sm.senderId !== userId) return;
    await db.update(scheduledMessages).set({ cancelledAt: new Date() }).where(eq(scheduledMessages.id, scheduledId));
    socket.emit('scheduled:cancelled', { scheduledId });
  });

  socket.on('room:kick', async ({ roomId, targetUserId, adminId }: { roomId: string; targetUserId: string; adminId: string }) => {
    // Verify admin
    const adminMem = await db.select().from(roomMembers)
      .where(and(eq(roomMembers.roomId, roomId), eq(roomMembers.userId, adminId))).limit(1);
    if (!adminMem[0] || adminMem[0].role !== 'admin') return;

    await db.update(roomMembers).set({ isBanned: true }).where(and(eq(roomMembers.roomId, roomId), eq(roomMembers.userId, targetUserId)));

    // Find target socket and kick them
    const [targetUser] = await db.select().from(users).where(eq(users.id, targetUserId)).limit(1);
    if (targetUser?.socketId) {
      const targetSocket = io.sockets.sockets.get(targetUser.socketId);
      if (targetSocket) {
        targetSocket.leave(`room:${roomId}`);
        targetSocket.emit('room:kicked', { roomId });
      }
    }

    io.to(`room:${roomId}`).emit('room:member-kicked', { roomId, userId: targetUserId });
  });

  socket.on('room:promote', async ({ roomId, targetUserId, adminId }: { roomId: string; targetUserId: string; adminId: string }) => {
    const adminMem = await db.select().from(roomMembers)
      .where(and(eq(roomMembers.roomId, roomId), eq(roomMembers.userId, adminId))).limit(1);
    if (!adminMem[0] || adminMem[0].role !== 'admin') return;

    await db.update(roomMembers).set({ role: 'admin' }).where(and(eq(roomMembers.roomId, roomId), eq(roomMembers.userId, targetUserId)));
    const [u] = await db.select().from(users).where(eq(users.id, targetUserId)).limit(1);
    io.to(`room:${roomId}`).emit('room:member-promoted', { roomId, userId: targetUserId, username: u?.username });
  });

  socket.on('disconnect', async () => {
    if (currentUserId) {
      // Clear typing timers for this user
      for (const [roomId, roomTimers] of typingTimers) {
        const t = roomTimers.get(currentUserId);
        if (t) {
          clearTimeout(t);
          roomTimers.delete(currentUserId);
          io.to(`room:${roomId}`).emit('typing:update', { roomId, userId: currentUserId, isTyping: false });
        }
      }
      const inactTimer = inactivityTimers.get(currentUserId);
      if (inactTimer) clearTimeout(inactTimer);

      await db.update(users).set({ socketId: null, lastActive: new Date() }).where(eq(users.id, currentUserId));
      // Don't set offline – keep last status; just update lastActive
      io.emit('user:status', { userId: currentUserId, status: 'away', lastActive: new Date().toISOString() });
    }
  });
});

// Health check
app.get('/api/health', (_req, res) => res.json({ ok: true }));

httpServer.listen(PORT, () => {
  console.log(`Server running on port ${PORT}`);
});

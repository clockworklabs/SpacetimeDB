import 'dotenv/config';
import express from 'express';
import { createServer } from 'http';
import { Server } from 'socket.io';
import cors from 'cors';
import { drizzle } from 'drizzle-orm/node-postgres';
import pkg from 'pg';
const { Pool } = pkg;
import { eq, and, desc, inArray, sql, not, lt } from 'drizzle-orm';
import { users, rooms, roomMembers, messages, messageEdits, readReceipts, lastRead, reactions, scheduledMessages } from './schema.js';
import { randomUUID } from 'crypto';

const app = express();
const httpServer = createServer(app);

const CLIENT_ORIGIN = 'http://localhost:5474';

const io = new Server(httpServer, {
  cors: { origin: CLIENT_ORIGIN, methods: ['GET', 'POST'] },
});

app.use(cors({ origin: CLIENT_ORIGIN }));
app.use(express.json());

const pool = new Pool({ connectionString: process.env.DATABASE_URL });
const db = drizzle(pool);

// Track socket -> userId mapping and typing timers
const socketUserMap = new Map<string, string>();
const typingTimers = new Map<string, NodeJS.Timeout>();
const inactivityTimers = new Map<string, NodeJS.Timeout>();

// ─── REST Routes ────────────────────────────────────────────────────────────

// Register/login user
app.post('/api/users', async (req, res) => {
  const { name } = req.body as { name: string };
  if (!name || name.trim().length < 1 || name.trim().length > 30) {
    return res.status(400).json({ error: 'Name must be 1-30 characters' });
  }
  const trimmed = name.trim();
  // Upsert by name
  const existing = await db.select().from(users).where(eq(users.name, trimmed)).limit(1);
  if (existing.length > 0) {
    const u = existing[0];
    await db.update(users).set({ status: 'online', lastActive: new Date() }).where(eq(users.id, u.id));
    return res.json({ ...u, status: 'online' });
  }
  const id = randomUUID();
  const [user] = await db.insert(users).values({ id, name: trimmed, status: 'online' }).returning();
  return res.json(user);
});

// Get all users
app.get('/api/users', async (_req, res) => {
  const all = await db.select().from(users).orderBy(users.name);
  res.json(all);
});

// Update user status
app.patch('/api/users/:id/status', async (req, res) => {
  const { status } = req.body as { status: string };
  const validStatuses = ['online', 'away', 'dnd', 'invisible'];
  if (!validStatuses.includes(status)) return res.status(400).json({ error: 'Invalid status' });
  const [u] = await db.update(users)
    .set({ status: status as 'online' | 'away' | 'dnd' | 'invisible', lastActive: new Date() })
    .where(eq(users.id, req.params.id))
    .returning();
  if (!u) return res.status(404).json({ error: 'User not found' });
  io.emit('userStatusChanged', u);
  res.json(u);
});

// Get rooms
app.get('/api/rooms', async (_req, res) => {
  const all = await db.select().from(rooms).orderBy(rooms.name);
  res.json(all);
});

// Create room
app.post('/api/rooms', async (req, res) => {
  const { name, userId } = req.body as { name: string; userId: string };
  if (!name || name.trim().length < 1 || name.trim().length > 50) {
    return res.status(400).json({ error: 'Room name must be 1-50 characters' });
  }
  const id = randomUUID();
  try {
    const [room] = await db.insert(rooms).values({ id, name: name.trim(), creatorId: userId }).returning();
    // Auto-join creator as admin
    await db.insert(roomMembers).values({ roomId: id, userId, isAdmin: true });
    io.emit('roomCreated', room);
    res.json(room);
  } catch {
    res.status(400).json({ error: 'Room name already exists' });
  }
});

// Get room members
app.get('/api/rooms/:roomId/members', async (req, res) => {
  const members = await db
    .select({ user: users, member: roomMembers })
    .from(roomMembers)
    .innerJoin(users, eq(users.id, roomMembers.userId))
    .where(and(eq(roomMembers.roomId, req.params.roomId), eq(roomMembers.isBanned, false)));
  res.json(members);
});

// Join room
app.post('/api/rooms/:roomId/join', async (req, res) => {
  const { userId } = req.body as { userId: string };
  const existing = await db.select().from(roomMembers)
    .where(and(eq(roomMembers.roomId, req.params.roomId), eq(roomMembers.userId, userId)))
    .limit(1);
  if (existing.length > 0) {
    if (existing[0].isBanned) return res.status(403).json({ error: 'You are banned from this room' });
    return res.json({ joined: true });
  }
  await db.insert(roomMembers).values({ roomId: req.params.roomId, userId, isAdmin: false });
  io.to(req.params.roomId).emit('memberJoined', { roomId: req.params.roomId, userId });
  res.json({ joined: true });
});

// Leave room
app.post('/api/rooms/:roomId/leave', async (req, res) => {
  const { userId } = req.body as { userId: string };
  await db.delete(roomMembers)
    .where(and(eq(roomMembers.roomId, req.params.roomId), eq(roomMembers.userId, userId)));
  io.to(req.params.roomId).emit('memberLeft', { roomId: req.params.roomId, userId });
  res.json({ left: true });
});

// Kick user
app.post('/api/rooms/:roomId/kick', async (req, res) => {
  const { adminId, targetUserId } = req.body as { adminId: string; targetUserId: string };
  const [admin] = await db.select().from(roomMembers)
    .where(and(eq(roomMembers.roomId, req.params.roomId), eq(roomMembers.userId, adminId), eq(roomMembers.isAdmin, true)));
  if (!admin) return res.status(403).json({ error: 'Not an admin' });
  await db.update(roomMembers)
    .set({ isBanned: true })
    .where(and(eq(roomMembers.roomId, req.params.roomId), eq(roomMembers.userId, targetUserId)));
  io.to(req.params.roomId).emit('userKicked', { roomId: req.params.roomId, userId: targetUserId });
  res.json({ kicked: true });
});

// Promote user
app.post('/api/rooms/:roomId/promote', async (req, res) => {
  const { adminId, targetUserId } = req.body as { adminId: string; targetUserId: string };
  const [admin] = await db.select().from(roomMembers)
    .where(and(eq(roomMembers.roomId, req.params.roomId), eq(roomMembers.userId, adminId), eq(roomMembers.isAdmin, true)));
  if (!admin) return res.status(403).json({ error: 'Not an admin' });
  await db.update(roomMembers)
    .set({ isAdmin: true })
    .where(and(eq(roomMembers.roomId, req.params.roomId), eq(roomMembers.userId, targetUserId)));
  io.to(req.params.roomId).emit('memberPromoted', { roomId: req.params.roomId, userId: targetUserId });
  res.json({ promoted: true });
});

// Get messages for a room
app.get('/api/rooms/:roomId/messages', async (req, res) => {
  const userId = req.query.userId as string;
  const now = new Date();

  // Get sent messages (not scheduled pending)
  const msgs = await db.select().from(messages)
    .where(and(
      eq(messages.roomId, req.params.roomId),
      eq(messages.isSent, true),
    ))
    .orderBy(messages.createdAt)
    .limit(100);

  // Filter expired ephemeral messages
  const visible = msgs.filter(m => !m.expiresAt || m.expiresAt > now);

  // Get reactions
  const msgIds = visible.map(m => m.id);
  const allReactions = msgIds.length > 0
    ? await db.select().from(reactions).where(inArray(reactions.messageId, msgIds))
    : [];

  // Get read receipts
  const allReceipts = msgIds.length > 0
    ? await db.select({ receipt: readReceipts, user: users })
        .from(readReceipts)
        .innerJoin(users, eq(users.id, readReceipts.userId))
        .where(inArray(readReceipts.messageId, msgIds))
    : [];

  // Update last read for this user
  if (userId && visible.length > 0) {
    const lastMsg = visible[visible.length - 1];
    const existing = await db.select().from(lastRead)
      .where(and(eq(lastRead.roomId, req.params.roomId), eq(lastRead.userId, userId))).limit(1);
    if (existing.length > 0) {
      await db.update(lastRead).set({ lastMessageId: lastMsg.id, updatedAt: new Date() })
        .where(and(eq(lastRead.roomId, req.params.roomId), eq(lastRead.userId, userId)));
    } else {
      await db.insert(lastRead).values({ roomId: req.params.roomId, userId, lastMessageId: lastMsg.id });
    }
    // Mark all visible messages as read
    for (const msg of visible) {
      if (msg.userId !== userId) {
        const existingReceipt = await db.select().from(readReceipts)
          .where(and(eq(readReceipts.messageId, msg.id), eq(readReceipts.userId, userId))).limit(1);
        if (existingReceipt.length === 0) {
          await db.insert(readReceipts).values({ messageId: msg.id, userId }).onConflictDoNothing();
        }
      }
    }
  }

  const result = visible.map(m => ({
    ...m,
    reactions: allReactions.filter(r => r.messageId === m.id),
    readBy: allReceipts.filter(r => r.receipt.messageId === m.id).map(r => ({ userId: r.user.id, name: r.user.name })),
  }));

  res.json(result);
});

// Send message
app.post('/api/rooms/:roomId/messages', async (req, res) => {
  const { userId, content, expiresInSeconds } = req.body as {
    userId: string; content: string; expiresInSeconds?: number;
  };
  if (!content || content.trim().length === 0) return res.status(400).json({ error: 'Empty message' });
  if (content.length > 2000) return res.status(400).json({ error: 'Message too long' });

  // Check member and not banned
  const [member] = await db.select().from(roomMembers)
    .where(and(eq(roomMembers.roomId, req.params.roomId), eq(roomMembers.userId, userId)));
  if (!member || member.isBanned) return res.status(403).json({ error: 'Not a member or banned' });

  const id = randomUUID();
  const expiresAt = expiresInSeconds ? new Date(Date.now() + expiresInSeconds * 1000) : null;
  const [msg] = await db.insert(messages).values({
    id, roomId: req.params.roomId, userId, content: content.trim(),
    expiresAt: expiresAt ?? undefined, isSent: true,
  }).returning();

  const [user] = await db.select().from(users).where(eq(users.id, userId)).limit(1);
  const fullMsg = { ...msg, reactions: [], readBy: [], userName: user?.name };
  io.to(req.params.roomId).emit('newMessage', fullMsg);
  res.json(fullMsg);
});

// Edit message
app.patch('/api/messages/:messageId', async (req, res) => {
  const { userId, content } = req.body as { userId: string; content: string };
  const [msg] = await db.select().from(messages).where(eq(messages.id, req.params.messageId)).limit(1);
  if (!msg) return res.status(404).json({ error: 'Not found' });
  if (msg.userId !== userId) return res.status(403).json({ error: 'Not your message' });

  // Save edit history
  await db.insert(messageEdits).values({ id: randomUUID(), messageId: msg.id, content: msg.content });
  const [updated] = await db.update(messages)
    .set({ content: content.trim(), isEdited: true })
    .where(eq(messages.id, req.params.messageId))
    .returning();
  io.to(msg.roomId).emit('messageEdited', updated);
  res.json(updated);
});

// Get message edit history
app.get('/api/messages/:messageId/history', async (req, res) => {
  const history = await db.select().from(messageEdits)
    .where(eq(messageEdits.messageId, req.params.messageId))
    .orderBy(messageEdits.editedAt);
  res.json(history);
});

// React to message
app.post('/api/messages/:messageId/reactions', async (req, res) => {
  const { userId, emoji } = req.body as { userId: string; emoji: string };
  // Toggle
  const existing = await db.select().from(reactions)
    .where(and(eq(reactions.messageId, req.params.messageId), eq(reactions.userId, userId), eq(reactions.emoji, emoji)))
    .limit(1);
  if (existing.length > 0) {
    await db.delete(reactions).where(eq(reactions.id, existing[0].id));
  } else {
    await db.insert(reactions).values({ id: randomUUID(), messageId: req.params.messageId, userId, emoji });
  }
  const allReactions = await db.select({ reaction: reactions, user: users })
    .from(reactions)
    .innerJoin(users, eq(users.id, reactions.userId))
    .where(eq(reactions.messageId, req.params.messageId));
  const [msg] = await db.select().from(messages).where(eq(messages.id, req.params.messageId)).limit(1);
  if (msg) {
    io.to(msg.roomId).emit('reactionsUpdated', { messageId: req.params.messageId, reactions: allReactions });
  }
  res.json(allReactions);
});

// Mark messages as read
app.post('/api/rooms/:roomId/read', async (req, res) => {
  const { userId } = req.body as { userId: string };
  const msgs = await db.select().from(messages)
    .where(and(eq(messages.roomId, req.params.roomId), eq(messages.isSent, true)));
  for (const msg of msgs) {
    if (msg.userId !== userId) {
      await db.insert(readReceipts).values({ messageId: msg.id, userId }).onConflictDoNothing();
    }
  }
  const lastMsg = msgs[msgs.length - 1];
  if (lastMsg) {
    const existing = await db.select().from(lastRead)
      .where(and(eq(lastRead.roomId, req.params.roomId), eq(lastRead.userId, userId))).limit(1);
    if (existing.length > 0) {
      await db.update(lastRead).set({ lastMessageId: lastMsg.id, updatedAt: new Date() })
        .where(and(eq(lastRead.roomId, req.params.roomId), eq(lastRead.userId, userId)));
    } else {
      await db.insert(lastRead).values({ roomId: req.params.roomId, userId, lastMessageId: lastMsg.id });
    }
  }
  // Notify others in room
  io.to(req.params.roomId).emit('messagesRead', { roomId: req.params.roomId, userId });
  res.json({ ok: true });
});

// Get unread counts for a user
app.get('/api/users/:userId/unread', async (req, res) => {
  const userId = req.params.userId;
  // Get all rooms user is in
  const memberRooms = await db.select().from(roomMembers)
    .where(and(eq(roomMembers.userId, userId), eq(roomMembers.isBanned, false)));

  const result: Record<string, number> = {};
  for (const m of memberRooms) {
    const lr = await db.select().from(lastRead)
      .where(and(eq(lastRead.roomId, m.roomId), eq(lastRead.userId, userId))).limit(1);
    const lastMsgId = lr[0]?.lastMessageId;
    let unreadCount = 0;
    if (lastMsgId) {
      const [lastMsg] = await db.select().from(messages).where(eq(messages.id, lastMsgId)).limit(1);
      if (lastMsg) {
        const count = await db.select({ count: sql<number>`count(*)` }).from(messages)
          .where(and(
            eq(messages.roomId, m.roomId),
            eq(messages.isSent, true),
            sql`${messages.createdAt} > ${lastMsg.createdAt}`,
            not(eq(messages.userId, userId)),
          ));
        unreadCount = Number(count[0]?.count ?? 0);
      }
    } else {
      const count = await db.select({ count: sql<number>`count(*)` }).from(messages)
        .where(and(eq(messages.roomId, m.roomId), eq(messages.isSent, true), not(eq(messages.userId, userId))));
      unreadCount = Number(count[0]?.count ?? 0);
    }
    result[m.roomId] = unreadCount;
  }
  res.json(result);
});

// Scheduled messages
app.post('/api/rooms/:roomId/scheduled', async (req, res) => {
  const { userId, content, scheduledAt } = req.body as { userId: string; content: string; scheduledAt: string };
  const id = randomUUID();
  const [sm] = await db.insert(scheduledMessages).values({
    id, roomId: req.params.roomId, userId, content, scheduledAt: new Date(scheduledAt),
  }).returning();
  res.json(sm);
});

app.get('/api/users/:userId/scheduled', async (req, res) => {
  const pending = await db.select().from(scheduledMessages)
    .where(and(eq(scheduledMessages.userId, req.params.userId), eq(scheduledMessages.isCancelled, false)));
  res.json(pending);
});

app.delete('/api/scheduled/:id', async (req, res) => {
  await db.update(scheduledMessages).set({ isCancelled: true }).where(eq(scheduledMessages.id, req.params.id));
  res.json({ cancelled: true });
});

// ─── Socket.io ──────────────────────────────────────────────────────────────

io.on('connection', (socket) => {
  socket.on('identify', async (userId: string) => {
    socketUserMap.set(socket.id, userId);
    await db.update(users).set({ status: 'online', lastActive: new Date() }).where(eq(users.id, userId));
    const [u] = await db.select().from(users).where(eq(users.id, userId)).limit(1);
    if (u) io.emit('userStatusChanged', u);

    // Set inactivity timer
    const resetInactivity = () => {
      const existing = inactivityTimers.get(userId);
      if (existing) clearTimeout(existing);
      const t = setTimeout(async () => {
        const user = await db.select().from(users).where(eq(users.id, userId)).limit(1);
        if (user[0]?.status === 'online') {
          await db.update(users).set({ status: 'away' }).where(eq(users.id, userId));
          const [updated] = await db.select().from(users).where(eq(users.id, userId)).limit(1);
          if (updated) io.emit('userStatusChanged', updated);
        }
      }, 5 * 60 * 1000); // 5 minutes
      inactivityTimers.set(userId, t);
    };
    resetInactivity();
    socket.on('activity', resetInactivity);
  });

  socket.on('joinRoom', (roomId: string) => {
    socket.join(roomId);
  });

  socket.on('leaveRoom', (roomId: string) => {
    socket.leave(roomId);
  });

  socket.on('typing', ({ roomId, userId }: { roomId: string; userId: string }) => {
    const key = `${roomId}:${userId}`;
    socket.to(roomId).emit('userTyping', { roomId, userId });
    const existing = typingTimers.get(key);
    if (existing) clearTimeout(existing);
    const t = setTimeout(() => {
      socket.to(roomId).emit('userStoppedTyping', { roomId, userId });
      typingTimers.delete(key);
    }, 4000);
    typingTimers.set(key, t);
  });

  socket.on('stopTyping', ({ roomId, userId }: { roomId: string; userId: string }) => {
    const key = `${roomId}:${userId}`;
    const existing = typingTimers.get(key);
    if (existing) {
      clearTimeout(existing);
      typingTimers.delete(key);
    }
    socket.to(roomId).emit('userStoppedTyping', { roomId, userId });
  });

  socket.on('markRead', async ({ roomId, userId }: { roomId: string; userId: string }) => {
    const msgs = await db.select().from(messages)
      .where(and(eq(messages.roomId, roomId), eq(messages.isSent, true)));
    for (const msg of msgs) {
      if (msg.userId !== userId) {
        await db.insert(readReceipts).values({ messageId: msg.id, userId }).onConflictDoNothing();
      }
    }
    const lastMsg = msgs[msgs.length - 1];
    if (lastMsg) {
      const existing = await db.select().from(lastRead)
        .where(and(eq(lastRead.roomId, roomId), eq(lastRead.userId, userId))).limit(1);
      if (existing.length > 0) {
        await db.update(lastRead).set({ lastMessageId: lastMsg.id, updatedAt: new Date() })
          .where(and(eq(lastRead.roomId, roomId), eq(lastRead.userId, userId)));
      } else {
        await db.insert(lastRead).values({ roomId, userId, lastMessageId: lastMsg.id });
      }
    }
    io.to(roomId).emit('messagesRead', { roomId, userId });
  });

  socket.on('disconnect', async () => {
    const userId = socketUserMap.get(socket.id);
    if (userId) {
      socketUserMap.delete(socket.id);
      // Check if any other socket has this userId
      const stillConnected = [...socketUserMap.values()].includes(userId);
      if (!stillConnected) {
        await db.update(users).set({ lastActive: new Date() }).where(eq(users.id, userId));
        // Keep status as is (could be invisible etc), just update lastActive
        const [u] = await db.select().from(users).where(eq(users.id, userId)).limit(1);
        if (u) io.emit('userStatusChanged', u);
      }
    }
  });
});

// ─── Scheduled message processor ────────────────────────────────────────────

async function processScheduledMessages() {
  const now = new Date();
  const pending = await db.select().from(scheduledMessages)
    .where(and(
      eq(scheduledMessages.isCancelled, false),
      lt(scheduledMessages.scheduledAt, now),
    ));
  for (const sm of pending) {
    const id = randomUUID();
    const [msg] = await db.insert(messages).values({
      id, roomId: sm.roomId, userId: sm.userId, content: sm.content, isSent: true,
    }).returning();
    await db.update(scheduledMessages).set({ isCancelled: true }).where(eq(scheduledMessages.id, sm.id));
    const [user] = await db.select().from(users).where(eq(users.id, sm.userId)).limit(1);
    io.to(sm.roomId).emit('newMessage', { ...msg, reactions: [], readBy: [], userName: user?.name });
  }
}

// Delete expired ephemeral messages
async function cleanupEphemeral() {
  const now = new Date();
  const expired = await db.select().from(messages)
    .where(and(eq(messages.isSent, true), lt(messages.expiresAt!, now)));
  for (const msg of expired) {
    await db.delete(readReceipts).where(eq(readReceipts.messageId, msg.id));
    await db.delete(reactions).where(eq(reactions.messageId, msg.id));
    await db.delete(messageEdits).where(eq(messageEdits.messageId, msg.id));
    await db.delete(messages).where(eq(messages.id, msg.id));
    io.to(msg.roomId).emit('messageDeleted', { messageId: msg.id, roomId: msg.roomId });
  }
}

setInterval(processScheduledMessages, 5000);
setInterval(cleanupEphemeral, 5000);

const PORT = process.env.PORT ? parseInt(process.env.PORT) : 3301;
httpServer.listen(PORT, () => {
  console.log(`Server running on http://localhost:${PORT}`);
});

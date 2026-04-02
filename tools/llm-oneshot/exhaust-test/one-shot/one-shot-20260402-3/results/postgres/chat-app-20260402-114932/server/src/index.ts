import 'dotenv/config';
import express from 'express';
import { createServer } from 'http';
import { Server, Socket } from 'socket.io';
import cors from 'cors';
import { drizzle } from 'drizzle-orm/node-postgres';
import { Pool } from 'pg';
import { eq, and, desc, isNull, lte, not, gt, inArray, sql } from 'drizzle-orm';
import * as schema from './schema.js';

const PORT = process.env.PORT ? parseInt(process.env.PORT) : 3101;
const DATABASE_URL = process.env.DATABASE_URL || 'postgresql://spacetime:spacetime@localhost:5433/spacetime_run1_run1_run1';
const CLIENT_ORIGIN = 'http://localhost:5274';

const pool = new Pool({ connectionString: DATABASE_URL });
const db = drizzle(pool, { schema });

const app = express();
app.use(cors({ origin: CLIENT_ORIGIN, credentials: true }));
app.use(express.json());

const httpServer = createServer(app);
const io = new Server(httpServer, {
  cors: { origin: CLIENT_ORIGIN, methods: ['GET', 'POST'], credentials: true },
});

// In-memory state
const typingState = new Map<number, Map<number, ReturnType<typeof setTimeout>>>();
const userSockets = new Map<number, Socket>();
const socketUsers = new Map<string, number>();

// ─── Helpers ─────────────────────────────────────────────────────────────────

async function getMessagesForRoom(roomId: number) {
  const msgs = await db
    .select({
      id: schema.messages.id,
      roomId: schema.messages.roomId,
      userId: schema.messages.userId,
      userName: schema.users.name,
      content: schema.messages.content,
      isEdited: schema.messages.isEdited,
      expiresAt: schema.messages.expiresAt,
      createdAt: schema.messages.createdAt,
      updatedAt: schema.messages.updatedAt,
    })
    .from(schema.messages)
    .innerJoin(schema.users, eq(schema.messages.userId, schema.users.id))
    .where(and(eq(schema.messages.roomId, roomId), isNull(schema.messages.deletedAt)))
    .orderBy(schema.messages.createdAt)
    .limit(100);

  if (msgs.length === 0) return [];

  const messageIds = msgs.map((m) => m.id);

  const rxns = await db
    .select({
      messageId: schema.reactions.messageId,
      emoji: schema.reactions.emoji,
      userId: schema.reactions.userId,
      userName: schema.users.name,
    })
    .from(schema.reactions)
    .innerJoin(schema.users, eq(schema.reactions.userId, schema.users.id))
    .where(inArray(schema.reactions.messageId, messageIds));

  const receipts = await db
    .select({
      messageId: schema.readReceipts.messageId,
      userId: schema.readReceipts.userId,
      userName: schema.users.name,
    })
    .from(schema.readReceipts)
    .innerJoin(schema.users, eq(schema.readReceipts.userId, schema.users.id))
    .where(inArray(schema.readReceipts.messageId, messageIds));

  const reactionsByMsg = new Map<number, Map<string, { emoji: string; count: number; users: string[] }>>();
  for (const rxn of rxns) {
    if (!reactionsByMsg.has(rxn.messageId)) reactionsByMsg.set(rxn.messageId, new Map());
    const emojiMap = reactionsByMsg.get(rxn.messageId)!;
    if (!emojiMap.has(rxn.emoji)) emojiMap.set(rxn.emoji, { emoji: rxn.emoji, count: 0, users: [] });
    const entry = emojiMap.get(rxn.emoji)!;
    entry.count++;
    entry.users.push(rxn.userName);
  }

  const readByMsg = new Map<number, { userId: number; userName: string }[]>();
  for (const r of receipts) {
    if (!readByMsg.has(r.messageId)) readByMsg.set(r.messageId, []);
    readByMsg.get(r.messageId)!.push({ userId: r.userId, userName: r.userName });
  }

  return msgs.map((m) => ({
    ...m,
    reactions: Array.from(reactionsByMsg.get(m.id)?.values() ?? []),
    readBy: readByMsg.get(m.id) ?? [],
  }));
}

async function getSingleMessage(messageId: number) {
  const [msg] = await db
    .select({
      id: schema.messages.id,
      roomId: schema.messages.roomId,
      userId: schema.messages.userId,
      userName: schema.users.name,
      content: schema.messages.content,
      isEdited: schema.messages.isEdited,
      expiresAt: schema.messages.expiresAt,
      createdAt: schema.messages.createdAt,
      updatedAt: schema.messages.updatedAt,
    })
    .from(schema.messages)
    .innerJoin(schema.users, eq(schema.messages.userId, schema.users.id))
    .where(and(eq(schema.messages.id, messageId), isNull(schema.messages.deletedAt)));

  if (!msg) return null;

  const rxns = await db
    .select({
      emoji: schema.reactions.emoji,
      userId: schema.reactions.userId,
      userName: schema.users.name,
    })
    .from(schema.reactions)
    .innerJoin(schema.users, eq(schema.reactions.userId, schema.users.id))
    .where(eq(schema.reactions.messageId, messageId));

  const receipts = await db
    .select({ userId: schema.readReceipts.userId, userName: schema.users.name })
    .from(schema.readReceipts)
    .innerJoin(schema.users, eq(schema.readReceipts.userId, schema.users.id))
    .where(eq(schema.readReceipts.messageId, messageId));

  const emojiMap = new Map<string, { emoji: string; count: number; users: string[] }>();
  for (const rxn of rxns) {
    if (!emojiMap.has(rxn.emoji)) emojiMap.set(rxn.emoji, { emoji: rxn.emoji, count: 0, users: [] });
    const e = emojiMap.get(rxn.emoji)!;
    e.count++;
    e.users.push(rxn.userName);
  }

  return {
    ...msg,
    reactions: Array.from(emojiMap.values()),
    readBy: receipts,
  };
}

async function getUnreadCount(roomId: number, userId: number): Promise<number> {
  const [lr] = await db
    .select()
    .from(schema.lastRead)
    .where(and(eq(schema.lastRead.roomId, roomId), eq(schema.lastRead.userId, userId)));

  const lastReadId = lr?.lastReadMessageId ?? 0;

  const [result] = await db
    .select({ count: sql<number>`count(*)::int` })
    .from(schema.messages)
    .where(
      and(
        eq(schema.messages.roomId, roomId),
        isNull(schema.messages.deletedAt),
        lastReadId > 0 ? gt(schema.messages.id, lastReadId) : undefined,
      ),
    );

  return result?.count ?? 0;
}

async function broadcastPresence(userId: number) {
  const [user] = await db.select().from(schema.users).where(eq(schema.users.id, userId));
  if (!user) return;
  // Don't reveal invisible users as online
  const publicStatus = user.status === 'invisible' ? 'offline' : user.status;
  io.emit('presence_update', {
    userId: user.id,
    name: user.name,
    status: publicStatus,
    lastActive: user.lastActive,
  });
}

// ─── REST Endpoints ───────────────────────────────────────────────────────────

app.post('/api/users/register', async (req, res) => {
  const { name } = req.body as { name: string };
  if (!name || name.trim().length === 0) return res.status(400).json({ error: 'Name required' });
  const trimmed = name.trim().slice(0, 50);
  try {
    const existing = await db.select().from(schema.users).where(eq(schema.users.name, trimmed));
    if (existing.length > 0) {
      await db.update(schema.users)
        .set({ status: 'online', lastActive: new Date() })
        .where(eq(schema.users.id, existing[0].id));
      return res.json({ ...existing[0], status: 'online' });
    }
    const [user] = await db.insert(schema.users).values({ name: trimmed, status: 'online' }).returning();
    return res.json(user);
  } catch {
    return res.status(500).json({ error: 'Registration failed' });
  }
});

app.get('/api/users', async (_req, res) => {
  const users = await db.select().from(schema.users).orderBy(schema.users.name);
  const onlineIds = new Set(userSockets.keys());
  const result = users.map((u) => ({
    ...u,
    status: onlineIds.has(u.id) ? (u.status === 'invisible' ? 'offline' : u.status) : 'offline',
    isOnline: onlineIds.has(u.id) && u.status !== 'invisible',
  }));
  res.json(result);
});

app.get('/api/rooms', async (_req, res) => {
  const rooms = await db.select().from(schema.rooms).orderBy(schema.rooms.createdAt);
  res.json(rooms);
});

app.post('/api/rooms', async (req, res) => {
  const { name, userId } = req.body as { name: string; userId: number };
  if (!name || !userId) return res.status(400).json({ error: 'name and userId required' });
  const trimmed = name.trim().slice(0, 100);
  try {
    const [room] = await db.insert(schema.rooms).values({ name: trimmed, createdBy: userId }).returning();
    await db.insert(schema.roomMembers).values({ roomId: room.id, userId, isAdmin: true });
    return res.json(room);
  } catch {
    return res.status(400).json({ error: 'Room name already exists' });
  }
});

app.post('/api/rooms/:id/join', async (req, res) => {
  const roomId = parseInt(req.params.id);
  const { userId } = req.body as { userId: number };
  const [existing] = await db
    .select()
    .from(schema.roomMembers)
    .where(and(eq(schema.roomMembers.roomId, roomId), eq(schema.roomMembers.userId, userId)));
  if (existing?.isBanned) return res.status(403).json({ error: 'You are banned from this room' });
  if (!existing) {
    await db.insert(schema.roomMembers).values({ roomId, userId }).onConflictDoNothing();
  }
  res.json({ ok: true });
});

app.post('/api/rooms/:id/leave', async (req, res) => {
  const roomId = parseInt(req.params.id);
  const { userId } = req.body as { userId: number };
  await db
    .delete(schema.roomMembers)
    .where(and(eq(schema.roomMembers.roomId, roomId), eq(schema.roomMembers.userId, userId)));
  res.json({ ok: true });
});

app.get('/api/rooms/:id/members', async (req, res) => {
  const roomId = parseInt(req.params.id);
  const members = await db
    .select({
      userId: schema.roomMembers.userId,
      name: schema.users.name,
      isAdmin: schema.roomMembers.isAdmin,
      isBanned: schema.roomMembers.isBanned,
      status: schema.users.status,
      lastActive: schema.users.lastActive,
    })
    .from(schema.roomMembers)
    .innerJoin(schema.users, eq(schema.roomMembers.userId, schema.users.id))
    .where(eq(schema.roomMembers.roomId, roomId));
  const onlineIds = new Set(userSockets.keys());
  res.json(members.map((m) => ({
    ...m,
    isOnline: onlineIds.has(m.userId) && m.status !== 'invisible',
    status: onlineIds.has(m.userId) ? (m.status === 'invisible' ? 'offline' : m.status) : 'offline',
  })));
});

app.get('/api/rooms/:id/messages', async (req, res) => {
  const roomId = parseInt(req.params.id);
  const userId = parseInt(req.query['userId'] as string);
  if (userId) {
    const [member] = await db
      .select()
      .from(schema.roomMembers)
      .where(and(eq(schema.roomMembers.roomId, roomId), eq(schema.roomMembers.userId, userId)));
    if (member?.isBanned) return res.status(403).json({ error: 'Banned' });
  }
  const msgs = await getMessagesForRoom(roomId);
  res.json(msgs);
});

app.get('/api/messages/:id/history', async (req, res) => {
  const messageId = parseInt(req.params.id);
  const edits = await db
    .select()
    .from(schema.messageEdits)
    .where(eq(schema.messageEdits.messageId, messageId))
    .orderBy(schema.messageEdits.editedAt);
  res.json(edits);
});

app.get('/api/rooms/:id/scheduled', async (req, res) => {
  const roomId = parseInt(req.params.id);
  const userId = parseInt(req.query['userId'] as string);
  const scheduled = await db
    .select()
    .from(schema.scheduledMessages)
    .where(
      and(
        eq(schema.scheduledMessages.roomId, roomId),
        eq(schema.scheduledMessages.userId, userId),
        eq(schema.scheduledMessages.sent, false),
        eq(schema.scheduledMessages.cancelled, false),
      ),
    )
    .orderBy(schema.scheduledMessages.scheduledFor);
  res.json(scheduled);
});

app.post('/api/rooms/:id/scheduled', async (req, res) => {
  const roomId = parseInt(req.params.id);
  const { userId, content, scheduledFor } = req.body as { userId: number; content: string; scheduledFor: string };
  if (!content || !scheduledFor) return res.status(400).json({ error: 'content and scheduledFor required' });
  const [msg] = await db
    .insert(schema.scheduledMessages)
    .values({ roomId, userId, content, scheduledFor: new Date(scheduledFor) })
    .returning();
  res.json(msg);
});

app.delete('/api/scheduled/:id', async (req, res) => {
  const id = parseInt(req.params.id);
  const userId = parseInt(req.query['userId'] as string);
  await db
    .update(schema.scheduledMessages)
    .set({ cancelled: true })
    .where(and(eq(schema.scheduledMessages.id, id), eq(schema.scheduledMessages.userId, userId)));
  io.to(`user:${userId}`).emit('scheduled_cancelled', { id });
  res.json({ ok: true });
});

app.post('/api/rooms/:id/kick', async (req, res) => {
  const roomId = parseInt(req.params.id);
  const { adminId, targetUserId } = req.body as { adminId: number; targetUserId: number };
  const [admin] = await db
    .select()
    .from(schema.roomMembers)
    .where(and(eq(schema.roomMembers.roomId, roomId), eq(schema.roomMembers.userId, adminId)));
  if (!admin?.isAdmin) return res.status(403).json({ error: 'Not admin' });
  await db
    .delete(schema.roomMembers)
    .where(and(eq(schema.roomMembers.roomId, roomId), eq(schema.roomMembers.userId, targetUserId)));
  const targetSocket = userSockets.get(targetUserId);
  if (targetSocket) {
    targetSocket.leave(`room:${roomId}`);
    targetSocket.emit('you_kicked', { roomId });
  }
  io.to(`room:${roomId}`).emit('user_kicked', { roomId, userId: targetUserId });
  res.json({ ok: true });
});

app.post('/api/rooms/:id/ban', async (req, res) => {
  const roomId = parseInt(req.params.id);
  const { adminId, targetUserId } = req.body as { adminId: number; targetUserId: number };
  const [admin] = await db
    .select()
    .from(schema.roomMembers)
    .where(and(eq(schema.roomMembers.roomId, roomId), eq(schema.roomMembers.userId, adminId)));
  if (!admin?.isAdmin) return res.status(403).json({ error: 'Not admin' });
  await db
    .update(schema.roomMembers)
    .set({ isBanned: true })
    .where(and(eq(schema.roomMembers.roomId, roomId), eq(schema.roomMembers.userId, targetUserId)));
  const targetSocket = userSockets.get(targetUserId);
  if (targetSocket) {
    targetSocket.leave(`room:${roomId}`);
    targetSocket.emit('you_banned', { roomId });
  }
  io.to(`room:${roomId}`).emit('user_banned', { roomId, userId: targetUserId });
  res.json({ ok: true });
});

app.post('/api/rooms/:id/promote', async (req, res) => {
  const roomId = parseInt(req.params.id);
  const { adminId, targetUserId } = req.body as { adminId: number; targetUserId: number };
  const [admin] = await db
    .select()
    .from(schema.roomMembers)
    .where(and(eq(schema.roomMembers.roomId, roomId), eq(schema.roomMembers.userId, adminId)));
  if (!admin?.isAdmin) return res.status(403).json({ error: 'Not admin' });
  await db
    .update(schema.roomMembers)
    .set({ isAdmin: true })
    .where(and(eq(schema.roomMembers.roomId, roomId), eq(schema.roomMembers.userId, targetUserId)));
  io.to(`room:${roomId}`).emit('member_promoted', { roomId, userId: targetUserId });
  res.json({ ok: true });
});

// ─── Socket.io Events ─────────────────────────────────────────────────────────

io.on('connection', (socket: Socket) => {
  socket.on('authenticate', async ({ userId }: { userId: number }) => {
    userSockets.set(userId, socket);
    socketUsers.set(socket.id, userId);
    socket.join(`user:${userId}`);

    await db.update(schema.users)
      .set({ status: 'online', lastActive: new Date() })
      .where(eq(schema.users.id, userId));

    const [user] = await db.select().from(schema.users).where(eq(schema.users.id, userId));
    if (user) {
      socket.emit('authenticated', user);
      await broadcastPresence(userId);
    }

    // Send online users list
    const allUsers = await db.select().from(schema.users);
    const onlineIds = new Set(userSockets.keys());
    const onlineUsers = allUsers.map((u) => ({
      ...u,
      isOnline: onlineIds.has(u.id) && u.status !== 'invisible',
      status: onlineIds.has(u.id) ? (u.status === 'invisible' ? 'offline' : u.status) : 'offline',
    }));
    io.emit('users_online', onlineUsers);
  });

  socket.on('join_room', async ({ roomId }: { roomId: number }) => {
    const userId = socketUsers.get(socket.id);
    if (!userId) return;

    const [member] = await db
      .select()
      .from(schema.roomMembers)
      .where(and(eq(schema.roomMembers.roomId, roomId), eq(schema.roomMembers.userId, userId)));

    if (member?.isBanned) {
      socket.emit('error', { message: 'You are banned from this room' });
      return;
    }

    socket.join(`room:${roomId}`);
    const messages = await getMessagesForRoom(roomId);

    const members = await db
      .select({
        userId: schema.roomMembers.userId,
        name: schema.users.name,
        isAdmin: schema.roomMembers.isAdmin,
        isBanned: schema.roomMembers.isBanned,
        status: schema.users.status,
        lastActive: schema.users.lastActive,
      })
      .from(schema.roomMembers)
      .innerJoin(schema.users, eq(schema.roomMembers.userId, schema.users.id))
      .where(eq(schema.roomMembers.roomId, roomId));

    const onlineIds = new Set(userSockets.keys());
    const enrichedMembers = members.map((m) => ({
      ...m,
      isOnline: onlineIds.has(m.userId) && m.status !== 'invisible',
      status: onlineIds.has(m.userId) ? (m.status === 'invisible' ? 'offline' : m.status) : 'offline',
    }));

    const [room] = await db.select().from(schema.rooms).where(eq(schema.rooms.id, roomId));
    socket.emit('room_joined', { room, messages, members: enrichedMembers });
  });

  socket.on('leave_room', ({ roomId }: { roomId: number }) => {
    socket.leave(`room:${roomId}`);
  });

  socket.on('send_message', async ({ roomId, content, expiresInSeconds }: {
    roomId: number; content: string; expiresInSeconds?: number;
  }) => {
    const userId = socketUsers.get(socket.id);
    if (!userId) return;
    if (!content || content.trim().length === 0) return;
    if (content.trim().length > 2000) return;

    const [member] = await db
      .select()
      .from(schema.roomMembers)
      .where(and(eq(schema.roomMembers.roomId, roomId), eq(schema.roomMembers.userId, userId)));
    if (!member || member.isBanned) return;

    const expiresAt = expiresInSeconds ? new Date(Date.now() + expiresInSeconds * 1000) : undefined;
    const [msg] = await db
      .insert(schema.messages)
      .values({ roomId, userId, content: content.trim(), expiresAt })
      .returning();

    await db.update(schema.users).set({ lastActive: new Date() }).where(eq(schema.users.id, userId));

    const fullMsg = await getSingleMessage(msg.id);
    if (fullMsg) {
      io.to(`room:${roomId}`).emit('new_message', fullMsg);
    }

    // Update unread counts for all members not currently in the room
    const allMembers = await db
      .select()
      .from(schema.roomMembers)
      .where(and(eq(schema.roomMembers.roomId, roomId), eq(schema.roomMembers.isBanned, false)));

    for (const m of allMembers) {
      if (m.userId !== userId) {
        const count = await getUnreadCount(roomId, m.userId);
        io.to(`user:${m.userId}`).emit('unread_update', { roomId, count });
      }
    }
  });

  socket.on('typing_start', ({ roomId }: { roomId: number }) => {
    const userId = socketUsers.get(socket.id);
    if (!userId) return;

    if (!typingState.has(roomId)) typingState.set(roomId, new Map());
    const roomTyping = typingState.get(roomId)!;

    // Clear existing timeout
    if (roomTyping.has(userId)) clearTimeout(roomTyping.get(userId)!);

    const timeout = setTimeout(() => {
      roomTyping.delete(userId);
      broadcastTyping(roomId);
    }, 5000);

    roomTyping.set(userId, timeout);
    broadcastTyping(roomId);
  });

  socket.on('typing_stop', ({ roomId }: { roomId: number }) => {
    const userId = socketUsers.get(socket.id);
    if (!userId) return;
    const roomTyping = typingState.get(roomId);
    if (roomTyping) {
      if (roomTyping.has(userId)) clearTimeout(roomTyping.get(userId)!);
      roomTyping.delete(userId);
      broadcastTyping(roomId);
    }
  });

  socket.on('mark_read', async ({ roomId, messageId }: { roomId: number; messageId: number }) => {
    const userId = socketUsers.get(socket.id);
    if (!userId) return;

    // Upsert last_read
    await db.insert(schema.lastRead)
      .values({ roomId, userId, lastReadMessageId: messageId, updatedAt: new Date() })
      .onConflictDoUpdate({
        target: [schema.lastRead.roomId, schema.lastRead.userId],
        set: { lastReadMessageId: messageId, updatedAt: new Date() },
      });

    // Insert read receipt for this message and all previous unread
    const msgs = await db
      .select({ id: schema.messages.id })
      .from(schema.messages)
      .where(
        and(
          eq(schema.messages.roomId, roomId),
          isNull(schema.messages.deletedAt),
          lte(schema.messages.id, messageId),
        ),
      );

    for (const msg of msgs) {
      await db.insert(schema.readReceipts)
        .values({ messageId: msg.id, userId })
        .onConflictDoNothing();
    }

    // Broadcast receipt updates for the marked message
    const receipts = await db
      .select({ userId: schema.readReceipts.userId, userName: schema.users.name })
      .from(schema.readReceipts)
      .innerJoin(schema.users, eq(schema.readReceipts.userId, schema.users.id))
      .where(eq(schema.readReceipts.messageId, messageId));

    io.to(`room:${roomId}`).emit('read_receipt_update', { messageId, seenBy: receipts });

    const count = await getUnreadCount(roomId, userId);
    socket.emit('unread_update', { roomId, count });
  });

  socket.on('add_reaction', async ({ messageId, emoji }: { messageId: number; emoji: string }) => {
    const userId = socketUsers.get(socket.id);
    if (!userId) return;

    const [existing] = await db
      .select()
      .from(schema.reactions)
      .where(and(eq(schema.reactions.messageId, messageId), eq(schema.reactions.userId, userId), eq(schema.reactions.emoji, emoji)));

    if (existing) {
      await db.delete(schema.reactions).where(eq(schema.reactions.id, existing.id));
    } else {
      await db.insert(schema.reactions).values({ messageId, userId, emoji });
    }

    const rxns = await db
      .select({ emoji: schema.reactions.emoji, userId: schema.reactions.userId, userName: schema.users.name })
      .from(schema.reactions)
      .innerJoin(schema.users, eq(schema.reactions.userId, schema.users.id))
      .where(eq(schema.reactions.messageId, messageId));

    const emojiMap = new Map<string, { emoji: string; count: number; users: string[] }>();
    for (const rxn of rxns) {
      if (!emojiMap.has(rxn.emoji)) emojiMap.set(rxn.emoji, { emoji: rxn.emoji, count: 0, users: [] });
      const e = emojiMap.get(rxn.emoji)!;
      e.count++;
      e.users.push(rxn.userName);
    }

    const [msg] = await db.select().from(schema.messages).where(eq(schema.messages.id, messageId));
    if (msg) {
      io.to(`room:${msg.roomId}`).emit('reaction_update', {
        messageId,
        reactions: Array.from(emojiMap.values()),
      });
    }
  });

  socket.on('edit_message', async ({ messageId, content }: { messageId: number; content: string }) => {
    const userId = socketUsers.get(socket.id);
    if (!userId) return;
    if (!content || content.trim().length === 0) return;

    const [msg] = await db
      .select()
      .from(schema.messages)
      .where(and(eq(schema.messages.id, messageId), eq(schema.messages.userId, userId), isNull(schema.messages.deletedAt)));

    if (!msg) return;

    // Save old content to history
    await db.insert(schema.messageEdits).values({ messageId, content: msg.content });

    await db.update(schema.messages)
      .set({ content: content.trim(), isEdited: true, updatedAt: new Date() })
      .where(eq(schema.messages.id, messageId));

    const updated = await getSingleMessage(messageId);
    if (updated) {
      io.to(`room:${msg.roomId}`).emit('message_updated', updated);
    }
  });

  socket.on('set_status', async ({ status }: { status: string }) => {
    const userId = socketUsers.get(socket.id);
    if (!userId) return;
    const validStatuses = ['online', 'away', 'dnd', 'invisible'];
    if (!validStatuses.includes(status)) return;
    await db.update(schema.users).set({ status, lastActive: new Date() }).where(eq(schema.users.id, userId));
    await broadcastPresence(userId);

    // Re-emit users_online
    const allUsers = await db.select().from(schema.users);
    const onlineIds = new Set(userSockets.keys());
    io.emit('users_online', allUsers.map((u) => ({
      ...u,
      isOnline: onlineIds.has(u.id) && u.status !== 'invisible',
      status: onlineIds.has(u.id) ? (u.status === 'invisible' ? 'offline' : u.status) : 'offline',
    })));
  });

  socket.on('activity_ping', async () => {
    const userId = socketUsers.get(socket.id);
    if (!userId) return;
    await db.update(schema.users).set({ lastActive: new Date() }).where(eq(schema.users.id, userId));
  });

  socket.on('disconnect', async () => {
    const userId = socketUsers.get(socket.id);
    if (userId) {
      userSockets.delete(userId);
      socketUsers.delete(socket.id);

      // Clear typing for this user
      for (const [roomId, roomTyping] of typingState) {
        if (roomTyping.has(userId)) {
          clearTimeout(roomTyping.get(userId)!);
          roomTyping.delete(userId);
          broadcastTyping(roomId);
        }
      }

      // Update user last_active but keep status
      await db.update(schema.users).set({ lastActive: new Date() }).where(eq(schema.users.id, userId));

      // Emit offline presence
      const [user] = await db.select().from(schema.users).where(eq(schema.users.id, userId));
      if (user) {
        io.emit('presence_update', { userId, name: user.name, status: 'offline', lastActive: new Date() });
      }

      // Re-emit users_online
      const allUsers = await db.select().from(schema.users);
      const onlineIds = new Set(userSockets.keys());
      io.emit('users_online', allUsers.map((u) => ({
        ...u,
        isOnline: onlineIds.has(u.id) && u.status !== 'invisible',
        status: onlineIds.has(u.id) ? (u.status === 'invisible' ? 'offline' : u.status) : 'offline',
      })));
    }
  });
});

// ─── Helpers ─────────────────────────────────────────────────────────────────

async function broadcastTyping(roomId: number) {
  const roomTyping = typingState.get(roomId);
  if (!roomTyping) {
    io.to(`room:${roomId}`).emit('typing_update', { roomId, typingUsers: [] });
    return;
  }
  const userIds = Array.from(roomTyping.keys());
  const names: string[] = [];
  for (const uid of userIds) {
    const [u] = await db.select({ name: schema.users.name }).from(schema.users).where(eq(schema.users.id, uid));
    if (u) names.push(u.name);
  }
  io.to(`room:${roomId}`).emit('typing_update', { roomId, typingUsers: names });
}

// ─── Background Tasks ─────────────────────────────────────────────────────────

// Process scheduled messages every 15 seconds
setInterval(async () => {
  const now = new Date();
  const pending = await db
    .select()
    .from(schema.scheduledMessages)
    .where(
      and(
        eq(schema.scheduledMessages.sent, false),
        eq(schema.scheduledMessages.cancelled, false),
        lte(schema.scheduledMessages.scheduledFor, now),
      ),
    );

  for (const sm of pending) {
    const [msg] = await db
      .insert(schema.messages)
      .values({ roomId: sm.roomId, userId: sm.userId, content: sm.content })
      .returning();

    await db.update(schema.scheduledMessages).set({ sent: true }).where(eq(schema.scheduledMessages.id, sm.id));

    const fullMsg = await getSingleMessage(msg.id);
    if (fullMsg) {
      io.to(`room:${sm.roomId}`).emit('new_message', fullMsg);
      io.to(`user:${sm.userId}`).emit('scheduled_sent', { id: sm.id, roomId: sm.roomId });
    }

    // Update unread counts
    const members = await db
      .select()
      .from(schema.roomMembers)
      .where(and(eq(schema.roomMembers.roomId, sm.roomId), eq(schema.roomMembers.isBanned, false)));

    for (const m of members) {
      if (m.userId !== sm.userId) {
        const count = await getUnreadCount(sm.roomId, m.userId);
        io.to(`user:${m.userId}`).emit('unread_update', { roomId: sm.roomId, count });
      }
    }
  }
}, 15000);

// Delete expired ephemeral messages every 10 seconds
setInterval(async () => {
  const now = new Date();
  const expired = await db
    .select({ id: schema.messages.id, roomId: schema.messages.roomId })
    .from(schema.messages)
    .where(and(isNull(schema.messages.deletedAt), not(isNull(schema.messages.expiresAt)), lte(schema.messages.expiresAt, now)));

  for (const msg of expired) {
    await db.update(schema.messages).set({ deletedAt: now }).where(eq(schema.messages.id, msg.id));
    io.to(`room:${msg.roomId}`).emit('message_deleted', { messageId: msg.id, roomId: msg.roomId });
  }
}, 10000);

// Auto-away: mark users as away after 5 minutes of inactivity
setInterval(async () => {
  const fiveMinutesAgo = new Date(Date.now() - 5 * 60 * 1000);
  const onlineUserIds = Array.from(userSockets.keys());
  if (onlineUserIds.length === 0) return;

  const usersToSetAway = await db
    .select()
    .from(schema.users)
    .where(
      and(
        eq(schema.users.status, 'online'),
        inArray(schema.users.id, onlineUserIds),
        lte(schema.users.lastActive, fiveMinutesAgo),
      ),
    );

  for (const user of usersToSetAway) {
    await db.update(schema.users).set({ status: 'away' }).where(eq(schema.users.id, user.id));
    io.emit('presence_update', { userId: user.id, name: user.name, status: 'away', lastActive: user.lastActive });
  }
}, 60000);

// ─── Start Server ─────────────────────────────────────────────────────────────

httpServer.listen(PORT, () => {
  console.log(`Server running on http://localhost:${PORT}`);
});

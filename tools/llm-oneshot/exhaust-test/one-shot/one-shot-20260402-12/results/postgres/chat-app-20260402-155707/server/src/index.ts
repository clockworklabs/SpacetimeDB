import express from 'express';
import { createServer } from 'http';
import { Server } from 'socket.io';
import { drizzle } from 'drizzle-orm/node-postgres';
import pg from 'pg';
import cors from 'cors';
import dotenv from 'dotenv';
import { eq, and, inArray, not, isNull, or, desc, lt, sql } from 'drizzle-orm';
import * as schema from './schema.js';

dotenv.config();

const { Pool } = pg;

const app = express();
const httpServer = createServer(app);
const io = new Server(httpServer, {
  cors: { origin: '*', methods: ['GET', 'POST'] }
});

const pool = new Pool({
  connectionString: process.env.DATABASE_URL || 'postgresql://spacetime:spacetime@localhost:5433/spacetime_run1_run1'
});

const db = drizzle(pool, { schema });

app.use(cors({ origin: '*' }));
app.use(express.json());

// In-memory state
const typingTimers = new Map<string, NodeJS.Timeout>();
const socketToUser = new Map<string, number>();
const userToSocket = new Map<number, string>();
const onlineUsers = new Set<number>();

// ---- Helpers ----

async function getMessagesForRoom(roomId: number, currentUserId: number) {
  const msgs = await db.select().from(schema.messages)
    .where(and(
      eq(schema.messages.roomId, roomId),
      eq(schema.messages.isDeleted, false),
      eq(schema.messages.isSent, true)
    ))
    .orderBy(schema.messages.createdAt);

  const msgIds = msgs.map(m => m.id);
  if (msgIds.length === 0) return [];

  const receipts = await db.select({
    messageId: schema.messageReadReceipts.messageId,
    userId: schema.messageReadReceipts.userId,
    userName: schema.users.name,
  })
    .from(schema.messageReadReceipts)
    .innerJoin(schema.users, eq(schema.users.id, schema.messageReadReceipts.userId))
    .where(inArray(schema.messageReadReceipts.messageId, msgIds));

  const reactions = await db.select({
    messageId: schema.messageReactions.messageId,
    userId: schema.messageReactions.userId,
    userName: schema.users.name,
    emoji: schema.messageReactions.emoji,
  })
    .from(schema.messageReactions)
    .innerJoin(schema.users, eq(schema.users.id, schema.messageReactions.userId))
    .where(inArray(schema.messageReactions.messageId, msgIds));

  const senderIds = [...new Set(msgs.map(m => m.userId))];
  const senders = senderIds.length > 0
    ? await db.select().from(schema.users).where(inArray(schema.users.id, senderIds))
    : [];
  const senderMap = new Map(senders.map(s => [s.id, s]));

  return msgs.map(msg => {
    const msgReceipts = receipts.filter(r => r.messageId === msg.id);
    const msgReactions = reactions.filter(r => r.messageId === msg.id);

    const reactionMap = new Map<string, { count: number; users: string[]; hasReacted: boolean }>();
    for (const r of msgReactions) {
      const existing = reactionMap.get(r.emoji) || { count: 0, users: [], hasReacted: false };
      existing.count++;
      existing.users.push(r.userName);
      if (r.userId === currentUserId) existing.hasReacted = true;
      reactionMap.set(r.emoji, existing);
    }

    return {
      id: msg.id,
      roomId: msg.roomId,
      userId: msg.userId,
      userName: senderMap.get(msg.userId)?.name ?? 'Unknown',
      content: msg.content,
      createdAt: msg.createdAt,
      expiresAt: msg.expiresAt,
      scheduledFor: msg.scheduledFor,
      isSent: msg.isSent,
      isDeleted: msg.isDeleted,
      readBy: msgReceipts.map(r => ({ userId: r.userId, userName: r.userName })),
      reactions: Array.from(reactionMap.entries()).map(([emoji, data]) => ({ emoji, ...data })),
    };
  });
}

async function getUnreadCount(roomId: number, userId: number): Promise<number> {
  const result = await db.select({ count: sql<number>`count(*)::int` })
    .from(schema.messages)
    .where(and(
      eq(schema.messages.roomId, roomId),
      eq(schema.messages.isDeleted, false),
      eq(schema.messages.isSent, true),
      not(inArray(
        schema.messages.id,
        db.select({ id: schema.messageReadReceipts.messageId })
          .from(schema.messageReadReceipts)
          .where(eq(schema.messageReadReceipts.userId, userId))
      ))
    ));
  return result[0]?.count ?? 0;
}

// ---- REST Routes ----

// Register or get user by name
app.post('/api/users', async (req, res) => {
  const { name } = req.body as { name: string };
  if (!name || name.trim().length === 0 || name.trim().length > 30) {
    res.status(400).json({ error: 'Name must be 1-30 characters' });
    return;
  }
  const trimmed = name.trim();
  try {
    const existing = await db.select().from(schema.users).where(eq(schema.users.name, trimmed));
    if (existing.length > 0) {
      res.json(existing[0]);
    } else {
      const created = await db.insert(schema.users).values({ name: trimmed }).returning();
      res.json(created[0]);
    }
  } catch (e) {
    res.status(500).json({ error: 'Failed to create user' });
  }
});

// Get all users
app.get('/api/users', async (_req, res) => {
  const users = await db.select().from(schema.users).orderBy(schema.users.name);
  const result = users.map(u => ({
    ...u,
    isOnline: onlineUsers.has(u.id),
  }));
  res.json(result);
});

// Update user status
app.put('/api/users/:id/status', async (req, res) => {
  const userId = parseInt(req.params.id);
  const { status } = req.body as { status: string };
  const valid = ['online', 'away', 'dnd', 'invisible'];
  if (!valid.includes(status)) {
    res.status(400).json({ error: 'Invalid status' });
    return;
  }
  await db.update(schema.users).set({ status, lastActive: new Date() }).where(eq(schema.users.id, userId));
  io.emit('user_status_update', { userId, status });
  res.json({ ok: true });
});

// List rooms
app.get('/api/rooms', async (_req, res) => {
  const rooms = await db.select().from(schema.rooms).orderBy(schema.rooms.createdAt);
  res.json(rooms);
});

// Create room
app.post('/api/rooms', async (req, res) => {
  const { name, userId } = req.body as { name: string; userId: number };
  if (!name || name.trim().length === 0 || name.trim().length > 50) {
    res.status(400).json({ error: 'Room name must be 1-50 characters' });
    return;
  }
  try {
    const room = await db.insert(schema.rooms).values({ name: name.trim(), createdBy: userId }).returning();
    // Creator auto-joins
    await db.insert(schema.roomMembers).values({ roomId: room[0].id, userId }).onConflictDoNothing();
    io.emit('room_created', room[0]);
    res.json(room[0]);
  } catch (e: any) {
    if (e.code === '23505') {
      res.status(409).json({ error: 'Room name already exists' });
    } else {
      res.status(500).json({ error: 'Failed to create room' });
    }
  }
});

// Join room
app.post('/api/rooms/:id/join', async (req, res) => {
  const roomId = parseInt(req.params.id);
  const { userId } = req.body as { userId: number };
  await db.insert(schema.roomMembers).values({ roomId, userId }).onConflictDoNothing();
  res.json({ ok: true });
});

// Leave room
app.post('/api/rooms/:id/leave', async (req, res) => {
  const roomId = parseInt(req.params.id);
  const { userId } = req.body as { userId: number };
  await db.delete(schema.roomMembers).where(
    and(eq(schema.roomMembers.roomId, roomId), eq(schema.roomMembers.userId, userId))
  );
  res.json({ ok: true });
});

// Get room members
app.get('/api/rooms/:id/members', async (req, res) => {
  const roomId = parseInt(req.params.id);
  const members = await db.select({
    userId: schema.roomMembers.userId,
    userName: schema.users.name,
    status: schema.users.status,
    lastActive: schema.users.lastActive,
  })
    .from(schema.roomMembers)
    .innerJoin(schema.users, eq(schema.users.id, schema.roomMembers.userId))
    .where(eq(schema.roomMembers.roomId, roomId));
  res.json(members.map(m => ({ ...m, isOnline: onlineUsers.has(m.userId) })));
});

// Get messages for room
app.get('/api/rooms/:id/messages', async (req, res) => {
  const roomId = parseInt(req.params.id);
  const userId = parseInt(req.query.userId as string);
  const msgs = await getMessagesForRoom(roomId, userId);
  res.json(msgs);
});

// Get scheduled messages (pending) for a user
app.get('/api/rooms/:id/scheduled', async (req, res) => {
  const roomId = parseInt(req.params.id);
  const userId = parseInt(req.query.userId as string);
  const msgs = await db.select().from(schema.messages)
    .where(and(
      eq(schema.messages.roomId, roomId),
      eq(schema.messages.userId, userId),
      eq(schema.messages.isSent, false),
      eq(schema.messages.isDeleted, false)
    ))
    .orderBy(schema.messages.scheduledFor);
  res.json(msgs);
});

// Cancel scheduled message
app.delete('/api/messages/:id/schedule', async (req, res) => {
  const msgId = parseInt(req.params.id);
  const { userId } = req.body as { userId: number };
  await db.update(schema.messages)
    .set({ isDeleted: true })
    .where(and(
      eq(schema.messages.id, msgId),
      eq(schema.messages.userId, userId),
      eq(schema.messages.isSent, false)
    ));
  res.json({ ok: true });
});

// ---- Socket.io ----

io.on('connection', (socket) => {
  console.log(`Socket connected: ${socket.id}`);

  socket.on('authenticate', async (data: { userId: number }) => {
    const { userId } = data;
    socketToUser.set(socket.id, userId);
    userToSocket.set(userId, socket.id);
    onlineUsers.add(userId);

    await db.update(schema.users)
      .set({ status: 'online', lastActive: new Date() })
      .where(eq(schema.users.id, userId));

    io.emit('user_online', { userId });

    // Join socket rooms for all chat rooms the user is a member of
    const memberships = await db.select({ roomId: schema.roomMembers.roomId })
      .from(schema.roomMembers)
      .where(eq(schema.roomMembers.userId, userId));
    for (const m of memberships) {
      socket.join(`room:${m.roomId}`);
    }
  });

  socket.on('join_room', async (data: { roomId: number; userId: number }) => {
    const { roomId, userId } = data;
    socket.join(`room:${roomId}`);
    io.to(`room:${roomId}`).emit('user_joined_room', { roomId, userId });
  });

  socket.on('leave_room', (data: { roomId: number; userId: number }) => {
    const { roomId } = data;
    socket.leave(`room:${roomId}`);
  });

  socket.on('send_message', async (data: {
    roomId: number;
    userId: number;
    content: string;
    expiresInMs?: number;
    scheduledFor?: string;
  }) => {
    const { roomId, userId, content, expiresInMs, scheduledFor } = data;

    if (!content || content.trim().length === 0 || content.trim().length > 2000) return;

    const expiresAt = expiresInMs ? new Date(Date.now() + expiresInMs) : null;
    const scheduled = scheduledFor ? new Date(scheduledFor) : null;
    const isSent = !scheduled;

    const [msg] = await db.insert(schema.messages).values({
      roomId,
      userId,
      content: content.trim(),
      expiresAt: expiresAt ?? undefined,
      scheduledFor: scheduled ?? undefined,
      isSent,
    }).returning();

    if (isSent) {
      const sender = await db.select().from(schema.users).where(eq(schema.users.id, userId));
      const enriched = {
        id: msg.id,
        roomId: msg.roomId,
        userId: msg.userId,
        userName: sender[0]?.name ?? 'Unknown',
        content: msg.content,
        createdAt: msg.createdAt,
        expiresAt: msg.expiresAt,
        scheduledFor: msg.scheduledFor,
        isSent: msg.isSent,
        isDeleted: msg.isDeleted,
        readBy: [],
        reactions: [],
      };
      io.to(`room:${roomId}`).emit('new_message', enriched);
    } else {
      // Notify the sender about their scheduled message
      socket.emit('scheduled_message_created', msg);
    }
  });

  socket.on('start_typing', (data: { roomId: number; userId: number; userName: string }) => {
    const { roomId, userId, userName } = data;
    const key = `${roomId}:${userId}`;

    // Clear existing timer
    const existing = typingTimers.get(key);
    if (existing) clearTimeout(existing);

    // Broadcast typing
    socket.to(`room:${roomId}`).emit('typing_update', { roomId, userId, userName, isTyping: true });

    // Auto-expire after 4 seconds
    const timer = setTimeout(() => {
      typingTimers.delete(key);
      socket.to(`room:${roomId}`).emit('typing_update', { roomId, userId, userName, isTyping: false });
    }, 4000);
    typingTimers.set(key, timer);
  });

  socket.on('stop_typing', (data: { roomId: number; userId: number; userName: string }) => {
    const { roomId, userId, userName } = data;
    const key = `${roomId}:${userId}`;
    const existing = typingTimers.get(key);
    if (existing) {
      clearTimeout(existing);
      typingTimers.delete(key);
    }
    socket.to(`room:${roomId}`).emit('typing_update', { roomId, userId, userName, isTyping: false });
  });

  socket.on('mark_read', async (data: { roomId: number; userId: number; messageIds: number[] }) => {
    const { roomId, userId, messageIds } = data;
    if (!messageIds.length) return;

    // Insert read receipts
    for (const msgId of messageIds) {
      await db.insert(schema.messageReadReceipts)
        .values({ messageId: msgId, userId })
        .onConflictDoNothing();
    }

    const user = await db.select().from(schema.users).where(eq(schema.users.id, userId));
    const userName = user[0]?.name ?? 'Unknown';

    io.to(`room:${roomId}`).emit('messages_read', {
      roomId,
      userId,
      userName,
      messageIds,
    });
  });

  socket.on('toggle_reaction', async (data: { messageId: number; userId: number; emoji: string; roomId: number }) => {
    const { messageId, userId, emoji, roomId } = data;

    const existing = await db.select()
      .from(schema.messageReactions)
      .where(and(
        eq(schema.messageReactions.messageId, messageId),
        eq(schema.messageReactions.userId, userId),
        eq(schema.messageReactions.emoji, emoji)
      ));

    if (existing.length > 0) {
      await db.delete(schema.messageReactions).where(
        and(
          eq(schema.messageReactions.messageId, messageId),
          eq(schema.messageReactions.userId, userId),
          eq(schema.messageReactions.emoji, emoji)
        )
      );
    } else {
      await db.insert(schema.messageReactions).values({ messageId, userId, emoji }).onConflictDoNothing();
    }

    // Get updated reactions for the message
    const reactions = await db.select({
      userId: schema.messageReactions.userId,
      userName: schema.users.name,
      emoji: schema.messageReactions.emoji,
    })
      .from(schema.messageReactions)
      .innerJoin(schema.users, eq(schema.users.id, schema.messageReactions.userId))
      .where(eq(schema.messageReactions.messageId, messageId));

    const reactionMap = new Map<string, { count: number; users: string[] }>();
    for (const r of reactions) {
      const entry = reactionMap.get(r.emoji) || { count: 0, users: [] };
      entry.count++;
      entry.users.push(r.userName);
      reactionMap.set(r.emoji, entry);
    }

    io.to(`room:${roomId}`).emit('reaction_update', {
      messageId,
      reactions: Array.from(reactionMap.entries()).map(([emoji, data]) => ({ emoji, ...data })),
    });
  });

  socket.on('disconnect', async () => {
    const userId = socketToUser.get(socket.id);
    if (userId) {
      socketToUser.delete(socket.id);
      userToSocket.delete(userId);
      onlineUsers.delete(userId);

      await db.update(schema.users)
        .set({ lastActive: new Date() })
        .where(eq(schema.users.id, userId));

      io.emit('user_offline', { userId });
    }
    console.log(`Socket disconnected: ${socket.id}`);
  });
});

// ---- Background Jobs ----

// Send scheduled messages
setInterval(async () => {
  const now = new Date();
  const due = await db.select().from(schema.messages)
    .where(and(
      eq(schema.messages.isSent, false),
      eq(schema.messages.isDeleted, false),
      lt(schema.messages.scheduledFor, now)
    ));

  for (const msg of due) {
    await db.update(schema.messages)
      .set({ isSent: true, createdAt: now })
      .where(eq(schema.messages.id, msg.id));

    const sender = await db.select().from(schema.users).where(eq(schema.users.id, msg.userId));
    const enriched = {
      id: msg.id,
      roomId: msg.roomId,
      userId: msg.userId,
      userName: sender[0]?.name ?? 'Unknown',
      content: msg.content,
      createdAt: now,
      expiresAt: msg.expiresAt,
      scheduledFor: msg.scheduledFor,
      isSent: true,
      isDeleted: false,
      readBy: [],
      reactions: [],
    };
    io.to(`room:${msg.roomId}`).emit('new_message', enriched);
  }
}, 5000);

// Delete expired ephemeral messages
setInterval(async () => {
  const now = new Date();
  const expired = await db.select().from(schema.messages)
    .where(and(
      eq(schema.messages.isDeleted, false),
      eq(schema.messages.isSent, true),
      lt(schema.messages.expiresAt, now)
    ));

  for (const msg of expired) {
    await db.update(schema.messages)
      .set({ isDeleted: true })
      .where(eq(schema.messages.id, msg.id));

    io.to(`room:${msg.roomId}`).emit('message_deleted', { messageId: msg.id, roomId: msg.roomId });
  }
}, 5000);

const PORT = parseInt(process.env.PORT || '3101');
httpServer.listen(PORT, () => {
  console.log(`Chat server running on port ${PORT}`);
});

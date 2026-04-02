import express from 'express';
import { createServer } from 'http';
import { Server } from 'socket.io';
import cors from 'cors';
import 'dotenv/config';
import { drizzle } from 'drizzle-orm/node-postgres';
import { Pool } from 'pg';
import { eq, and, lt, isNotNull, inArray, sql } from 'drizzle-orm';
import {
  users,
  rooms,
  roomMembers,
  messages,
  messageEdits,
  readReceipts,
  userRoomReads,
  reactions,
} from './schema.js';

const PORT = parseInt(process.env.PORT || '3101');
const DATABASE_URL =
  process.env.DATABASE_URL ||
  'postgresql://spacetime:spacetime@localhost:5433/spacetime_run1_run1_run1';

const pool = new Pool({ connectionString: DATABASE_URL });
const db = drizzle(pool);

const app = express();
const httpServer = createServer(app);

const io = new Server(httpServer, {
  cors: {
    origin: [
      'http://localhost:5274',
      'http://127.0.0.1:5274',
      'http://localhost:5173',
    ],
    methods: ['GET', 'POST', 'PUT', 'DELETE'],
    credentials: true,
  },
});

app.use(
  cors({
    origin: [
      'http://localhost:5274',
      'http://127.0.0.1:5274',
      'http://localhost:5173',
    ],
    credentials: true,
  }),
);
app.use(express.json());

// In-memory typing state: roomId -> userId -> { username, timer }
const typingState = new Map<
  number,
  Map<number, { username: string; timer: ReturnType<typeof setTimeout> }>
>();

// Socket tracking
const socketToUser = new Map<string, number>();
const userSockets = new Map<number, Set<string>>();

function broadcastTyping(roomId: number) {
  const roomTyping = typingState.get(roomId);
  const typingUsers = roomTyping
    ? Array.from(roomTyping.values()).map((t) => t.username)
    : [];
  io.to(`room:${roomId}`).emit('typing-update', { roomId, typingUsers });
}

// Background: process scheduled messages
setInterval(async () => {
  try {
    const now = new Date();
    const pending = await db
      .select()
      .from(messages)
      .where(
        and(
          eq(messages.isSent, false),
          eq(messages.isDeleted, false),
          isNotNull(messages.scheduledAt),
          lt(messages.scheduledAt, now),
        ),
      );

    for (const msg of pending) {
      await db
        .update(messages)
        .set({ isSent: true })
        .where(eq(messages.id, msg.id));

      const [user] = await db
        .select()
        .from(users)
        .where(eq(users.id, msg.userId));
      const msgFull = {
        ...msg,
        isSent: true,
        username: user?.username || 'Unknown',
        reactions: [] as ReactionGroup[],
        readers: [] as { userId: number; username: string }[],
      };
      io.to(`room:${msg.roomId}`).emit('new-message', msgFull);
    }
  } catch (e) {
    console.error('Scheduled message processor error:', e);
  }
}, 10000);

// Background: clean up expired ephemeral messages
setInterval(async () => {
  try {
    const now = new Date();
    const expired = await db
      .select({ id: messages.id, roomId: messages.roomId })
      .from(messages)
      .where(
        and(
          eq(messages.isDeleted, false),
          isNotNull(messages.expiresAt),
          lt(messages.expiresAt, now),
        ),
      );

    for (const msg of expired) {
      await db
        .update(messages)
        .set({ isDeleted: true })
        .where(eq(messages.id, msg.id));
      io.to(`room:${msg.roomId}`).emit('message-deleted', {
        messageId: msg.id,
      });
    }
  } catch (e) {
    console.error('Ephemeral message cleaner error:', e);
  }
}, 10000);

// Background: auto-set to away after 5 min inactivity (online only)
setInterval(async () => {
  try {
    const fiveMinAgo = new Date(Date.now() - 5 * 60 * 1000);
    const staleOnline = await db
      .select()
      .from(users)
      .where(
        and(eq(users.status, 'online'), lt(users.lastActive, fiveMinAgo)),
      );

    for (const user of staleOnline) {
      const sockets = userSockets.get(user.id);
      if (!sockets || sockets.size === 0) {
        await db
          .update(users)
          .set({ status: 'away' })
          .where(eq(users.id, user.id));
        io.emit('status-changed', {
          userId: user.id,
          status: 'away',
          lastActive: user.lastActive,
        });
      }
    }
  } catch (e) {
    console.error('Away auto-set error:', e);
  }
}, 30000);

// Types
interface ReactionGroup {
  emoji: string;
  count: number;
  users: string[];
  userIds: number[];
}

async function buildMessagesWithMeta(roomId: number) {
  const msgs = await db
    .select({
      id: messages.id,
      roomId: messages.roomId,
      userId: messages.userId,
      content: messages.content,
      createdAt: messages.createdAt,
      expiresAt: messages.expiresAt,
      scheduledAt: messages.scheduledAt,
      isSent: messages.isSent,
      isDeleted: messages.isDeleted,
      editedAt: messages.editedAt,
      username: users.username,
    })
    .from(messages)
    .innerJoin(users, eq(messages.userId, users.id))
    .where(
      and(
        eq(messages.roomId, roomId),
        eq(messages.isSent, true),
        eq(messages.isDeleted, false),
      ),
    )
    .orderBy(messages.createdAt)
    .limit(100);

  if (msgs.length === 0) return [];

  const messageIds = msgs.map((m) => m.id);

  const allReactions = await db
    .select({
      messageId: reactions.messageId,
      emoji: reactions.emoji,
      userId: reactions.userId,
      username: users.username,
    })
    .from(reactions)
    .innerJoin(users, eq(reactions.userId, users.id))
    .where(inArray(reactions.messageId, messageIds));

  const allReceipts = await db
    .select({
      messageId: readReceipts.messageId,
      userId: readReceipts.userId,
      username: users.username,
    })
    .from(readReceipts)
    .innerJoin(users, eq(readReceipts.userId, users.id))
    .where(inArray(readReceipts.messageId, messageIds));

  const reactionsByMsg = new Map<
    number,
    Map<string, { count: number; users: string[]; userIds: number[] }>
  >();
  for (const r of allReactions) {
    if (!reactionsByMsg.has(r.messageId))
      reactionsByMsg.set(r.messageId, new Map());
    const map = reactionsByMsg.get(r.messageId)!;
    if (!map.has(r.emoji)) map.set(r.emoji, { count: 0, users: [], userIds: [] });
    const entry = map.get(r.emoji)!;
    entry.count++;
    entry.users.push(r.username);
    entry.userIds.push(r.userId);
  }

  const receiptsByMsg = new Map<
    number,
    { userId: number; username: string }[]
  >();
  for (const r of allReceipts) {
    if (!receiptsByMsg.has(r.messageId)) receiptsByMsg.set(r.messageId, []);
    receiptsByMsg.get(r.messageId)!.push({ userId: r.userId, username: r.username });
  }

  return msgs.map((m) => ({
    ...m,
    reactions: Array.from(
      (reactionsByMsg.get(m.id) || new Map()).entries(),
    ).map(([emoji, data]) => ({
      emoji,
      count: data.count,
      users: data.users,
      userIds: data.userIds,
    })),
    readers: receiptsByMsg.get(m.id) || [],
  }));
}

async function getReactionsForMessage(messageId: number): Promise<ReactionGroup[]> {
  const rows = await db
    .select({
      emoji: reactions.emoji,
      userId: reactions.userId,
      username: users.username,
    })
    .from(reactions)
    .innerJoin(users, eq(reactions.userId, users.id))
    .where(eq(reactions.messageId, messageId));

  const map = new Map<string, { count: number; users: string[]; userIds: number[] }>();
  for (const r of rows) {
    if (!map.has(r.emoji)) map.set(r.emoji, { count: 0, users: [], userIds: [] });
    const entry = map.get(r.emoji)!;
    entry.count++;
    entry.users.push(r.username);
    entry.userIds.push(r.userId);
  }
  return Array.from(map.entries()).map(([emoji, data]) => ({
    emoji,
    count: data.count,
    users: data.users,
    userIds: data.userIds,
  }));
}

// ─── REST ROUTES ────────────────────────────────────────────────────────────

// Register / login
app.post('/api/users', async (req, res) => {
  try {
    const { username } = req.body as { username?: string };
    if (!username) return res.status(400).json({ error: 'Username required' });
    const name = username.trim().slice(0, 50);
    if (!name) return res.status(400).json({ error: 'Username required' });

    const existing = await db
      .select()
      .from(users)
      .where(eq(users.username, name));
    if (existing.length > 0) {
      await db
        .update(users)
        .set({ status: 'online', lastActive: new Date() })
        .where(eq(users.id, existing[0].id));
      return res.json({ ...existing[0], status: 'online' });
    }

    const [newUser] = await db
      .insert(users)
      .values({ username: name, status: 'online', lastActive: new Date() })
      .returning();
    return res.json(newUser);
  } catch (e: unknown) {
    console.error(e);
    return res.status(500).json({ error: String(e) });
  }
});

// Get all users
app.get('/api/users', async (_req, res) => {
  try {
    const all = await db.select().from(users).orderBy(users.username);
    return res.json(all);
  } catch (e: unknown) {
    return res.status(500).json({ error: String(e) });
  }
});

// Update status
app.put('/api/users/:id/status', async (req, res) => {
  try {
    const userId = parseInt(req.params.id);
    const { status } = req.body as { status: string };
    const valid = ['online', 'away', 'dnd', 'invisible'];
    if (!valid.includes(status)) return res.status(400).json({ error: 'Invalid status' });

    await db
      .update(users)
      .set({ status, lastActive: new Date() })
      .where(eq(users.id, userId));

    const [user] = await db.select().from(users).where(eq(users.id, userId));
    io.emit('status-changed', {
      userId,
      status,
      lastActive: user?.lastActive,
      username: user?.username,
    });
    return res.json({ ok: true });
  } catch (e: unknown) {
    return res.status(500).json({ error: String(e) });
  }
});

// Get rooms
app.get('/api/rooms', async (_req, res) => {
  try {
    const all = await db.select().from(rooms).orderBy(rooms.createdAt);
    return res.json(all);
  } catch (e: unknown) {
    return res.status(500).json({ error: String(e) });
  }
});

// Create room
app.post('/api/rooms', async (req, res) => {
  try {
    const { name, userId } = req.body as { name?: string; userId?: number };
    if (!name || !userId) return res.status(400).json({ error: 'name and userId required' });
    const trimmed = name.trim().slice(0, 100);
    if (!trimmed) return res.status(400).json({ error: 'Room name required' });

    const [room] = await db
      .insert(rooms)
      .values({ name: trimmed, creatorId: userId })
      .returning();

    await db
      .insert(roomMembers)
      .values({ roomId: room.id, userId, isAdmin: true })
      .onConflictDoNothing();

    io.emit('room-created', room);
    return res.json(room);
  } catch (e: unknown) {
    const err = e as { code?: string; message?: string };
    if (err.code === '23505') return res.status(409).json({ error: 'Room name already exists' });
    return res.status(500).json({ error: String(e) });
  }
});

// Join room
app.post('/api/rooms/:id/join', async (req, res) => {
  try {
    const roomId = parseInt(req.params.id);
    const { userId } = req.body as { userId: number };

    const [existing] = await db
      .select()
      .from(roomMembers)
      .where(and(eq(roomMembers.roomId, roomId), eq(roomMembers.userId, userId)));

    if (existing?.isBanned) return res.status(403).json({ error: 'You are banned from this room' });

    await db
      .insert(roomMembers)
      .values({ roomId, userId, isAdmin: false })
      .onConflictDoNothing();

    const [user] = await db.select().from(users).where(eq(users.id, userId));
    io.to(`room:${roomId}`).emit('member-joined', {
      roomId,
      userId,
      username: user?.username,
    });
    return res.json({ ok: true });
  } catch (e: unknown) {
    return res.status(500).json({ error: String(e) });
  }
});

// Leave room
app.post('/api/rooms/:id/leave', async (req, res) => {
  try {
    const roomId = parseInt(req.params.id);
    const { userId } = req.body as { userId: number };

    await db
      .delete(roomMembers)
      .where(and(eq(roomMembers.roomId, roomId), eq(roomMembers.userId, userId)));

    io.to(`room:${roomId}`).emit('member-left', { roomId, userId });
    return res.json({ ok: true });
  } catch (e: unknown) {
    return res.status(500).json({ error: String(e) });
  }
});

// Get members
app.get('/api/rooms/:id/members', async (req, res) => {
  try {
    const roomId = parseInt(req.params.id);
    const members = await db
      .select({
        userId: roomMembers.userId,
        isAdmin: roomMembers.isAdmin,
        isBanned: roomMembers.isBanned,
        joinedAt: roomMembers.joinedAt,
        username: users.username,
        status: users.status,
        lastActive: users.lastActive,
      })
      .from(roomMembers)
      .innerJoin(users, eq(roomMembers.userId, users.id))
      .where(and(eq(roomMembers.roomId, roomId), eq(roomMembers.isBanned, false)));
    return res.json(members);
  } catch (e: unknown) {
    return res.status(500).json({ error: String(e) });
  }
});

// Get messages
app.get('/api/rooms/:id/messages', async (req, res) => {
  try {
    const roomId = parseInt(req.params.id);
    const userId = parseInt((req.query.userId as string) || '0');

    if (userId) {
      const [member] = await db
        .select()
        .from(roomMembers)
        .where(and(eq(roomMembers.roomId, roomId), eq(roomMembers.userId, userId)));
      if (!member || member.isBanned) return res.status(403).json({ error: 'Access denied' });
    }

    const result = await buildMessagesWithMeta(roomId);
    return res.json(result);
  } catch (e: unknown) {
    return res.status(500).json({ error: String(e) });
  }
});

// Send message
app.post('/api/rooms/:id/messages', async (req, res) => {
  try {
    const roomId = parseInt(req.params.id);
    const { userId, content, expiresInSeconds, scheduledAt } = req.body as {
      userId: number;
      content: string;
      expiresInSeconds?: number;
      scheduledAt?: string;
    };

    if (!content?.trim()) return res.status(400).json({ error: 'Content required' });
    if (content.length > 2000) return res.status(400).json({ error: 'Message too long' });

    const [member] = await db
      .select()
      .from(roomMembers)
      .where(and(eq(roomMembers.roomId, roomId), eq(roomMembers.userId, userId)));
    if (!member || member.isBanned) return res.status(403).json({ error: 'Access denied' });

    const expiresAt = expiresInSeconds
      ? new Date(Date.now() + expiresInSeconds * 1000)
      : null;
    const scheduledDate = scheduledAt ? new Date(scheduledAt) : null;
    const isSent = !scheduledDate;

    const [msg] = await db
      .insert(messages)
      .values({
        roomId,
        userId,
        content: content.trim(),
        expiresAt,
        scheduledAt: scheduledDate,
        isSent,
      })
      .returning();

    const [user] = await db.select().from(users).where(eq(users.id, userId));
    await db
      .update(users)
      .set({ lastActive: new Date() })
      .where(eq(users.id, userId));

    const msgFull = {
      ...msg,
      username: user?.username || 'Unknown',
      reactions: [] as ReactionGroup[],
      readers: [] as { userId: number; username: string }[],
    };

    if (isSent) {
      io.to(`room:${roomId}`).emit('new-message', msgFull);
    }

    return res.json(msgFull);
  } catch (e: unknown) {
    return res.status(500).json({ error: String(e) });
  }
});

// Get scheduled messages for user
app.get('/api/rooms/:id/messages/scheduled', async (req, res) => {
  try {
    const roomId = parseInt(req.params.id);
    const userId = parseInt((req.query.userId as string) || '0');

    const scheduled = await db
      .select({
        id: messages.id,
        content: messages.content,
        scheduledAt: messages.scheduledAt,
        createdAt: messages.createdAt,
      })
      .from(messages)
      .where(
        and(
          eq(messages.roomId, roomId),
          eq(messages.userId, userId),
          eq(messages.isSent, false),
          eq(messages.isDeleted, false),
        ),
      )
      .orderBy(messages.scheduledAt);

    return res.json(scheduled);
  } catch (e: unknown) {
    return res.status(500).json({ error: String(e) });
  }
});

// Cancel scheduled message
app.delete('/api/messages/:id/scheduled', async (req, res) => {
  try {
    const messageId = parseInt(req.params.id);
    const { userId } = req.body as { userId: number };

    const [msg] = await db.select().from(messages).where(eq(messages.id, messageId));
    if (!msg || msg.userId !== userId) return res.status(403).json({ error: 'Forbidden' });
    if (msg.isSent) return res.status(400).json({ error: 'Message already sent' });

    await db
      .update(messages)
      .set({ isDeleted: true })
      .where(eq(messages.id, messageId));

    return res.json({ ok: true });
  } catch (e: unknown) {
    return res.status(500).json({ error: String(e) });
  }
});

// Edit message
app.put('/api/messages/:id', async (req, res) => {
  try {
    const messageId = parseInt(req.params.id);
    const { userId, content } = req.body as { userId: number; content: string };

    if (!content?.trim()) return res.status(400).json({ error: 'Content required' });

    const [msg] = await db.select().from(messages).where(eq(messages.id, messageId));
    if (!msg || msg.userId !== userId) return res.status(403).json({ error: 'Forbidden' });
    if (msg.isDeleted) return res.status(404).json({ error: 'Not found' });

    await db.insert(messageEdits).values({ messageId, oldContent: msg.content });

    const [updated] = await db
      .update(messages)
      .set({ content: content.trim(), editedAt: new Date() })
      .where(eq(messages.id, messageId))
      .returning();

    const [user] = await db.select().from(users).where(eq(users.id, userId));
    const reactionList = await getReactionsForMessage(messageId);
    const receiptRows = await db
      .select({ userId: readReceipts.userId, username: users.username })
      .from(readReceipts)
      .innerJoin(users, eq(readReceipts.userId, users.id))
      .where(eq(readReceipts.messageId, messageId));

    const msgFull = {
      ...updated,
      username: user?.username || 'Unknown',
      reactions: reactionList,
      readers: receiptRows,
    };

    io.to(`room:${msg.roomId}`).emit('message-edited', msgFull);
    return res.json(msgFull);
  } catch (e: unknown) {
    return res.status(500).json({ error: String(e) });
  }
});

// Get edit history
app.get('/api/messages/:id/history', async (req, res) => {
  try {
    const messageId = parseInt(req.params.id);
    const history = await db
      .select()
      .from(messageEdits)
      .where(eq(messageEdits.messageId, messageId))
      .orderBy(messageEdits.editedAt);
    return res.json(history);
  } catch (e: unknown) {
    return res.status(500).json({ error: String(e) });
  }
});

// Toggle reaction
app.post('/api/messages/:id/react', async (req, res) => {
  try {
    const messageId = parseInt(req.params.id);
    const { userId, emoji } = req.body as { userId: number; emoji: string };
    if (!emoji) return res.status(400).json({ error: 'Emoji required' });

    const [msg] = await db.select().from(messages).where(eq(messages.id, messageId));
    if (!msg || msg.isDeleted) return res.status(404).json({ error: 'Not found' });

    const existing = await db
      .select()
      .from(reactions)
      .where(
        and(
          eq(reactions.messageId, messageId),
          eq(reactions.userId, userId),
          eq(reactions.emoji, emoji),
        ),
      );

    if (existing.length > 0) {
      await db
        .delete(reactions)
        .where(
          and(
            eq(reactions.messageId, messageId),
            eq(reactions.userId, userId),
            eq(reactions.emoji, emoji),
          ),
        );
    } else {
      await db.insert(reactions).values({ messageId, userId, emoji });
    }

    const reactionList = await getReactionsForMessage(messageId);
    io.to(`room:${msg.roomId}`).emit('reaction-updated', {
      messageId,
      reactions: reactionList,
    });
    return res.json(reactionList);
  } catch (e: unknown) {
    return res.status(500).json({ error: String(e) });
  }
});

// Mark messages as read
app.post('/api/rooms/:id/read', async (req, res) => {
  try {
    const roomId = parseInt(req.params.id);
    const { userId, messageIds } = req.body as {
      userId: number;
      messageIds: number[];
    };
    if (!Array.isArray(messageIds) || messageIds.length === 0)
      return res.json({ ok: true });

    for (const messageId of messageIds) {
      await db
        .insert(readReceipts)
        .values({ messageId, userId })
        .onConflictDoNothing();
    }

    await db
      .insert(userRoomReads)
      .values({ userId, roomId, lastReadAt: new Date() })
      .onConflictDoUpdate({
        target: [userRoomReads.userId, userRoomReads.roomId],
        set: { lastReadAt: sql`excluded.last_read_at` },
      });

    const [user] = await db.select().from(users).where(eq(users.id, userId));
    for (const messageId of messageIds) {
      io.to(`room:${roomId}`).emit('message-read', {
        messageId,
        userId,
        username: user?.username,
      });
    }

    return res.json({ ok: true });
  } catch (e: unknown) {
    return res.status(500).json({ error: String(e) });
  }
});

// Get unread count
app.get('/api/rooms/:id/unread', async (req, res) => {
  try {
    const roomId = parseInt(req.params.id);
    const userId = parseInt((req.query.userId as string) || '0');

    const [lastRead] = await db
      .select()
      .from(userRoomReads)
      .where(
        and(eq(userRoomReads.userId, userId), eq(userRoomReads.roomId, roomId)),
      );

    let count: number;
    if (!lastRead) {
      const result = await db
        .select({ count: sql<number>`count(*)::int` })
        .from(messages)
        .where(
          and(
            eq(messages.roomId, roomId),
            eq(messages.isSent, true),
            eq(messages.isDeleted, false),
          ),
        );
      count = result[0]?.count ?? 0;
    } else {
      const result = await db
        .select({ count: sql<number>`count(*)::int` })
        .from(messages)
        .where(
          and(
            eq(messages.roomId, roomId),
            eq(messages.isSent, true),
            eq(messages.isDeleted, false),
            sql`${messages.createdAt} > ${lastRead.lastReadAt}`,
          ),
        );
      count = result[0]?.count ?? 0;
    }

    return res.json({ count });
  } catch (e: unknown) {
    return res.status(500).json({ error: String(e) });
  }
});

// Kick user
app.post('/api/rooms/:id/kick', async (req, res) => {
  try {
    const roomId = parseInt(req.params.id);
    const { adminId, targetUserId } = req.body as {
      adminId: number;
      targetUserId: number;
    };

    const [admin] = await db
      .select()
      .from(roomMembers)
      .where(and(eq(roomMembers.roomId, roomId), eq(roomMembers.userId, adminId)));
    if (!admin?.isAdmin) return res.status(403).json({ error: 'Not an admin' });

    await db
      .delete(roomMembers)
      .where(and(eq(roomMembers.roomId, roomId), eq(roomMembers.userId, targetUserId)));

    io.to(`room:${roomId}`).emit('user-kicked', { roomId, userId: targetUserId });

    const targetSockets = userSockets.get(targetUserId);
    if (targetSockets) {
      for (const sid of targetSockets) {
        io.to(sid).emit('you-were-kicked', { roomId });
      }
    }

    return res.json({ ok: true });
  } catch (e: unknown) {
    return res.status(500).json({ error: String(e) });
  }
});

// Ban user
app.post('/api/rooms/:id/ban', async (req, res) => {
  try {
    const roomId = parseInt(req.params.id);
    const { adminId, targetUserId } = req.body as {
      adminId: number;
      targetUserId: number;
    };

    const [admin] = await db
      .select()
      .from(roomMembers)
      .where(and(eq(roomMembers.roomId, roomId), eq(roomMembers.userId, adminId)));
    if (!admin?.isAdmin) return res.status(403).json({ error: 'Not an admin' });

    await db
      .update(roomMembers)
      .set({ isBanned: true })
      .where(and(eq(roomMembers.roomId, roomId), eq(roomMembers.userId, targetUserId)));

    io.to(`room:${roomId}`).emit('user-banned', { roomId, userId: targetUserId });

    const targetSockets = userSockets.get(targetUserId);
    if (targetSockets) {
      for (const sid of targetSockets) {
        io.to(sid).emit('you-were-banned', { roomId });
      }
    }

    return res.json({ ok: true });
  } catch (e: unknown) {
    return res.status(500).json({ error: String(e) });
  }
});

// Promote user to admin
app.post('/api/rooms/:id/promote', async (req, res) => {
  try {
    const roomId = parseInt(req.params.id);
    const { adminId, targetUserId } = req.body as {
      adminId: number;
      targetUserId: number;
    };

    const [admin] = await db
      .select()
      .from(roomMembers)
      .where(and(eq(roomMembers.roomId, roomId), eq(roomMembers.userId, adminId)));
    if (!admin?.isAdmin) return res.status(403).json({ error: 'Not an admin' });

    await db
      .update(roomMembers)
      .set({ isAdmin: true })
      .where(and(eq(roomMembers.roomId, roomId), eq(roomMembers.userId, targetUserId)));

    io.to(`room:${roomId}`).emit('member-promoted', {
      roomId,
      userId: targetUserId,
    });
    return res.json({ ok: true });
  } catch (e: unknown) {
    return res.status(500).json({ error: String(e) });
  }
});

// ─── SOCKET.IO ──────────────────────────────────────────────────────────────

io.on('connection', (socket) => {
  console.log('Socket connected:', socket.id);

  socket.on('register', async ({ userId }: { userId: number }) => {
    socketToUser.set(socket.id, userId);
    if (!userSockets.has(userId)) userSockets.set(userId, new Set());
    userSockets.get(userId)!.add(socket.id);

    const [user] = await db
      .select()
      .from(users)
      .where(eq(users.id, userId))
      .catch(() => [null]);

    // Restore to online if was away (not if dnd/invisible)
    if (user && (user.status === 'away' || user.status === 'online')) {
      await db
        .update(users)
        .set({ status: 'online', lastActive: new Date() })
        .where(eq(users.id, userId))
        .catch(console.error);
    }

    io.emit('user-online', { userId, username: user?.username });
  });

  socket.on('join-room', ({ roomId }: { roomId: number }) => {
    socket.join(`room:${roomId}`);
  });

  socket.on('leave-room', ({ roomId }: { roomId: number }) => {
    socket.leave(`room:${roomId}`);
  });

  socket.on(
    'typing',
    ({
      roomId,
      userId,
      username,
    }: {
      roomId: number;
      userId: number;
      username: string;
    }) => {
      if (!typingState.has(roomId)) typingState.set(roomId, new Map());
      const roomTyping = typingState.get(roomId)!;

      const existing = roomTyping.get(userId);
      if (existing) clearTimeout(existing.timer);

      const timer = setTimeout(() => {
        roomTyping.delete(userId);
        broadcastTyping(roomId);
      }, 3000);

      roomTyping.set(userId, { username, timer });
      broadcastTyping(roomId);
    },
  );

  socket.on(
    'stop-typing',
    ({ roomId, userId }: { roomId: number; userId: number }) => {
      const roomTyping = typingState.get(roomId);
      if (roomTyping) {
        const existing = roomTyping.get(userId);
        if (existing) clearTimeout(existing.timer);
        roomTyping.delete(userId);
        broadcastTyping(roomId);
      }
    },
  );

  socket.on('update-activity', async ({ userId }: { userId: number }) => {
    await db
      .update(users)
      .set({ lastActive: new Date() })
      .where(eq(users.id, userId))
      .catch(console.error);
  });

  socket.on('disconnect', async () => {
    const userId = socketToUser.get(socket.id);
    socketToUser.delete(socket.id);

    if (userId !== undefined) {
      const sockets = userSockets.get(userId);
      if (sockets) {
        sockets.delete(socket.id);
        if (sockets.size === 0) {
          userSockets.delete(userId);
          const now = new Date();
          await db
            .update(users)
            .set({ lastActive: now })
            .where(eq(users.id, userId))
            .catch(console.error);

          const [user] = await db
            .select()
            .from(users)
            .where(eq(users.id, userId))
            .catch(() => [null]);

          if (user && user.status !== 'invisible') {
            io.emit('user-offline', {
              userId,
              username: user.username,
              lastActive: now,
            });
          }
        }
      }

      // Clean up typing state
      for (const [roomId, roomTyping] of typingState.entries()) {
        const existing = roomTyping.get(userId);
        if (existing) {
          clearTimeout(existing.timer);
          roomTyping.delete(userId);
          broadcastTyping(roomId);
        }
      }
    }

    console.log('Socket disconnected:', socket.id);
  });
});

httpServer.listen(PORT, () => {
  console.log(`Server running on port ${PORT}`);
});

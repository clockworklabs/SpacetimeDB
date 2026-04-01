import 'dotenv/config';
import express from 'express';
import { createServer } from 'http';
import { Server } from 'socket.io';
import { drizzle } from 'drizzle-orm/node-postgres';
import pg from 'pg';
import cors from 'cors';
import { eq, and } from 'drizzle-orm';
import { users, rooms, roomMembers, messages, lastRead } from './schema.js';

const { Pool } = pg;

const pool = new Pool({
  connectionString: process.env.DATABASE_URL || 'postgresql://spacetime:spacetime@localhost:5433/spacetime',
});

const db = drizzle(pool);

const app = express();
const httpServer = createServer(app);
const io = new Server(httpServer, {
  cors: { origin: 'http://localhost:5173', methods: ['GET', 'POST'] },
});

app.use(cors({ origin: 'http://localhost:5173', credentials: true }));
app.use(express.json());

// In-memory state
const typingTimers = new Map<number, Map<number, ReturnType<typeof setTimeout>>>(); // roomId -> userId -> timer
const socketUsers = new Map<string, number>(); // socketId -> userId
const userSockets = new Map<number, Set<string>>(); // userId -> socketIds

// Rate limiting: userId -> { count, resetAt }
const rateLimits = new Map<number, { count: number; resetAt: number }>();

function checkRateLimit(userId: number): boolean {
  const now = Date.now();
  const limit = rateLimits.get(userId);
  if (!limit || now > limit.resetAt) {
    rateLimits.set(userId, { count: 1, resetAt: now + 3000 });
    return true;
  }
  if (limit.count >= 5) return false;
  limit.count++;
  return true;
}

async function getUnreadCount(userId: number, roomId: number): Promise<number> {
  const result = await pool.query<{ count: string }>(
    `SELECT COUNT(*) as count FROM messages
     WHERE room_id = $1
     AND id > COALESCE(
       (SELECT COALESCE(last_message_id, 0) FROM last_read WHERE user_id = $2 AND room_id = $1), 0
     )`,
    [roomId, userId]
  );
  return parseInt(result.rows[0]?.count ?? '0', 10);
}

async function getMessagesWithSeenBy(roomId: number): Promise<Array<{
  id: number; roomId: number; userId: number; userName: string;
  content: string; createdAt: Date; seenBy: string[];
}>> {
  const result = await pool.query<{
    id: number; room_id: number; user_id: number; user_name: string;
    content: string; created_at: Date; seen_by: string[];
  }>(
    `SELECT
       m.id, m.room_id, m.user_id, m.content, m.created_at,
       u.name as user_name,
       COALESCE(
         array_agg(DISTINCT seen_u.name) FILTER (
           WHERE lr.user_id IS NOT NULL AND lr.user_id != m.user_id
         ),
         ARRAY[]::text[]
       ) as seen_by
     FROM messages m
     JOIN users u ON u.id = m.user_id
     LEFT JOIN last_read lr ON lr.room_id = m.room_id AND lr.last_message_id >= m.id
     LEFT JOIN users seen_u ON seen_u.id = lr.user_id
     WHERE m.room_id = $1
     GROUP BY m.id, u.name
     ORDER BY m.created_at ASC
     LIMIT 100`,
    [roomId]
  );
  return result.rows.map(r => ({
    id: r.id,
    roomId: r.room_id,
    userId: r.user_id,
    userName: r.user_name,
    content: r.content,
    createdAt: r.created_at,
    seenBy: r.seen_by || [],
  }));
}

// ---- REST API ----

app.post('/api/users/register', async (req, res) => {
  const { name } = req.body as { name?: string };
  if (!name || typeof name !== 'string' || name.trim().length === 0) {
    return res.status(400).json({ error: 'Name is required' });
  }
  const trimmed = name.trim().slice(0, 32);

  try {
    const existing = await db.select().from(users).where(eq(users.name, trimmed)).limit(1);
    if (existing[0]) return res.json(existing[0]);

    const inserted = await db.insert(users).values({ name: trimmed }).returning();
    return res.json(inserted[0]);
  } catch (err: unknown) {
    const pgErr = err as { code?: string };
    if (pgErr.code === '23505') {
      const existing = await db.select().from(users).where(eq(users.name, trimmed)).limit(1);
      if (existing[0]) return res.json(existing[0]);
    }
    console.error('Register error:', err);
    return res.status(500).json({ error: 'Failed to register' });
  }
});

app.get('/api/users', async (_req, res) => {
  const allUsers = await db.select().from(users).orderBy(users.name);
  return res.json(allUsers);
});

app.get('/api/rooms', async (req, res) => {
  const userId = parseInt(req.query.userId as string);
  const allRooms = await db.select().from(rooms).orderBy(rooms.name);

  if (!userId || isNaN(userId)) {
    return res.json(allRooms.map(r => ({ ...r, unreadCount: 0 })));
  }

  const roomsWithUnread = await Promise.all(
    allRooms.map(async (room) => ({
      ...room,
      unreadCount: await getUnreadCount(userId, room.id),
    }))
  );
  return res.json(roomsWithUnread);
});

app.post('/api/rooms', async (req, res) => {
  const { name, userId } = req.body as { name?: string; userId?: number };
  if (!name || typeof name !== 'string' || name.trim().length === 0) {
    return res.status(400).json({ error: 'Room name is required' });
  }
  const trimmed = name.trim().slice(0, 64);

  try {
    const inserted = await db.insert(rooms).values({ name: trimmed }).returning();
    const room = inserted[0];

    if (userId) {
      await db.insert(roomMembers).values({ userId, roomId: room.id }).onConflictDoNothing();
    }

    const roomWithCount = { ...room, unreadCount: 0 };
    io.emit('room_created', roomWithCount);
    return res.json(roomWithCount);
  } catch (err: unknown) {
    const pgErr = err as { code?: string };
    if (pgErr.code === '23505') {
      return res.status(409).json({ error: 'Room already exists' });
    }
    console.error('Create room error:', err);
    return res.status(500).json({ error: 'Failed to create room' });
  }
});

app.post('/api/rooms/:id/join', async (req, res) => {
  const roomId = parseInt(req.params.id);
  const { userId } = req.body as { userId?: number };
  if (!userId) return res.status(400).json({ error: 'userId required' });

  await db.insert(roomMembers).values({ userId, roomId }).onConflictDoNothing();
  return res.json({ success: true });
});

app.post('/api/rooms/:id/leave', async (req, res) => {
  const roomId = parseInt(req.params.id);
  const { userId } = req.body as { userId?: number };
  if (!userId) return res.status(400).json({ error: 'userId required' });

  await db.delete(roomMembers).where(
    and(eq(roomMembers.userId, userId), eq(roomMembers.roomId, roomId))
  );
  return res.json({ success: true });
});

app.get('/api/rooms/:id/messages', async (req, res) => {
  const roomId = parseInt(req.params.id);
  const userId = parseInt(req.query.userId as string);

  const msgs = await getMessagesWithSeenBy(roomId);

  // Auto-mark as read up to last message
  if (userId && !isNaN(userId) && msgs.length > 0) {
    const lastMsg = msgs[msgs.length - 1];
    await db.insert(lastRead)
      .values({ userId, roomId, lastMessageId: lastMsg.id, updatedAt: new Date() })
      .onConflictDoUpdate({
        target: [lastRead.userId, lastRead.roomId],
        set: { lastMessageId: lastMsg.id, updatedAt: new Date() },
      });

    // Get user name and broadcast read update
    const userRows = await db.select().from(users).where(eq(users.id, userId)).limit(1);
    const userName = userRows[0]?.name ?? 'Unknown';
    io.to(`room:${roomId}`).emit('read_update', { userId, userName, roomId, lastMessageId: lastMsg.id });
  }

  return res.json(msgs);
});

app.post('/api/rooms/:id/messages', async (req, res) => {
  const roomId = parseInt(req.params.id);
  const { userId, content } = req.body as { userId?: number; content?: string };

  if (!userId || !content || typeof content !== 'string' || content.trim().length === 0) {
    return res.status(400).json({ error: 'userId and content required' });
  }

  const trimmed = content.trim().slice(0, 2000);

  if (!checkRateLimit(userId)) {
    return res.status(429).json({ error: 'Too many messages. Please slow down.' });
  }

  const inserted = await db.insert(messages).values({ roomId, userId, content: trimmed }).returning();
  const msg = inserted[0];

  const userRows = await db.select().from(users).where(eq(users.id, userId)).limit(1);
  const userName = userRows[0]?.name ?? 'Unknown';

  // Auto-mark as read for sender
  await db.insert(lastRead)
    .values({ userId, roomId, lastMessageId: msg.id, updatedAt: new Date() })
    .onConflictDoUpdate({
      target: [lastRead.userId, lastRead.roomId],
      set: { lastMessageId: msg.id, updatedAt: new Date() },
    });

  const msgWithUser = {
    id: msg.id,
    roomId: msg.roomId,
    userId: msg.userId,
    userName,
    content: msg.content,
    createdAt: msg.createdAt,
    seenBy: [] as string[],
  };

  io.to(`room:${roomId}`).emit('message', msgWithUser);

  // Notify room members about unread count changes
  const members = await db.select().from(roomMembers).where(eq(roomMembers.roomId, roomId));
  for (const member of members) {
    if (member.userId !== userId) {
      const sockets = userSockets.get(member.userId);
      if (sockets && sockets.size > 0) {
        const unread = await getUnreadCount(member.userId, roomId);
        for (const socketId of sockets) {
          io.to(socketId).emit('unread_update', { roomId, count: unread });
        }
      }
    }
  }

  return res.json(msgWithUser);
});

app.post('/api/rooms/:id/read', async (req, res) => {
  const roomId = parseInt(req.params.id);
  const { userId, lastMessageId } = req.body as { userId?: number; lastMessageId?: number };

  if (!userId || lastMessageId === undefined) {
    return res.status(400).json({ error: 'Required fields missing' });
  }

  await db.insert(lastRead)
    .values({ userId, roomId, lastMessageId, updatedAt: new Date() })
    .onConflictDoUpdate({
      target: [lastRead.userId, lastRead.roomId],
      set: { lastMessageId, updatedAt: new Date() },
    });

  const userRows = await db.select().from(users).where(eq(users.id, userId)).limit(1);
  const userName = userRows[0]?.name ?? 'Unknown';

  io.to(`room:${roomId}`).emit('read_update', { userId, userName, roomId, lastMessageId });

  return res.json({ success: true });
});

// ---- Socket.io ----

io.on('connection', (socket) => {
  console.log('Socket connected:', socket.id);

  socket.on('identify', async (userId: number) => {
    socketUsers.set(socket.id, userId);
    if (!userSockets.has(userId)) userSockets.set(userId, new Set());
    userSockets.get(userId)!.add(socket.id);

    await db.update(users).set({ online: true }).where(eq(users.id, userId));
    io.emit('user_status', { userId, online: true });
  });

  socket.on('join_room', (roomId: number) => {
    socket.join(`room:${roomId}`);
  });

  socket.on('leave_room', (roomId: number) => {
    socket.leave(`room:${roomId}`);
  });

  socket.on('typing', (roomId: number) => {
    const userId = socketUsers.get(socket.id);
    if (!userId) return;

    if (!typingTimers.has(roomId)) typingTimers.set(roomId, new Map());
    const roomTimers = typingTimers.get(roomId)!;

    if (roomTimers.has(userId)) clearTimeout(roomTimers.get(userId)!);

    db.select().from(users).where(eq(users.id, userId)).limit(1).then(userRows => {
      const userName = userRows[0]?.name ?? 'Unknown';
      socket.to(`room:${roomId}`).emit('typing', { userId, userName, roomId });

      const timer = setTimeout(() => {
        socket.to(`room:${roomId}`).emit('typing_stop', { userId, roomId });
        roomTimers.delete(userId);
      }, 3000);
      roomTimers.set(userId, timer);
    });
  });

  socket.on('read_up_to', async ({ roomId, lastMessageId }: { roomId: number; lastMessageId: number }) => {
    const userId = socketUsers.get(socket.id);
    if (!userId) return;

    await db.insert(lastRead)
      .values({ userId, roomId, lastMessageId, updatedAt: new Date() })
      .onConflictDoUpdate({
        target: [lastRead.userId, lastRead.roomId],
        set: { lastMessageId, updatedAt: new Date() },
      });

    const userRows = await db.select().from(users).where(eq(users.id, userId)).limit(1);
    const userName = userRows[0]?.name ?? 'Unknown';
    io.to(`room:${roomId}`).emit('read_update', { userId, userName, roomId, lastMessageId });
    socket.emit('unread_update', { roomId, count: 0 });
  });

  socket.on('disconnect', async () => {
    const userId = socketUsers.get(socket.id);
    socketUsers.delete(socket.id);

    if (userId) {
      const sockets = userSockets.get(userId);
      if (sockets) {
        sockets.delete(socket.id);
        if (sockets.size === 0) {
          userSockets.delete(userId);
          await db.update(users).set({ online: false }).where(eq(users.id, userId));
          io.emit('user_status', { userId, online: false });

          for (const [roomId, roomTimers] of typingTimers) {
            if (roomTimers.has(userId)) {
              clearTimeout(roomTimers.get(userId)!);
              roomTimers.delete(userId);
              io.to(`room:${roomId}`).emit('typing_stop', { userId, roomId });
            }
          }
        }
      }
    }

    console.log('Socket disconnected:', socket.id);
  });
});

const PORT = parseInt(process.env.PORT || '3001', 10);
httpServer.listen(PORT, () => {
  console.log(`Server running on port ${PORT}`);
});

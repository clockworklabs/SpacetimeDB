import express from 'express';
import { createServer } from 'http';
import { Server } from 'socket.io';
import { drizzle } from 'drizzle-orm/node-postgres';
import { Pool } from 'pg';
import * as schema from './schema.js';
import { eq, and, inArray } from 'drizzle-orm';
import cors from 'cors';
import dotenv from 'dotenv';

dotenv.config();

const app = express();
const httpServer = createServer(app);

const io = new Server(httpServer, {
  cors: {
    origin: 'http://localhost:6273',
    methods: ['GET', 'POST'],
  },
});

app.use(cors({ origin: 'http://localhost:6273' }));
app.use(express.json());

const pool = new Pool({ connectionString: process.env.DATABASE_URL });
const db = drizzle(pool, { schema });

// In-memory typing state: roomId -> Map<userId, { timer, userName }>
const typingState = new Map<number, Map<number, { timer: NodeJS.Timeout; userName: string }>>();

// Socket to user mapping
const connectedUsers = new Map<string, { id: number; name: string }>();
const userSockets = new Map<number, string>();

// Rate limiting: userId -> last message timestamp
const lastMessageTime = new Map<number, number>();

// ─── REST API ─────────────────────────────────────────────────────────────────

// Create or get user by name
app.post('/api/users', async (req, res) => {
  const { name } = req.body as { name?: string };
  if (!name || name.trim().length === 0) {
    return res.status(400).json({ error: 'Name required' });
  }
  if (name.trim().length > 30) {
    return res.status(400).json({ error: 'Name must be 30 characters or fewer' });
  }

  try {
    let [user] = await db.select().from(schema.users).where(eq(schema.users.name, name.trim()));
    if (!user) {
      [user] = await db.insert(schema.users).values({ name: name.trim() }).returning();
    }
    res.json(user);
  } catch {
    res.status(500).json({ error: 'Failed to create user' });
  }
});

// Get online users
app.get('/api/users/online', async (_req, res) => {
  try {
    const users = await db.select().from(schema.users).where(eq(schema.users.online, true));
    res.json(users);
  } catch {
    res.status(500).json({ error: 'Failed to get online users' });
  }
});

// List rooms with unread counts
app.get('/api/rooms', async (req, res) => {
  const userId = parseInt(req.query.userId as string);
  if (!userId) return res.status(400).json({ error: 'userId required' });

  try {
    const rooms = await db.select().from(schema.rooms).orderBy(schema.rooms.name);
    const memberships = await db
      .select()
      .from(schema.roomMembers)
      .where(eq(schema.roomMembers.userId, userId));
    const joinedRooms = new Set(memberships.map((m) => m.roomId));

    const roomsWithCounts = await Promise.all(
      rooms.map(async (room) => {
        const result = await pool.query<{ count: string }>(
          `SELECT COUNT(m.id)::int as count
           FROM messages m
           LEFT JOIN read_receipts rr ON rr.message_id = m.id AND rr.user_id = $1
           WHERE m.room_id = $2 AND rr.message_id IS NULL`,
          [userId, room.id]
        );
        return {
          ...room,
          unreadCount: parseInt(result.rows[0]?.count ?? '0'),
          joined: joinedRooms.has(room.id),
        };
      })
    );

    res.json(roomsWithCounts);
  } catch (e) {
    console.error(e);
    res.status(500).json({ error: 'Failed to get rooms' });
  }
});

// Create room
app.post('/api/rooms', async (req, res) => {
  const { name, userId } = req.body as { name?: string; userId?: number };
  if (!name || name.trim().length === 0) {
    return res.status(400).json({ error: 'Room name required' });
  }
  if (name.trim().length > 50) {
    return res.status(400).json({ error: 'Room name must be 50 characters or fewer' });
  }

  try {
    const [room] = await db
      .insert(schema.rooms)
      .values({ name: name.trim() })
      .returning();

    if (userId) {
      await db
        .insert(schema.roomMembers)
        .values({ userId, roomId: room.id })
        .onConflictDoNothing();
    }

    const roomWithMeta = { ...room, unreadCount: 0, joined: userId ? true : false };
    io.emit('room_created', roomWithMeta);
    res.json(roomWithMeta);
  } catch (e: unknown) {
    if ((e as { code?: string }).code === '23505') {
      return res.status(400).json({ error: 'Room name already exists' });
    }
    res.status(500).json({ error: 'Failed to create room' });
  }
});

// Join room
app.post('/api/rooms/:id/join', async (req, res) => {
  const roomId = parseInt(req.params.id);
  const { userId } = req.body as { userId: number };

  try {
    await db
      .insert(schema.roomMembers)
      .values({ userId, roomId })
      .onConflictDoNothing();
    res.json({ ok: true });
  } catch {
    res.status(500).json({ error: 'Failed to join room' });
  }
});

// Leave room
app.post('/api/rooms/:id/leave', async (req, res) => {
  const roomId = parseInt(req.params.id);
  const { userId } = req.body as { userId: number };

  try {
    await db
      .delete(schema.roomMembers)
      .where(
        and(eq(schema.roomMembers.userId, userId), eq(schema.roomMembers.roomId, roomId))
      );
    res.json({ ok: true });
  } catch {
    res.status(500).json({ error: 'Failed to leave room' });
  }
});

// Get messages for a room (marks all as read for userId)
app.get('/api/rooms/:id/messages', async (req, res) => {
  const roomId = parseInt(req.params.id);
  const userId = parseInt(req.query.userId as string);

  try {
    const msgs = await db
      .select({
        id: schema.messages.id,
        roomId: schema.messages.roomId,
        userId: schema.messages.userId,
        content: schema.messages.content,
        createdAt: schema.messages.createdAt,
        userName: schema.users.name,
      })
      .from(schema.messages)
      .innerJoin(schema.users, eq(schema.messages.userId, schema.users.id))
      .where(eq(schema.messages.roomId, roomId))
      .orderBy(schema.messages.createdAt)
      .limit(200);

    // Get read receipts for these messages
    let receiptsByMessage: Record<number, { userId: number; userName: string }[]> = {};
    if (msgs.length > 0) {
      const receipts = await db
        .select({
          messageId: schema.readReceipts.messageId,
          userId: schema.readReceipts.userId,
          userName: schema.users.name,
        })
        .from(schema.readReceipts)
        .innerJoin(schema.users, eq(schema.readReceipts.userId, schema.users.id))
        .where(inArray(schema.readReceipts.messageId, msgs.map((m) => m.id)));

      for (const r of receipts) {
        if (!receiptsByMessage[r.messageId]) receiptsByMessage[r.messageId] = [];
        receiptsByMessage[r.messageId].push({ userId: r.userId, userName: r.userName });
      }
    }

    const result = msgs.map((m) => ({
      ...m,
      readBy: (receiptsByMessage[m.id] ?? []).filter((r) => r.userId !== m.userId),
    }));

    // Mark all messages as read for this user and broadcast
    if (userId && msgs.length > 0) {
      const [user] = await db.select().from(schema.users).where(eq(schema.users.id, userId));
      const newlyRead: number[] = [];

      for (const msg of msgs) {
        const inserted = await db
          .insert(schema.readReceipts)
          .values({ userId, messageId: msg.id })
          .onConflictDoNothing()
          .returning();
        if (inserted.length > 0) newlyRead.push(msg.id);
      }

      if (newlyRead.length > 0 && user) {
        io.to(`room:${roomId}`).emit('bulk_read', {
          messageIds: newlyRead,
          userId,
          userName: user.name,
        });
      }
    }

    res.json(result);
  } catch (e) {
    console.error(e);
    res.status(500).json({ error: 'Failed to get messages' });
  }
});

// ─── Socket.io ────────────────────────────────────────────────────────────────

io.on('connection', (socket) => {
  console.log('Client connected:', socket.id);

  socket.on('register', async ({ userId, userName }: { userId: number; userName: string }) => {
    connectedUsers.set(socket.id, { id: userId, name: userName });
    userSockets.set(userId, socket.id);

    await db
      .update(schema.users)
      .set({ online: true })
      .where(eq(schema.users.id, userId));

    io.emit('user_status', { userId, online: true, name: userName });
  });

  socket.on('join_room', ({ roomId }: { roomId: number }) => {
    socket.join(`room:${roomId}`);
  });

  socket.on('leave_room', ({ roomId }: { roomId: number }) => {
    socket.leave(`room:${roomId}`);
    const user = connectedUsers.get(socket.id);
    if (user) stopTyping(user.id, user.name, roomId);
  });

  socket.on(
    'send_message',
    async ({ roomId, content }: { roomId: number; content: string }) => {
      const user = connectedUsers.get(socket.id);
      if (!user) return;

      // Rate limit: 500ms between messages
      const now = Date.now();
      const last = lastMessageTime.get(user.id) ?? 0;
      if (now - last < 500) return;
      lastMessageTime.set(user.id, now);

      if (!content?.trim() || content.trim().length > 2000) return;

      const [message] = await db
        .insert(schema.messages)
        .values({ roomId, userId: user.id, content: content.trim() })
        .returning();

      const fullMessage = {
        ...message,
        userName: user.name,
        readBy: [] as { userId: number; userName: string }[],
      };

      io.to(`room:${roomId}`).emit('message', fullMessage);

      // Also notify members who are not actively viewing this room (for unread counts)
      const activeSocketIds = io.sockets.adapter.rooms.get(`room:${roomId}`) ?? new Set<string>();
      const members = await db.select().from(schema.roomMembers).where(eq(schema.roomMembers.roomId, roomId));
      for (const member of members) {
        if (member.userId === user.id) continue;
        const memberSocketId = userSockets.get(member.userId);
        if (memberSocketId && !activeSocketIds.has(memberSocketId)) {
          io.to(memberSocketId).emit('message', fullMessage);
        }
      }

      stopTyping(user.id, user.name, roomId);
    }
  );

  socket.on('typing_start', ({ roomId }: { roomId: number }) => {
    const user = connectedUsers.get(socket.id);
    if (!user) return;

    if (!typingState.has(roomId)) typingState.set(roomId, new Map());
    const roomTyping = typingState.get(roomId)!;

    // Reset timer
    if (roomTyping.has(user.id)) clearTimeout(roomTyping.get(user.id)!.timer);

    socket.to(`room:${roomId}`).emit('typing', { userId: user.id, userName: user.name, typing: true });

    const timer = setTimeout(() => {
      stopTyping(user.id, user.name, roomId);
    }, 3000);

    roomTyping.set(user.id, { timer, userName: user.name });
  });

  socket.on('typing_stop', ({ roomId }: { roomId: number }) => {
    const user = connectedUsers.get(socket.id);
    if (!user) return;
    stopTyping(user.id, user.name, roomId);
  });

  socket.on('mark_read', async ({ messageId }: { messageId: number }) => {
    const user = connectedUsers.get(socket.id);
    if (!user) return;

    const inserted = await db
      .insert(schema.readReceipts)
      .values({ userId: user.id, messageId })
      .onConflictDoNothing()
      .returning();

    if (inserted.length > 0) {
      const [message] = await db
        .select()
        .from(schema.messages)
        .where(eq(schema.messages.id, messageId));

      if (message) {
        io.to(`room:${message.roomId}`).emit('read_receipt', {
          messageId,
          userId: user.id,
          userName: user.name,
        });
      }
    }
  });

  socket.on('disconnect', async () => {
    const user = connectedUsers.get(socket.id);
    if (user) {
      connectedUsers.delete(socket.id);
      userSockets.delete(user.id);

      await db
        .update(schema.users)
        .set({ online: false, lastSeen: new Date() })
        .where(eq(schema.users.id, user.id));

      io.emit('user_status', { userId: user.id, online: false, name: user.name });

      // Clear all typing for this user
      typingState.forEach((roomTyping, roomId) => {
        if (roomTyping.has(user.id)) {
          clearTimeout(roomTyping.get(user.id)!.timer);
          roomTyping.delete(user.id);
          io.to(`room:${roomId}`).emit('typing', { userId: user.id, userName: user.name, typing: false });
        }
      });
    }
    console.log('Client disconnected:', socket.id);
  });
});

function stopTyping(userId: number, userName: string, roomId: number) {
  const roomTyping = typingState.get(roomId);
  if (roomTyping?.has(userId)) {
    clearTimeout(roomTyping.get(userId)!.timer);
    roomTyping.delete(userId);
  }
  io.to(`room:${roomId}`).emit('typing', { userId, userName, typing: false });
}

const PORT = parseInt(process.env.PORT ?? '6001');
httpServer.listen(PORT, () => {
  console.log(`Server running on http://localhost:${PORT}`);
});

import express from 'express';
import { createServer } from 'http';
import { Server } from 'socket.io';
import cors from 'cors';
import jwt from 'jsonwebtoken';
import dotenv from 'dotenv';
import { db } from './db/index.js';
import {
  users,
  rooms,
  messages,
  messageEdits,
  reactions,
  messageReads,
} from './db/schema.js';
import { eq, and, desc, isNull, lte, asc, inArray, sql, or } from 'drizzle-orm';
import { z } from 'zod';

dotenv.config();

const app = express();
const httpServer = createServer(app);
const io = new Server(httpServer, {
  cors: {
    origin: '*', // In production, restrict this
    methods: ['GET', 'POST'],
  },
});

const PORT = process.env.PORT || 3001;
const JWT_SECRET = process.env.JWT_SECRET || 'dev-secret-do-not-use-in-prod';

// Logger Middleware
app.use((req, res, next) => {
  console.log(`[${new Date().toISOString()}] ${req.method} ${req.url}`);
  next();
});

app.use(cors());
app.use(express.json());

// Global Error Handler & JSON Error Handler
app.use((err: any, req: any, res: any, next: any) => {
  console.error('Global Error Handler:', err);
  if (
    err instanceof SyntaxError &&
    'status' in err &&
    err.status === 400 &&
    'body' in err
  ) {
    console.error('Bad JSON:', err);
    return res.status(400).send({ error: 'Invalid JSON' });
  }
  next(err);
});

// --- Auth Middleware ---
const authenticateToken = (req: any, res: any, next: any) => {
  const authHeader = req.headers['authorization'];
  const token = authHeader && authHeader.split(' ')[1];

  if (!token) return res.sendStatus(401);

  jwt.verify(token, JWT_SECRET, (err: any, user: any) => {
    if (err) return res.sendStatus(403);
    req.user = user;
    next();
  });
};

// --- REST API ---

// Login / Register (Auto-register for simplicity)
app.post('/api/login', async (req, res) => {
  console.log(
    `[${new Date().toISOString()}] Processing login for body:`,
    JSON.stringify(req.body)
  );

  try {
    const schema = z.object({ username: z.string().min(1).max(50) });
    const validation = schema.safeParse(req.body);

    if (!validation.success) {
      console.log('Validation failed:', validation.error);
      return res.status(400).json({ error: 'Invalid username' });
    }

    const { username } = validation.data;
    console.log('Checking for existing user:', username);

    let user = await db
      .select()
      .from(users)
      .where(eq(users.username, username))
      .limit(1);
    console.log('User search result:', user);

    if (user.length === 0) {
      console.log('Creating new user:', username);
      const result = await db.insert(users).values({ username }).returning();
      console.log('Insert result:', result);
      user = result;
    }

    if (!user || user.length === 0) {
      throw new Error('Failed to retrieve or create user');
    }

    console.log('Signing token for user:', user[0].id);
    const token = jwt.sign(
      { id: user[0].id, username: user[0].username },
      JWT_SECRET,
      { expiresIn: '24h' }
    );

    console.log('Login successful, sending response');
    res.json({ token, user: user[0] });
  } catch (error) {
    console.log('LOGIN_ERROR_CAUGHT:', error); // Using log instead of error to ensure visibility
    // Check if error is an object and stringify it
    const errString =
      error instanceof Error ? error.stack : JSON.stringify(error);
    console.log('LOGIN_ERROR_DETAILS:', errString);
    res
      .status(500)
      .json({ error: 'Internal server error', details: String(error) });
  }
});

app.get('/api/users/me', authenticateToken, async (req: any, res) => {
  const user = await db
    .select()
    .from(users)
    .where(eq(users.id, req.user.id))
    .limit(1);
  if (user.length === 0) return res.sendStatus(404);
  res.json(user[0]);
});

// Rooms
app.get('/api/rooms', authenticateToken, async (req, res) => {
  const allRooms = await db.select().from(rooms).orderBy(desc(rooms.createdAt));

  // Get unread counts for each room for this user
  const userId = (req as any).user.id;

  const roomsWithCounts = await Promise.all(
    allRooms.map(async room => {
      const readStatus = await db
        .select()
        .from(messageReads)
        .where(
          and(eq(messageReads.roomId, room.id), eq(messageReads.userId, userId))
        )
        .limit(1);

      const lastReadId = readStatus[0]?.lastReadMessageId || 0;

      // Count messages > lastReadId
      const unreadCount = await db
        .select({ count: sql<number>`count(*)` })
        .from(messages)
        .where(
          and(
            eq(messages.roomId, room.id),
            sql`${messages.id} > ${lastReadId}`,
            or(
              isNull(messages.scheduledFor),
              lte(messages.scheduledFor, new Date())
            )
          )
        ); // Need to import 'or'

      return {
        ...room,
        unreadCount: Number(unreadCount[0].count),
      };
    })
  );

  res.json(roomsWithCounts);
});

app.post('/api/rooms', authenticateToken, async (req, res) => {
  const schema = z.object({ name: z.string().min(1).max(100) });
  const validation = schema.safeParse(req.body);

  if (!validation.success)
    return res.status(400).json({ error: 'Invalid name' });

  try {
    const result = await db
      .insert(rooms)
      .values({ name: validation.data.name })
      .returning();
    io.emit('room:created', result[0]); // Broadcast global event
    res.json(result[0]);
  } catch (e) {
    res.status(500).json({ error: 'Failed to create room' });
  }
});

// Messages in a room
app.get('/api/rooms/:roomId/messages', authenticateToken, async (req, res) => {
  const roomId = parseInt(req.params.roomId);
  if (isNaN(roomId)) return res.status(400).json({ error: 'Invalid room ID' });

  // Fetch messages that are NOT scheduled in future (or are owned by user)
  // For simplicity: only show sent messages to everyone.
  // User's own scheduled messages should be fetched separately or filtered.
  // The prompt says "Show pending scheduled messages to the author".

  const userId = (req as any).user.id;

  // Complex query: (roomId = ? AND (scheduledFor IS NULL OR scheduledFor <= NOW)) OR (roomId = ? AND userId = ? AND scheduledFor > NOW)
  // Actually, Drizzle queries are easier if we just fetch valid messages.

  const msgs = await db.query.messages.findMany({
    where: and(
      eq(messages.roomId, roomId),
      or(
        isNull(messages.scheduledFor),
        lte(messages.scheduledFor, new Date()),
        and(eq(messages.userId, userId)) // User sees their own scheduled messages
      )
    ),
    with: {
      author: true,
      reactions: {
        with: { user: true }, // Need user info for reactions
      },
      edits: {
        orderBy: desc(messageEdits.editedAt),
      },
    },
    orderBy: asc(messages.createdAt),
  });

  res.json(msgs);
});

// Send Message
app.post('/api/rooms/:roomId/messages', authenticateToken, async (req, res) => {
  const roomId = parseInt(req.params.roomId);
  const userId = (req as any).user.id;

  const schema = z.object({
    content: z.string().min(1),
    scheduledFor: z.string().datetime().optional(), // ISO string
    expiresInSeconds: z.number().int().positive().optional(),
  });

  const validation = schema.safeParse(req.body);
  if (!validation.success)
    return res.status(400).json({ error: 'Invalid input' });

  const { content, scheduledFor, expiresInSeconds } = validation.data;

  let expiresAt = null;
  const now = new Date();
  const scheduleDate = scheduledFor ? new Date(scheduledFor) : null;

  // Calculate expiration based on scheduled time or now
  if (expiresInSeconds) {
    const baseTime = scheduleDate || now;
    expiresAt = new Date(baseTime.getTime() + expiresInSeconds * 1000);
  }

  const result = await db
    .insert(messages)
    .values({
      roomId,
      userId,
      content,
      scheduledFor: scheduleDate,
      expiresAt: expiresAt,
    })
    .returning();

  const msg = await db.query.messages.findFirst({
    where: eq(messages.id, result[0].id),
    with: { author: true, reactions: true, edits: true },
  });

  if (!msg) return res.status(500).send();

  // If it's scheduled for future, only emit to sender (or don't emit to room yet)
  if (scheduleDate && scheduleDate > now) {
    // Only emit to sender's socket(s) if possible, or client handles "pending" state via API response.
    // We'll rely on client adding it from response.
    // No broadcast yet.
  } else {
    io.to(`room:${roomId}`).emit('message:created', msg);
    // Also update unread counts? Clients can calculate this.
  }

  res.json(msg);
});

// Edit Message
app.put('/api/messages/:messageId', authenticateToken, async (req, res) => {
  const messageId = parseInt(req.params.messageId);
  const userId = (req as any).user.id;
  const { content } = req.body;

  if (!content) return res.status(400).json({ error: 'Content required' });

  // Check ownership
  const msg = await db
    .select()
    .from(messages)
    .where(eq(messages.id, messageId))
    .limit(1);
  if (!msg.length) return res.status(404).json({ error: 'Message not found' });
  if (msg[0].userId !== userId)
    return res.status(403).json({ error: 'Not authorized' });

  // Save edit history
  await db.insert(messageEdits).values({
    messageId,
    content: msg[0].content, // Store OLD content
    editedAt: new Date(),
  });

  // Update message
  const updated = await db
    .update(messages)
    .set({ content, editedAt: new Date() })
    .where(eq(messages.id, messageId))
    .returning();

  const fullMsg = await db.query.messages.findFirst({
    where: eq(messages.id, messageId),
    with: {
      author: true,
      reactions: { with: { user: true } },
      edits: { orderBy: desc(messageEdits.editedAt) },
    },
  });

  io.to(`room:${msg[0].roomId}`).emit('message:updated', fullMsg);
  res.json(fullMsg);
});

// Add Reaction
app.post(
  '/api/messages/:messageId/reactions',
  authenticateToken,
  async (req, res) => {
    const messageId = parseInt(req.params.messageId);
    const userId = (req as any).user.id;
    const { emoji } = req.body;

    if (!emoji) return res.status(400).send();

    // Toggle reaction: check if exists
    const existing = await db
      .select()
      .from(reactions)
      .where(
        and(
          eq(reactions.messageId, messageId),
          eq(reactions.userId, userId),
          eq(reactions.emoji, emoji)
        )
      )
      .limit(1);

    let msg;
    // We need to fetch the message to get the roomId
    const msgBase = await db
      .select()
      .from(messages)
      .where(eq(messages.id, messageId))
      .limit(1);
    if (!msgBase.length) return res.status(404).send();

    if (existing.length > 0) {
      // Remove
      await db.delete(reactions).where(eq(reactions.id, existing[0].id));
    } else {
      // Add
      await db.insert(reactions).values({ messageId, userId, emoji });
    }

    // Broadcast update
    const fullMsg = await db.query.messages.findFirst({
      where: eq(messages.id, messageId),
      with: { author: true, reactions: { with: { user: true } }, edits: true },
    });

    io.to(`room:${msgBase[0].roomId}`).emit('message:updated', fullMsg);
    res.json(fullMsg);
  }
);

// Update Read Receipt
app.post('/api/rooms/:roomId/read', authenticateToken, async (req, res) => {
  const roomId = parseInt(req.params.roomId);
  const userId = (req as any).user.id;
  const { lastReadMessageId } = req.body;

  // Upsert read status
  const existing = await db
    .select()
    .from(messageReads)
    .where(
      and(eq(messageReads.roomId, roomId), eq(messageReads.userId, userId))
    )
    .limit(1);

  if (existing.length > 0) {
    await db
      .update(messageReads)
      .set({ lastReadMessageId, updatedAt: new Date() })
      .where(eq(messageReads.id, existing[0].id));
  } else {
    await db.insert(messageReads).values({ roomId, userId, lastReadMessageId });
  }

  // Notify others in room (for "Seen by..." feature)
  io.to(`room:${roomId}`).emit('room:read_updated', {
    roomId,
    userId,
    lastReadMessageId,
  });
  res.sendStatus(200);
});

// --- Socket.IO ---
io.use((socket, next) => {
  const token = socket.handshake.auth.token;
  if (!token) return next(new Error('Authentication error'));

  jwt.verify(token, JWT_SECRET, (err: any, decoded: any) => {
    if (err) return next(new Error('Authentication error'));
    socket.data.user = decoded;
    next();
  });
});

io.on('connection', socket => {
  console.log('User connected:', socket.data.user.username);

  // Join room channel
  socket.on('room:join', roomId => {
    socket.join(`room:${roomId}`);
    // Could emit "user online" here
  });

  socket.on('room:leave', roomId => {
    socket.leave(`room:${roomId}`);
  });

  socket.on('typing:start', roomId => {
    socket
      .to(`room:${roomId}`)
      .emit('typing:started', { username: socket.data.user.username, roomId });
  });

  socket.on('typing:stop', roomId => {
    socket
      .to(`room:${roomId}`)
      .emit('typing:stopped', { username: socket.data.user.username, roomId });
  });

  socket.on('disconnect', () => {
    // Handle offline status if tracking
  });
});

// --- Periodic Tasks ---
setInterval(async () => {
  const now = new Date();

  // 1. Process Scheduled Messages
  // Find messages with scheduledFor <= now.
  // We need a way to know if they were ALREADY processed.
  // Strategy: When processed, set scheduledFor to NULL?
  // But we need to know they were scheduled originally for UI?
  // Maybe just use a flag or check if we already emitted?
  // Easiest: Set scheduledFor to NULL to indicate "Sent".
  // But wait, "Show pending scheduled messages". If I set to NULL, they look like normal messages. That's good!
  // Once sent, they ARE normal messages.

  const dueMessages = await db
    .select()
    .from(messages)
    .where(lte(messages.scheduledFor, now));

  for (const msg of dueMessages) {
    // Update to null
    await db
      .update(messages)
      .set({ scheduledFor: null }) // Mark as sent
      .where(eq(messages.id, msg.id));

    // Fetch full for broadcast
    const fullMsg = await db.query.messages.findFirst({
      where: eq(messages.id, msg.id),
      with: { author: true, reactions: { with: { user: true } }, edits: true },
    });

    if (fullMsg) {
      io.to(`room:${msg.roomId}`).emit('message:created', fullMsg);
    }
  }

  // 2. Process Ephemeral Messages
  // Delete messages where expiresAt <= now
  const expiredMessages = await db
    .delete(messages)
    .where(lte(messages.expiresAt, now))
    .returning();

  for (const msg of expiredMessages) {
    io.to(`room:${msg.roomId}`).emit('message:deleted', {
      id: msg.id,
      roomId: msg.roomId,
    });
  }
}, 1000); // Check every second

httpServer.listen(PORT, () => {
  console.log(`Server running on port ${PORT}`);
});

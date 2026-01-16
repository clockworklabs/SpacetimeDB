import express from 'express';
import { createServer } from 'http';
import { Server as SocketServer, Socket } from 'socket.io';
import cors from 'cors';
import jwt from 'jsonwebtoken';
import { db } from './db.js';
import {
  users, rooms, roomMembers, messages, reactions,
  readReceipts, typingIndicators, messageEdits, roomInvitations
} from './schema.js';
import { eq, and, desc, gt, lt, isNull, or, sql, ne, inArray } from 'drizzle-orm';

const app = express();
const httpServer = createServer(app);
const io = new SocketServer(httpServer, {
  cors: { origin: '*', methods: ['GET', 'POST'] }
});

const JWT_SECRET = process.env.JWT_SECRET || 'chat-app-20260104-160000-secret';
const PORT = process.env.PORT || 3001;
const TYPING_TIMEOUT = 5000; // 5 seconds
const AWAY_TIMEOUT = 300000; // 5 minutes

app.use(cors());
app.use(express.json());

// Rate limiting map
const rateLimits = new Map<string, number>();
const RATE_LIMIT_MS = 500;

function checkRateLimit(userId: string): boolean {
  const now = Date.now();
  const lastAction = rateLimits.get(userId) || 0;
  if (now - lastAction < RATE_LIMIT_MS) return false;
  rateLimits.set(userId, now);
  return true;
}

// Auth middleware for REST
function authMiddleware(req: express.Request, res: express.Response, next: express.NextFunction) {
  const authHeader = req.headers.authorization;
  if (!authHeader?.startsWith('Bearer ')) {
    return res.status(401).json({ error: 'No token provided' });
  }
  try {
    const token = authHeader.slice(7);
    const decoded = jwt.verify(token, JWT_SECRET) as { userId: string };
    (req as any).userId = decoded.userId;
    next();
  } catch {
    return res.status(401).json({ error: 'Invalid token' });
  }
}

// Socket auth
const userSockets = new Map<string, Set<string>>(); // userId -> Set<socketId>

io.use((socket, next) => {
  const token = socket.handshake.auth.token;
  if (!token) return next(new Error('No token'));
  try {
    const decoded = jwt.verify(token, JWT_SECRET) as { userId: string };
    (socket as any).userId = decoded.userId;
    next();
  } catch {
    next(new Error('Invalid token'));
  }
});

// Helper to get user's accessible rooms
async function getUserRooms(userId: string): Promise<number[]> {
  const memberships = await db.select({ roomId: roomMembers.roomId })
    .from(roomMembers)
    .where(and(eq(roomMembers.userId, userId), eq(roomMembers.isBanned, false)));
  return memberships.map(m => m.roomId);
}

// Background jobs
async function processScheduledMessages() {
  const now = new Date();
  const scheduled = await db.select().from(messages)
    .where(and(
      eq(messages.isScheduled, true),
      lt(messages.scheduledFor, now)
    ));

  for (const msg of scheduled) {
    await db.update(messages)
      .set({ isScheduled: false, scheduledFor: null })
      .where(eq(messages.id, msg.id));

    const user = await db.select().from(users).where(eq(users.id, msg.userId)).limit(1);
    const fullMessage = { ...msg, isScheduled: false, user: user[0] };
    io.to(`room:${msg.roomId}`).emit('message:created', fullMessage);
  }
}

async function processExpiredMessages() {
  const now = new Date();
  const expired = await db.select().from(messages)
    .where(and(
      eq(messages.isEphemeral, true),
      lt(messages.expiresAt, now)
    ));

  for (const msg of expired) {
    await db.delete(messages).where(eq(messages.id, msg.id));
    io.to(`room:${msg.roomId}`).emit('message:deleted', { id: msg.id, roomId: msg.roomId });
  }
}

async function cleanupTypingIndicators() {
  const now = new Date();
  const expired = await db.select().from(typingIndicators)
    .where(lt(typingIndicators.expiresAt, now));

  for (const indicator of expired) {
    await db.delete(typingIndicators).where(eq(typingIndicators.id, indicator.id));
    io.to(`room:${indicator.roomId}`).emit('typing:stopped', {
      roomId: indicator.roomId,
      userId: indicator.userId
    });
  }
}

// Run background jobs every second
setInterval(async () => {
  try {
    await processScheduledMessages();
    await processExpiredMessages();
    await cleanupTypingIndicators();
  } catch (err) {
    console.error('Background job error:', err);
  }
}, 1000);

// REST API Routes

// Register/login
app.post('/api/auth/register', async (req, res) => {
  try {
    const { displayName } = req.body;
    if (!displayName || typeof displayName !== 'string' || displayName.length < 1 || displayName.length > 50) {
      return res.status(400).json({ error: 'Display name must be 1-50 characters' });
    }

    const [user] = await db.insert(users)
      .values({ displayName: displayName.trim() })
      .returning();

    const token = jwt.sign({ userId: user.id }, JWT_SECRET);
    res.json({ token, user });
  } catch (err) {
    console.error('Register error:', err);
    res.status(500).json({ error: 'Registration failed' });
  }
});

// Get current user
app.get('/api/users/me', authMiddleware, async (req, res) => {
  try {
    const userId = (req as any).userId;
    const [user] = await db.select().from(users).where(eq(users.id, userId));
    if (!user) return res.status(404).json({ error: 'User not found' });
    res.json(user);
  } catch (err) {
    res.status(500).json({ error: 'Failed to get user' });
  }
});

// Update user status
app.patch('/api/users/status', authMiddleware, async (req, res) => {
  try {
    const userId = (req as any).userId;
    const { status } = req.body;
    if (!['online', 'away', 'dnd', 'invisible'].includes(status)) {
      return res.status(400).json({ error: 'Invalid status' });
    }

    const [user] = await db.update(users)
      .set({ status, lastActive: new Date() })
      .where(eq(users.id, userId))
      .returning();

    io.emit('user:status', { userId, status, lastActive: user.lastActive });
    res.json(user);
  } catch (err) {
    res.status(500).json({ error: 'Failed to update status' });
  }
});

// Search users (for invitations)
app.get('/api/users/search', authMiddleware, async (req, res) => {
  try {
    const query = req.query.q as string;
    if (!query || query.length < 1) {
      return res.json([]);
    }

    const found = await db.select().from(users)
      .where(sql`LOWER(${users.displayName}) LIKE LOWER(${'%' + query + '%'})`)
      .limit(10);
    res.json(found);
  } catch (err) {
    res.status(500).json({ error: 'Search failed' });
  }
});

// Get all online users
app.get('/api/users/online', authMiddleware, async (req, res) => {
  try {
    const onlineUsers = await db.select().from(users)
      .where(ne(users.status, 'invisible'));
    res.json(onlineUsers);
  } catch (err) {
    res.status(500).json({ error: 'Failed to get online users' });
  }
});

// Get public rooms
app.get('/api/rooms', authMiddleware, async (req, res) => {
  try {
    const userId = (req as any).userId;
    const publicRooms = await db.select().from(rooms)
      .where(eq(rooms.isPrivate, false));

    // Also get private rooms user is member of
    const memberRooms = await db.select({ room: rooms })
      .from(roomMembers)
      .innerJoin(rooms, eq(rooms.id, roomMembers.roomId))
      .where(and(
        eq(roomMembers.userId, userId),
        eq(roomMembers.isBanned, false),
        eq(rooms.isPrivate, true)
      ));

    const allRooms = [...publicRooms, ...memberRooms.map(r => r.room)];
    const uniqueRooms = allRooms.filter((r, i, arr) => arr.findIndex(x => x.id === r.id) === i);
    res.json(uniqueRooms);
  } catch (err) {
    res.status(500).json({ error: 'Failed to get rooms' });
  }
});

// Create room
app.post('/api/rooms', authMiddleware, async (req, res) => {
  try {
    const userId = (req as any).userId;
    const { name, isPrivate } = req.body;

    if (!name || typeof name !== 'string' || name.length < 1 || name.length > 100) {
      return res.status(400).json({ error: 'Room name must be 1-100 characters' });
    }

    const [room] = await db.insert(rooms)
      .values({ name: name.trim(), isPrivate: !!isPrivate, createdBy: userId })
      .returning();

    // Creator automatically joins as admin
    await db.insert(roomMembers)
      .values({ roomId: room.id, userId, role: 'admin' });

    if (!isPrivate) {
      io.emit('room:created', room);
    }
    res.json(room);
  } catch (err) {
    console.error('Create room error:', err);
    res.status(500).json({ error: 'Failed to create room' });
  }
});

// Create DM
app.post('/api/rooms/dm', authMiddleware, async (req, res) => {
  try {
    const userId = (req as any).userId;
    const { targetUserId } = req.body;

    if (!targetUserId) {
      return res.status(400).json({ error: 'Target user required' });
    }

    // Check if DM already exists
    const existingDms = await db.select({ roomId: roomMembers.roomId })
      .from(roomMembers)
      .innerJoin(rooms, eq(rooms.id, roomMembers.roomId))
      .where(and(
        eq(roomMembers.userId, userId),
        eq(rooms.isDm, true)
      ));

    for (const dm of existingDms) {
      const otherMember = await db.select()
        .from(roomMembers)
        .where(and(
          eq(roomMembers.roomId, dm.roomId),
          eq(roomMembers.userId, targetUserId)
        ));
      if (otherMember.length > 0) {
        const [room] = await db.select().from(rooms).where(eq(rooms.id, dm.roomId));
        return res.json(room);
      }
    }

    // Get target user for room name
    const [targetUser] = await db.select().from(users).where(eq(users.id, targetUserId));
    const [currentUser] = await db.select().from(users).where(eq(users.id, userId));
    if (!targetUser) return res.status(404).json({ error: 'User not found' });

    const [room] = await db.insert(rooms)
      .values({
        name: `${currentUser.displayName} & ${targetUser.displayName}`,
        isPrivate: true,
        isDm: true,
        createdBy: userId
      })
      .returning();

    // Both users join automatically
    await db.insert(roomMembers).values([
      { roomId: room.id, userId, role: 'admin' },
      { roomId: room.id, userId: targetUserId, role: 'admin' }
    ]);

    // Notify both users
    const sockets1 = userSockets.get(userId);
    const sockets2 = userSockets.get(targetUserId);
    if (sockets1) sockets1.forEach(s => io.to(s).emit('room:created', room));
    if (sockets2) sockets2.forEach(s => io.to(s).emit('room:created', room));

    res.json(room);
  } catch (err) {
    console.error('Create DM error:', err);
    res.status(500).json({ error: 'Failed to create DM' });
  }
});

// Join room
app.post('/api/rooms/:roomId/join', authMiddleware, async (req, res) => {
  try {
    const userId = (req as any).userId;
    const roomId = parseInt(req.params.roomId);

    const [room] = await db.select().from(rooms).where(eq(rooms.id, roomId));
    if (!room) return res.status(404).json({ error: 'Room not found' });

    if (room.isPrivate) {
      return res.status(403).json({ error: 'Cannot join private room directly' });
    }

    // Check if banned
    const [existing] = await db.select().from(roomMembers)
      .where(and(eq(roomMembers.roomId, roomId), eq(roomMembers.userId, userId)));

    if (existing?.isBanned) {
      return res.status(403).json({ error: 'You are banned from this room' });
    }

    if (!existing) {
      await db.insert(roomMembers)
        .values({ roomId, userId })
        .onConflictDoNothing();
    }

    io.to(`room:${roomId}`).emit('member:joined', { roomId, userId });
    res.json({ success: true });
  } catch (err) {
    res.status(500).json({ error: 'Failed to join room' });
  }
});

// Leave room
app.post('/api/rooms/:roomId/leave', authMiddleware, async (req, res) => {
  try {
    const userId = (req as any).userId;
    const roomId = parseInt(req.params.roomId);

    await db.delete(roomMembers)
      .where(and(eq(roomMembers.roomId, roomId), eq(roomMembers.userId, userId)));

    io.to(`room:${roomId}`).emit('member:left', { roomId, userId });
    res.json({ success: true });
  } catch (err) {
    res.status(500).json({ error: 'Failed to leave room' });
  }
});

// Invite user to private room
app.post('/api/rooms/:roomId/invite', authMiddleware, async (req, res) => {
  try {
    const userId = (req as any).userId;
    const roomId = parseInt(req.params.roomId);
    const { targetUserId } = req.body;

    // Check if user is admin
    const [membership] = await db.select().from(roomMembers)
      .where(and(eq(roomMembers.roomId, roomId), eq(roomMembers.userId, userId)));

    if (!membership || membership.role !== 'admin') {
      return res.status(403).json({ error: 'Only admins can invite' });
    }

    const [room] = await db.select().from(rooms).where(eq(rooms.id, roomId));
    if (!room) return res.status(404).json({ error: 'Room not found' });

    // Create invitation
    const [invitation] = await db.insert(roomInvitations)
      .values({ roomId, invitedUserId: targetUserId, invitedBy: userId })
      .onConflictDoUpdate({
        target: [roomInvitations.roomId, roomInvitations.invitedUserId],
        set: { status: 'pending', invitedBy: userId, createdAt: new Date() }
      })
      .returning();

    // Notify invited user
    const targetSockets = userSockets.get(targetUserId);
    if (targetSockets) {
      const [inviter] = await db.select().from(users).where(eq(users.id, userId));
      targetSockets.forEach(s => io.to(s).emit('invitation:received', {
        invitation,
        room,
        inviter
      }));
    }

    res.json(invitation);
  } catch (err) {
    console.error('Invite error:', err);
    res.status(500).json({ error: 'Failed to invite user' });
  }
});

// Get my invitations
app.get('/api/invitations', authMiddleware, async (req, res) => {
  try {
    const userId = (req as any).userId;
    const invitations = await db.select({
      invitation: roomInvitations,
      room: rooms,
    })
      .from(roomInvitations)
      .innerJoin(rooms, eq(rooms.id, roomInvitations.roomId))
      .where(and(
        eq(roomInvitations.invitedUserId, userId),
        eq(roomInvitations.status, 'pending')
      ));

    res.json(invitations);
  } catch (err) {
    res.status(500).json({ error: 'Failed to get invitations' });
  }
});

// Accept/decline invitation
app.post('/api/invitations/:id/respond', authMiddleware, async (req, res) => {
  try {
    const userId = (req as any).userId;
    const invitationId = parseInt(req.params.id);
    const { accept } = req.body;

    const [invitation] = await db.select().from(roomInvitations)
      .where(and(
        eq(roomInvitations.id, invitationId),
        eq(roomInvitations.invitedUserId, userId)
      ));

    if (!invitation) return res.status(404).json({ error: 'Invitation not found' });

    if (accept) {
      await db.update(roomInvitations)
        .set({ status: 'accepted' })
        .where(eq(roomInvitations.id, invitationId));

      await db.insert(roomMembers)
        .values({ roomId: invitation.roomId, userId })
        .onConflictDoNothing();

      const [room] = await db.select().from(rooms).where(eq(rooms.id, invitation.roomId));
      io.to(`room:${invitation.roomId}`).emit('member:joined', { roomId: invitation.roomId, userId });

      // Notify user of new room
      const sockets = userSockets.get(userId);
      if (sockets) sockets.forEach(s => io.to(s).emit('room:created', room));
    } else {
      await db.update(roomInvitations)
        .set({ status: 'declined' })
        .where(eq(roomInvitations.id, invitationId));
    }

    res.json({ success: true });
  } catch (err) {
    res.status(500).json({ error: 'Failed to respond to invitation' });
  }
});

// Get room members
app.get('/api/rooms/:roomId/members', authMiddleware, async (req, res) => {
  try {
    const userId = (req as any).userId;
    const roomId = parseInt(req.params.roomId);

    // Check if user is member
    const [membership] = await db.select().from(roomMembers)
      .where(and(eq(roomMembers.roomId, roomId), eq(roomMembers.userId, userId)));

    const [room] = await db.select().from(rooms).where(eq(rooms.id, roomId));
    if (room?.isPrivate && !membership) {
      return res.status(403).json({ error: 'Not a member' });
    }

    const members = await db.select({ member: roomMembers, user: users })
      .from(roomMembers)
      .innerJoin(users, eq(users.id, roomMembers.userId))
      .where(and(eq(roomMembers.roomId, roomId), eq(roomMembers.isBanned, false)));

    res.json(members);
  } catch (err) {
    res.status(500).json({ error: 'Failed to get members' });
  }
});

// Kick user from room
app.post('/api/rooms/:roomId/kick', authMiddleware, async (req, res) => {
  try {
    const userId = (req as any).userId;
    const roomId = parseInt(req.params.roomId);
    const { targetUserId } = req.body;

    const [membership] = await db.select().from(roomMembers)
      .where(and(eq(roomMembers.roomId, roomId), eq(roomMembers.userId, userId)));

    if (!membership || membership.role !== 'admin') {
      return res.status(403).json({ error: 'Only admins can kick' });
    }

    await db.delete(roomMembers)
      .where(and(eq(roomMembers.roomId, roomId), eq(roomMembers.userId, targetUserId)));

    io.to(`room:${roomId}`).emit('member:kicked', { roomId, userId: targetUserId });

    // Notify kicked user
    const targetSockets = userSockets.get(targetUserId);
    if (targetSockets) {
      targetSockets.forEach(s => io.to(s).emit('room:kicked', { roomId }));
    }

    res.json({ success: true });
  } catch (err) {
    res.status(500).json({ error: 'Failed to kick user' });
  }
});

// Ban user from room
app.post('/api/rooms/:roomId/ban', authMiddleware, async (req, res) => {
  try {
    const userId = (req as any).userId;
    const roomId = parseInt(req.params.roomId);
    const { targetUserId } = req.body;

    const [membership] = await db.select().from(roomMembers)
      .where(and(eq(roomMembers.roomId, roomId), eq(roomMembers.userId, userId)));

    if (!membership || membership.role !== 'admin') {
      return res.status(403).json({ error: 'Only admins can ban' });
    }

    await db.update(roomMembers)
      .set({ isBanned: true })
      .where(and(eq(roomMembers.roomId, roomId), eq(roomMembers.userId, targetUserId)));

    io.to(`room:${roomId}`).emit('member:banned', { roomId, userId: targetUserId });

    const targetSockets = userSockets.get(targetUserId);
    if (targetSockets) {
      targetSockets.forEach(s => io.to(s).emit('room:banned', { roomId }));
    }

    res.json({ success: true });
  } catch (err) {
    res.status(500).json({ error: 'Failed to ban user' });
  }
});

// Promote user to admin
app.post('/api/rooms/:roomId/promote', authMiddleware, async (req, res) => {
  try {
    const userId = (req as any).userId;
    const roomId = parseInt(req.params.roomId);
    const { targetUserId } = req.body;

    const [membership] = await db.select().from(roomMembers)
      .where(and(eq(roomMembers.roomId, roomId), eq(roomMembers.userId, userId)));

    if (!membership || membership.role !== 'admin') {
      return res.status(403).json({ error: 'Only admins can promote' });
    }

    await db.update(roomMembers)
      .set({ role: 'admin' })
      .where(and(eq(roomMembers.roomId, roomId), eq(roomMembers.userId, targetUserId)));

    io.to(`room:${roomId}`).emit('member:promoted', { roomId, userId: targetUserId });
    res.json({ success: true });
  } catch (err) {
    res.status(500).json({ error: 'Failed to promote user' });
  }
});

// Get messages for a room
app.get('/api/rooms/:roomId/messages', authMiddleware, async (req, res) => {
  try {
    const userId = (req as any).userId;
    const roomId = parseInt(req.params.roomId);

    // Check membership for private rooms
    const [room] = await db.select().from(rooms).where(eq(rooms.id, roomId));
    if (room?.isPrivate) {
      const [membership] = await db.select().from(roomMembers)
        .where(and(eq(roomMembers.roomId, roomId), eq(roomMembers.userId, userId), eq(roomMembers.isBanned, false)));
      if (!membership) return res.status(403).json({ error: 'Not a member' });
    }

    const roomMessages = await db.select({ message: messages, user: users })
      .from(messages)
      .innerJoin(users, eq(users.id, messages.userId))
      .where(and(
        eq(messages.roomId, roomId),
        eq(messages.isScheduled, false)
      ))
      .orderBy(messages.createdAt);

    // Get reactions for messages
    const messageIds = roomMessages.map(m => m.message.id);
    const allReactions = messageIds.length > 0
      ? await db.select({ reaction: reactions, user: users })
          .from(reactions)
          .innerJoin(users, eq(users.id, reactions.userId))
          .where(inArray(reactions.messageId, messageIds))
      : [];

    // Get reply counts
    const replyCounts = messageIds.length > 0
      ? await db.select({
          parentId: messages.parentMessageId,
          count: sql<number>`count(*)::int`
        })
          .from(messages)
          .where(and(
            inArray(messages.parentMessageId, messageIds),
            eq(messages.isScheduled, false)
          ))
          .groupBy(messages.parentMessageId)
      : [];

    const result = roomMessages.map(m => ({
      ...m.message,
      user: m.user,
      reactions: allReactions.filter(r => r.reaction.messageId === m.message.id)
        .map(r => ({ ...r.reaction, user: r.user })),
      replyCount: replyCounts.find(rc => rc.parentId === m.message.id)?.count || 0
    }));

    res.json(result);
  } catch (err) {
    console.error('Get messages error:', err);
    res.status(500).json({ error: 'Failed to get messages' });
  }
});

// Get scheduled messages for current user
app.get('/api/rooms/:roomId/scheduled', authMiddleware, async (req, res) => {
  try {
    const userId = (req as any).userId;
    const roomId = parseInt(req.params.roomId);

    const scheduled = await db.select().from(messages)
      .where(and(
        eq(messages.roomId, roomId),
        eq(messages.userId, userId),
        eq(messages.isScheduled, true)
      ))
      .orderBy(messages.scheduledFor);

    res.json(scheduled);
  } catch (err) {
    res.status(500).json({ error: 'Failed to get scheduled messages' });
  }
});

// Send message
app.post('/api/rooms/:roomId/messages', authMiddleware, async (req, res) => {
  try {
    const userId = (req as any).userId;
    const roomId = parseInt(req.params.roomId);
    const { content, parentMessageId, scheduledFor, ephemeralMinutes } = req.body;

    if (!checkRateLimit(userId)) {
      return res.status(429).json({ error: 'Rate limited' });
    }

    if (!content || typeof content !== 'string' || content.length < 1 || content.length > 2000) {
      return res.status(400).json({ error: 'Content must be 1-2000 characters' });
    }

    // Check membership
    const [membership] = await db.select().from(roomMembers)
      .where(and(eq(roomMembers.roomId, roomId), eq(roomMembers.userId, userId), eq(roomMembers.isBanned, false)));

    if (!membership) {
      return res.status(403).json({ error: 'Must be a room member to send messages' });
    }

    const isScheduled = !!scheduledFor;
    const isEphemeral = !!ephemeralMinutes;

    const [message] = await db.insert(messages)
      .values({
        roomId,
        userId,
        content: content.trim(),
        parentMessageId: parentMessageId || null,
        isScheduled,
        scheduledFor: scheduledFor ? new Date(scheduledFor) : null,
        isEphemeral,
        expiresAt: isEphemeral ? new Date(Date.now() + ephemeralMinutes * 60 * 1000) : null,
      })
      .returning();

    const [user] = await db.select().from(users).where(eq(users.id, userId));
    const fullMessage = { ...message, user, reactions: [], replyCount: 0 };

    if (!isScheduled) {
      io.to(`room:${roomId}`).emit('message:created', fullMessage);

      // Update parent reply count if this is a reply
      if (parentMessageId) {
        io.to(`room:${roomId}`).emit('message:replyAdded', { messageId: parentMessageId });
      }
    }

    res.json(fullMessage);
  } catch (err) {
    console.error('Send message error:', err);
    res.status(500).json({ error: 'Failed to send message' });
  }
});

// Cancel scheduled message
app.delete('/api/messages/:messageId/scheduled', authMiddleware, async (req, res) => {
  try {
    const userId = (req as any).userId;
    const messageId = parseInt(req.params.messageId);

    const [message] = await db.select().from(messages).where(eq(messages.id, messageId));
    if (!message || message.userId !== userId) {
      return res.status(403).json({ error: 'Not authorized' });
    }

    await db.delete(messages).where(eq(messages.id, messageId));
    res.json({ success: true });
  } catch (err) {
    res.status(500).json({ error: 'Failed to cancel scheduled message' });
  }
});

// Edit message
app.patch('/api/messages/:messageId', authMiddleware, async (req, res) => {
  try {
    const userId = (req as any).userId;
    const messageId = parseInt(req.params.messageId);
    const { content } = req.body;

    if (!content || typeof content !== 'string' || content.length < 1 || content.length > 2000) {
      return res.status(400).json({ error: 'Content must be 1-2000 characters' });
    }

    const [message] = await db.select().from(messages).where(eq(messages.id, messageId));
    if (!message || message.userId !== userId) {
      return res.status(403).json({ error: 'Not authorized' });
    }

    // Save edit history
    await db.insert(messageEdits)
      .values({ messageId, previousContent: message.content });

    const [updated] = await db.update(messages)
      .set({ content: content.trim(), isEdited: true })
      .where(eq(messages.id, messageId))
      .returning();

    io.to(`room:${message.roomId}`).emit('message:updated', updated);
    res.json(updated);
  } catch (err) {
    res.status(500).json({ error: 'Failed to edit message' });
  }
});

// Get edit history
app.get('/api/messages/:messageId/history', authMiddleware, async (req, res) => {
  try {
    const messageId = parseInt(req.params.messageId);
    const history = await db.select().from(messageEdits)
      .where(eq(messageEdits.messageId, messageId))
      .orderBy(desc(messageEdits.editedAt));
    res.json(history);
  } catch (err) {
    res.status(500).json({ error: 'Failed to get edit history' });
  }
});

// Get thread replies
app.get('/api/messages/:messageId/replies', authMiddleware, async (req, res) => {
  try {
    const messageId = parseInt(req.params.messageId);

    const replies = await db.select({ message: messages, user: users })
      .from(messages)
      .innerJoin(users, eq(users.id, messages.userId))
      .where(eq(messages.parentMessageId, messageId))
      .orderBy(messages.createdAt);

    const result = replies.map(r => ({ ...r.message, user: r.user }));
    res.json(result);
  } catch (err) {
    res.status(500).json({ error: 'Failed to get replies' });
  }
});

// Toggle reaction
app.post('/api/messages/:messageId/reactions', authMiddleware, async (req, res) => {
  try {
    const userId = (req as any).userId;
    const messageId = parseInt(req.params.messageId);
    const { emoji } = req.body;

    if (!emoji || typeof emoji !== 'string' || emoji.length > 10) {
      return res.status(400).json({ error: 'Invalid emoji' });
    }

    const [message] = await db.select().from(messages).where(eq(messages.id, messageId));
    if (!message) return res.status(404).json({ error: 'Message not found' });

    // Check if reaction exists
    const [existing] = await db.select().from(reactions)
      .where(and(
        eq(reactions.messageId, messageId),
        eq(reactions.userId, userId),
        eq(reactions.emoji, emoji)
      ));

    if (existing) {
      await db.delete(reactions).where(eq(reactions.id, existing.id));
      io.to(`room:${message.roomId}`).emit('reaction:removed', {
        messageId, userId, emoji
      });
    } else {
      const [reaction] = await db.insert(reactions)
        .values({ messageId, userId, emoji })
        .returning();
      const [user] = await db.select().from(users).where(eq(users.id, userId));
      io.to(`room:${message.roomId}`).emit('reaction:added', {
        ...reaction, user
      });
    }

    res.json({ success: true });
  } catch (err) {
    res.status(500).json({ error: 'Failed to toggle reaction' });
  }
});

// Mark messages as read
app.post('/api/rooms/:roomId/read', authMiddleware, async (req, res) => {
  try {
    const userId = (req as any).userId;
    const roomId = parseInt(req.params.roomId);
    const { messageIds } = req.body;

    if (!Array.isArray(messageIds)) {
      return res.status(400).json({ error: 'messageIds must be an array' });
    }

    // Update last read timestamp for room member
    await db.update(roomMembers)
      .set({ lastReadAt: new Date() })
      .where(and(eq(roomMembers.roomId, roomId), eq(roomMembers.userId, userId)));

    // Insert read receipts
    for (const messageId of messageIds) {
      await db.insert(readReceipts)
        .values({ messageId, userId })
        .onConflictDoNothing();
    }

    const [user] = await db.select().from(users).where(eq(users.id, userId));
    io.to(`room:${roomId}`).emit('messages:read', { userId, messageIds, user });
    res.json({ success: true });
  } catch (err) {
    res.status(500).json({ error: 'Failed to mark as read' });
  }
});

// Get read receipts for messages
app.get('/api/messages/:messageId/receipts', authMiddleware, async (req, res) => {
  try {
    const messageId = parseInt(req.params.messageId);
    const receipts = await db.select({ receipt: readReceipts, user: users })
      .from(readReceipts)
      .innerJoin(users, eq(users.id, readReceipts.userId))
      .where(eq(readReceipts.messageId, messageId));

    res.json(receipts.map(r => ({ ...r.receipt, user: r.user })));
  } catch (err) {
    res.status(500).json({ error: 'Failed to get receipts' });
  }
});

// Get unread counts for all rooms
app.get('/api/unread', authMiddleware, async (req, res) => {
  try {
    const userId = (req as any).userId;

    const memberships = await db.select().from(roomMembers)
      .where(and(eq(roomMembers.userId, userId), eq(roomMembers.isBanned, false)));

    const counts: Record<number, number> = {};
    for (const membership of memberships) {
      const lastRead = membership.lastReadAt || new Date(0);
      const [result] = await db.select({ count: sql<number>`count(*)::int` })
        .from(messages)
        .where(and(
          eq(messages.roomId, membership.roomId),
          gt(messages.createdAt, lastRead),
          eq(messages.isScheduled, false),
          ne(messages.userId, userId)
        ));
      counts[membership.roomId] = result?.count || 0;
    }

    res.json(counts);
  } catch (err) {
    res.status(500).json({ error: 'Failed to get unread counts' });
  }
});

// Socket.io connection handling
io.on('connection', async (socket: Socket) => {
  const userId = (socket as any).userId;
  console.log(`User ${userId} connected`);

  // Track socket
  if (!userSockets.has(userId)) {
    userSockets.set(userId, new Set());
  }
  userSockets.get(userId)!.add(socket.id);

  // Set user online
  await db.update(users)
    .set({ status: 'online', lastActive: new Date() })
    .where(eq(users.id, userId));

  const [user] = await db.select().from(users).where(eq(users.id, userId));
  io.emit('user:online', user);

  // Join user's rooms
  const roomIds = await getUserRooms(userId);
  roomIds.forEach(roomId => socket.join(`room:${roomId}`));

  // Handle typing
  socket.on('typing:start', async (roomId: number) => {
    const expiresAt = new Date(Date.now() + TYPING_TIMEOUT);
    await db.insert(typingIndicators)
      .values({ roomId, userId, expiresAt })
      .onConflictDoUpdate({
        target: [typingIndicators.roomId, typingIndicators.userId],
        set: { expiresAt }
      });
    socket.to(`room:${roomId}`).emit('typing:started', { roomId, userId });
  });

  socket.on('typing:stop', async (roomId: number) => {
    await db.delete(typingIndicators)
      .where(and(eq(typingIndicators.roomId, roomId), eq(typingIndicators.userId, userId)));
    socket.to(`room:${roomId}`).emit('typing:stopped', { roomId, userId });
  });

  // Join a room's socket room
  socket.on('room:join', (roomId: number) => {
    socket.join(`room:${roomId}`);
  });

  socket.on('room:leave', (roomId: number) => {
    socket.leave(`room:${roomId}`);
  });

  // Heartbeat for activity tracking
  socket.on('heartbeat', async () => {
    await db.update(users)
      .set({ lastActive: new Date() })
      .where(eq(users.id, userId));
  });

  socket.on('disconnect', async () => {
    console.log(`User ${userId} disconnected`);
    const sockets = userSockets.get(userId);
    if (sockets) {
      sockets.delete(socket.id);
      if (sockets.size === 0) {
        userSockets.delete(userId);
        // Set user offline
        await db.update(users)
          .set({ lastActive: new Date() })
          .where(eq(users.id, userId));
        io.emit('user:offline', { userId });
      }
    }
  });
});

// Auto-away detection (runs every minute)
setInterval(async () => {
  const threshold = new Date(Date.now() - AWAY_TIMEOUT);
  const inactive = await db.select().from(users)
    .where(and(
      eq(users.status, 'online'),
      lt(users.lastActive, threshold)
    ));

  for (const user of inactive) {
    if (userSockets.has(user.id)) {
      await db.update(users)
        .set({ status: 'away' })
        .where(eq(users.id, user.id));
      io.emit('user:status', { userId: user.id, status: 'away', lastActive: user.lastActive });
    }
  }
}, 60000);

httpServer.listen(PORT, () => {
  console.log(`Server running on port ${PORT}`);
});

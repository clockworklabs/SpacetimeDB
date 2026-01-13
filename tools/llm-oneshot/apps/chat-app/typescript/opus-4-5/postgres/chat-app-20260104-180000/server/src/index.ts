import express from 'express';
import cors from 'cors';
import { createServer } from 'http';
import { Server } from 'socket.io';
import jwt from 'jsonwebtoken';
import { db } from './db.js';
import {
  users, rooms, roomMembers, messages, messageEdits, messageReactions,
  readReceipts, typingIndicators, roomInvites
} from './schema.js';
import { eq, and, or, desc, lt, lte, gt, gte, sql, ne, isNull, inArray, count } from 'drizzle-orm';

const app = express();
const httpServer = createServer(app);

const JWT_SECRET = process.env.JWT_SECRET || 'chat-app-20260104-180000-secret';
const PORT = parseInt(process.env.PORT || '3001');
const TYPING_TIMEOUT_MS = 5000;
const INACTIVITY_TIMEOUT_MS = 5 * 60 * 1000; // 5 minutes

app.use(cors());
app.use(express.json());

const io = new Server(httpServer, {
  cors: {
    origin: '*',
    methods: ['GET', 'POST'],
  },
});

// JWT middleware for REST
const authenticateToken = (req: express.Request, res: express.Response, next: express.NextFunction) => {
  const authHeader = req.headers['authorization'];
  const token = authHeader?.split(' ')[1];
  if (!token) return res.status(401).json({ error: 'Unauthorized' });

  try {
    const decoded = jwt.verify(token, JWT_SECRET) as { userId: string };
    (req as any).userId = decoded.userId;
    next();
  } catch {
    return res.status(403).json({ error: 'Invalid token' });
  }
};

// ============ REST ENDPOINTS ============

// Register user
app.post('/api/auth/register', async (req, res) => {
  try {
    const { displayName } = req.body;
    if (!displayName || typeof displayName !== 'string' || displayName.trim().length < 1 || displayName.length > 50) {
      return res.status(400).json({ error: 'Display name must be 1-50 characters' });
    }

    const [user] = await db.insert(users).values({
      displayName: displayName.trim(),
    }).returning();

    const token = jwt.sign({ userId: user.id }, JWT_SECRET);
    res.json({ user, token });
  } catch (error) {
    console.error('Register error:', error);
    res.status(500).json({ error: 'Internal server error' });
  }
});

// Get current user
app.get('/api/auth/me', authenticateToken, async (req, res) => {
  try {
    const userId = (req as any).userId;
    const [user] = await db.select().from(users).where(eq(users.id, userId));
    if (!user) return res.status(404).json({ error: 'User not found' });
    res.json(user);
  } catch (error) {
    console.error('Get user error:', error);
    res.status(500).json({ error: 'Internal server error' });
  }
});

// Update display name
app.put('/api/auth/displayName', authenticateToken, async (req, res) => {
  try {
    const userId = (req as any).userId;
    const { displayName } = req.body;
    if (!displayName || typeof displayName !== 'string' || displayName.trim().length < 1 || displayName.length > 50) {
      return res.status(400).json({ error: 'Display name must be 1-50 characters' });
    }

    const [user] = await db.update(users)
      .set({ displayName: displayName.trim() })
      .where(eq(users.id, userId))
      .returning();

    io.emit('user:updated', user);
    res.json(user);
  } catch (error) {
    console.error('Update displayName error:', error);
    res.status(500).json({ error: 'Internal server error' });
  }
});

// Update user status
app.put('/api/auth/status', authenticateToken, async (req, res) => {
  try {
    const userId = (req as any).userId;
    const { status } = req.body;
    if (!['online', 'away', 'dnd', 'invisible'].includes(status)) {
      return res.status(400).json({ error: 'Invalid status' });
    }

    const [user] = await db.update(users)
      .set({ status, lastActiveAt: new Date() })
      .where(eq(users.id, userId))
      .returning();

    io.emit('user:updated', user);
    res.json(user);
  } catch (error) {
    console.error('Update status error:', error);
    res.status(500).json({ error: 'Internal server error' });
  }
});

// Get all users
app.get('/api/users', authenticateToken, async (req, res) => {
  try {
    const allUsers = await db.select().from(users);
    res.json(allUsers);
  } catch (error) {
    console.error('Get users error:', error);
    res.status(500).json({ error: 'Internal server error' });
  }
});

// Get user by username for DMs
app.get('/api/users/by-name/:displayName', authenticateToken, async (req, res) => {
  try {
    const { displayName } = req.params;
    const [user] = await db.select().from(users).where(eq(users.displayName, displayName));
    if (!user) return res.status(404).json({ error: 'User not found' });
    res.json(user);
  } catch (error) {
    console.error('Get user by name error:', error);
    res.status(500).json({ error: 'Internal server error' });
  }
});

// Get public rooms
app.get('/api/rooms', authenticateToken, async (req, res) => {
  try {
    const publicRooms = await db.select().from(rooms).where(eq(rooms.roomType, 'public'));
    res.json(publicRooms);
  } catch (error) {
    console.error('Get rooms error:', error);
    res.status(500).json({ error: 'Internal server error' });
  }
});

// Get user's rooms (joined rooms including private ones)
app.get('/api/rooms/my', authenticateToken, async (req, res) => {
  try {
    const userId = (req as any).userId;
    const memberRooms = await db.select({
      room: rooms,
      member: roomMembers,
    })
      .from(roomMembers)
      .innerJoin(rooms, eq(rooms.id, roomMembers.roomId))
      .where(and(
        eq(roomMembers.userId, userId),
        eq(roomMembers.isBanned, false)
      ));

    res.json(memberRooms.map(r => ({ ...r.room, role: r.member.role, lastReadAt: r.member.lastReadAt })));
  } catch (error) {
    console.error('Get my rooms error:', error);
    res.status(500).json({ error: 'Internal server error' });
  }
});

// Create room
app.post('/api/rooms', authenticateToken, async (req, res) => {
  try {
    const userId = (req as any).userId;
    const { name, roomType } = req.body;

    if (!name || typeof name !== 'string' || name.trim().length < 1 || name.length > 100) {
      return res.status(400).json({ error: 'Room name must be 1-100 characters' });
    }
    if (roomType && !['public', 'private'].includes(roomType)) {
      return res.status(400).json({ error: 'Invalid room type' });
    }

    const [room] = await db.insert(rooms).values({
      name: name.trim(),
      createdBy: userId,
      roomType: roomType || 'public',
    }).returning();

    // Auto-join creator as admin
    await db.insert(roomMembers).values({
      roomId: room.id,
      userId,
      role: 'admin',
    });

    if (room.roomType === 'public') {
      io.emit('room:created', room);
    }
    res.json(room);
  } catch (error) {
    console.error('Create room error:', error);
    res.status(500).json({ error: 'Internal server error' });
  }
});

// Join room (public only)
app.post('/api/rooms/:roomId/join', authenticateToken, async (req, res) => {
  try {
    const userId = (req as any).userId;
    const roomId = parseInt(req.params.roomId);

    const [room] = await db.select().from(rooms).where(eq(rooms.id, roomId));
    if (!room) return res.status(404).json({ error: 'Room not found' });
    if (room.roomType !== 'public') {
      return res.status(403).json({ error: 'Cannot join private room without invite' });
    }

    // Check if banned
    const [existingMember] = await db.select().from(roomMembers)
      .where(and(eq(roomMembers.roomId, roomId), eq(roomMembers.userId, userId)));

    if (existingMember) {
      if (existingMember.isBanned) {
        return res.status(403).json({ error: 'You are banned from this room' });
      }
      return res.json({ message: 'Already a member' });
    }

    await db.insert(roomMembers).values({
      roomId,
      userId,
      role: 'member',
    });

    const [user] = await db.select().from(users).where(eq(users.id, userId));
    io.to(`room:${roomId}`).emit('room:member:joined', { roomId, user });
    res.json({ message: 'Joined room' });
  } catch (error) {
    console.error('Join room error:', error);
    res.status(500).json({ error: 'Internal server error' });
  }
});

// Leave room
app.post('/api/rooms/:roomId/leave', authenticateToken, async (req, res) => {
  try {
    const userId = (req as any).userId;
    const roomId = parseInt(req.params.roomId);

    await db.delete(roomMembers)
      .where(and(eq(roomMembers.roomId, roomId), eq(roomMembers.userId, userId)));

    const [user] = await db.select().from(users).where(eq(users.id, userId));
    io.to(`room:${roomId}`).emit('room:member:left', { roomId, user });
    res.json({ message: 'Left room' });
  } catch (error) {
    console.error('Leave room error:', error);
    res.status(500).json({ error: 'Internal server error' });
  }
});

// Get room members
app.get('/api/rooms/:roomId/members', authenticateToken, async (req, res) => {
  try {
    const userId = (req as any).userId;
    const roomId = parseInt(req.params.roomId);

    // Check membership
    const [membership] = await db.select().from(roomMembers)
      .where(and(eq(roomMembers.roomId, roomId), eq(roomMembers.userId, userId), eq(roomMembers.isBanned, false)));
    if (!membership) return res.status(403).json({ error: 'Not a member of this room' });

    const members = await db.select({
      member: roomMembers,
      user: users,
    })
      .from(roomMembers)
      .innerJoin(users, eq(users.id, roomMembers.userId))
      .where(and(eq(roomMembers.roomId, roomId), eq(roomMembers.isBanned, false)));

    res.json(members.map(m => ({ ...m.user, role: m.member.role })));
  } catch (error) {
    console.error('Get members error:', error);
    res.status(500).json({ error: 'Internal server error' });
  }
});

// Kick user from room
app.post('/api/rooms/:roomId/kick/:targetUserId', authenticateToken, async (req, res) => {
  try {
    const userId = (req as any).userId;
    const roomId = parseInt(req.params.roomId);
    const targetUserId = req.params.targetUserId;

    // Check admin
    const [membership] = await db.select().from(roomMembers)
      .where(and(eq(roomMembers.roomId, roomId), eq(roomMembers.userId, userId)));
    if (!membership || membership.role !== 'admin') {
      return res.status(403).json({ error: 'Only admins can kick users' });
    }

    // Remove member
    await db.delete(roomMembers)
      .where(and(eq(roomMembers.roomId, roomId), eq(roomMembers.userId, targetUserId)));

    const [targetUser] = await db.select().from(users).where(eq(users.id, targetUserId));
    io.to(`room:${roomId}`).emit('room:member:kicked', { roomId, user: targetUser });
    io.to(`user:${targetUserId}`).emit('room:kicked', { roomId });
    res.json({ message: 'User kicked' });
  } catch (error) {
    console.error('Kick user error:', error);
    res.status(500).json({ error: 'Internal server error' });
  }
});

// Ban user from room
app.post('/api/rooms/:roomId/ban/:targetUserId', authenticateToken, async (req, res) => {
  try {
    const userId = (req as any).userId;
    const roomId = parseInt(req.params.roomId);
    const targetUserId = req.params.targetUserId;

    // Check admin
    const [membership] = await db.select().from(roomMembers)
      .where(and(eq(roomMembers.roomId, roomId), eq(roomMembers.userId, userId)));
    if (!membership || membership.role !== 'admin') {
      return res.status(403).json({ error: 'Only admins can ban users' });
    }

    // Update to banned or insert if not member
    const [existingMember] = await db.select().from(roomMembers)
      .where(and(eq(roomMembers.roomId, roomId), eq(roomMembers.userId, targetUserId)));

    if (existingMember) {
      await db.update(roomMembers)
        .set({ isBanned: true })
        .where(and(eq(roomMembers.roomId, roomId), eq(roomMembers.userId, targetUserId)));
    } else {
      await db.insert(roomMembers).values({
        roomId,
        userId: targetUserId,
        isBanned: true,
      });
    }

    const [targetUser] = await db.select().from(users).where(eq(users.id, targetUserId));
    io.to(`room:${roomId}`).emit('room:member:banned', { roomId, user: targetUser });
    io.to(`user:${targetUserId}`).emit('room:banned', { roomId });
    res.json({ message: 'User banned' });
  } catch (error) {
    console.error('Ban user error:', error);
    res.status(500).json({ error: 'Internal server error' });
  }
});

// Promote user to admin
app.post('/api/rooms/:roomId/promote/:targetUserId', authenticateToken, async (req, res) => {
  try {
    const userId = (req as any).userId;
    const roomId = parseInt(req.params.roomId);
    const targetUserId = req.params.targetUserId;

    // Check admin
    const [membership] = await db.select().from(roomMembers)
      .where(and(eq(roomMembers.roomId, roomId), eq(roomMembers.userId, userId)));
    if (!membership || membership.role !== 'admin') {
      return res.status(403).json({ error: 'Only admins can promote users' });
    }

    await db.update(roomMembers)
      .set({ role: 'admin' })
      .where(and(eq(roomMembers.roomId, roomId), eq(roomMembers.userId, targetUserId)));

    const [targetUser] = await db.select().from(users).where(eq(users.id, targetUserId));
    io.to(`room:${roomId}`).emit('room:member:promoted', { roomId, user: targetUser });
    res.json({ message: 'User promoted to admin' });
  } catch (error) {
    console.error('Promote user error:', error);
    res.status(500).json({ error: 'Internal server error' });
  }
});

// Invite user to private room
app.post('/api/rooms/:roomId/invite', authenticateToken, async (req, res) => {
  try {
    const userId = (req as any).userId;
    const roomId = parseInt(req.params.roomId);
    const { username } = req.body;

    // Check room and membership
    const [room] = await db.select().from(rooms).where(eq(rooms.id, roomId));
    if (!room) return res.status(404).json({ error: 'Room not found' });

    const [membership] = await db.select().from(roomMembers)
      .where(and(eq(roomMembers.roomId, roomId), eq(roomMembers.userId, userId), eq(roomMembers.isBanned, false)));
    if (!membership) return res.status(403).json({ error: 'Not a member of this room' });

    // Find user to invite
    const [invitedUser] = await db.select().from(users).where(eq(users.displayName, username));
    if (!invitedUser) return res.status(404).json({ error: 'User not found' });

    // Check if already member
    const [existingMember] = await db.select().from(roomMembers)
      .where(and(eq(roomMembers.roomId, roomId), eq(roomMembers.userId, invitedUser.id)));
    if (existingMember && !existingMember.isBanned) {
      return res.status(400).json({ error: 'User is already a member' });
    }

    // Create or update invite
    const [existingInvite] = await db.select().from(roomInvites)
      .where(and(eq(roomInvites.roomId, roomId), eq(roomInvites.invitedUser, invitedUser.id)));

    if (existingInvite) {
      if (existingInvite.status === 'pending') {
        return res.status(400).json({ error: 'Invite already pending' });
      }
      await db.update(roomInvites)
        .set({ status: 'pending', invitedBy: userId })
        .where(eq(roomInvites.id, existingInvite.id));
    } else {
      await db.insert(roomInvites).values({
        roomId,
        invitedBy: userId,
        invitedUser: invitedUser.id,
      });
    }

    const [inviter] = await db.select().from(users).where(eq(users.id, userId));
    io.to(`user:${invitedUser.id}`).emit('room:invite', { room, invitedBy: inviter });
    res.json({ message: 'Invite sent' });
  } catch (error) {
    console.error('Invite user error:', error);
    res.status(500).json({ error: 'Internal server error' });
  }
});

// Get pending invites
app.get('/api/invites', authenticateToken, async (req, res) => {
  try {
    const userId = (req as any).userId;
    const invites = await db.select({
      invite: roomInvites,
      room: rooms,
      inviter: users,
    })
      .from(roomInvites)
      .innerJoin(rooms, eq(rooms.id, roomInvites.roomId))
      .innerJoin(users, eq(users.id, roomInvites.invitedBy))
      .where(and(eq(roomInvites.invitedUser, userId), eq(roomInvites.status, 'pending')));

    res.json(invites.map(i => ({
      id: i.invite.id,
      room: i.room,
      invitedBy: i.inviter,
      createdAt: i.invite.createdAt,
    })));
  } catch (error) {
    console.error('Get invites error:', error);
    res.status(500).json({ error: 'Internal server error' });
  }
});

// Accept invite
app.post('/api/invites/:inviteId/accept', authenticateToken, async (req, res) => {
  try {
    const userId = (req as any).userId;
    const inviteId = parseInt(req.params.inviteId);

    const [invite] = await db.select().from(roomInvites)
      .where(and(eq(roomInvites.id, inviteId), eq(roomInvites.invitedUser, userId)));
    if (!invite) return res.status(404).json({ error: 'Invite not found' });
    if (invite.status !== 'pending') return res.status(400).json({ error: 'Invite already processed' });

    await db.update(roomInvites).set({ status: 'accepted' }).where(eq(roomInvites.id, inviteId));

    // Add as member (remove ban if exists)
    const [existingMember] = await db.select().from(roomMembers)
      .where(and(eq(roomMembers.roomId, invite.roomId), eq(roomMembers.userId, userId)));

    if (existingMember) {
      await db.update(roomMembers)
        .set({ isBanned: false, role: 'member' })
        .where(eq(roomMembers.id, existingMember.id));
    } else {
      await db.insert(roomMembers).values({
        roomId: invite.roomId,
        userId,
        role: 'member',
      });
    }

    const [room] = await db.select().from(rooms).where(eq(rooms.id, invite.roomId));
    const [user] = await db.select().from(users).where(eq(users.id, userId));
    io.to(`room:${invite.roomId}`).emit('room:member:joined', { roomId: invite.roomId, user });
    res.json({ room });
  } catch (error) {
    console.error('Accept invite error:', error);
    res.status(500).json({ error: 'Internal server error' });
  }
});

// Decline invite
app.post('/api/invites/:inviteId/decline', authenticateToken, async (req, res) => {
  try {
    const userId = (req as any).userId;
    const inviteId = parseInt(req.params.inviteId);

    const [invite] = await db.select().from(roomInvites)
      .where(and(eq(roomInvites.id, inviteId), eq(roomInvites.invitedUser, userId)));
    if (!invite) return res.status(404).json({ error: 'Invite not found' });

    await db.update(roomInvites).set({ status: 'declined' }).where(eq(roomInvites.id, inviteId));
    res.json({ message: 'Invite declined' });
  } catch (error) {
    console.error('Decline invite error:', error);
    res.status(500).json({ error: 'Internal server error' });
  }
});

// Create DM
app.post('/api/dm', authenticateToken, async (req, res) => {
  try {
    const userId = (req as any).userId;
    const { targetUserId } = req.body;

    if (!targetUserId) return res.status(400).json({ error: 'Target user required' });
    if (targetUserId === userId) return res.status(400).json({ error: 'Cannot DM yourself' });

    // Check if DM already exists
    const existingDms = await db.select({
      room: rooms,
      members: sql<string>`array_agg(${roomMembers.userId}::text)`,
    })
      .from(rooms)
      .innerJoin(roomMembers, eq(roomMembers.roomId, rooms.id))
      .where(eq(rooms.roomType, 'dm'))
      .groupBy(rooms.id);

    for (const dm of existingDms) {
      const memberIds = dm.members.replace(/[{}]/g, '').split(',');
      if (memberIds.includes(userId) && memberIds.includes(targetUserId) && memberIds.length === 2) {
        return res.json(dm.room);
      }
    }

    // Create new DM
    const [targetUser] = await db.select().from(users).where(eq(users.id, targetUserId));
    const [currentUser] = await db.select().from(users).where(eq(users.id, userId));
    if (!targetUser) return res.status(404).json({ error: 'User not found' });

    const [room] = await db.insert(rooms).values({
      name: `${currentUser.displayName} & ${targetUser.displayName}`,
      createdBy: userId,
      roomType: 'dm',
    }).returning();

    await db.insert(roomMembers).values([
      { roomId: room.id, userId, role: 'member' },
      { roomId: room.id, userId: targetUserId, role: 'member' },
    ]);

    io.to(`user:${targetUserId}`).emit('dm:created', { room, withUser: currentUser });
    res.json(room);
  } catch (error) {
    console.error('Create DM error:', error);
    res.status(500).json({ error: 'Internal server error' });
  }
});

// Get messages for room
app.get('/api/rooms/:roomId/messages', authenticateToken, async (req, res) => {
  try {
    const userId = (req as any).userId;
    const roomId = parseInt(req.params.roomId);

    // Check membership
    const [membership] = await db.select().from(roomMembers)
      .where(and(eq(roomMembers.roomId, roomId), eq(roomMembers.userId, userId), eq(roomMembers.isBanned, false)));
    if (!membership) return res.status(403).json({ error: 'Not a member of this room' });

    const roomMessages = await db.select({
      message: messages,
      user: users,
    })
      .from(messages)
      .innerJoin(users, eq(users.id, messages.userId))
      .where(and(
        eq(messages.roomId, roomId),
        or(eq(messages.isScheduled, false), and(eq(messages.isScheduled, true), eq(messages.userId, userId)))
      ))
      .orderBy(messages.createdAt);

    res.json(roomMessages.map(m => ({ ...m.message, user: m.user })));
  } catch (error) {
    console.error('Get messages error:', error);
    res.status(500).json({ error: 'Internal server error' });
  }
});

// Send message
app.post('/api/rooms/:roomId/messages', authenticateToken, async (req, res) => {
  try {
    const userId = (req as any).userId;
    const roomId = parseInt(req.params.roomId);
    const { content, parentId, scheduledFor, expiresInMinutes } = req.body;

    if (!content || typeof content !== 'string' || content.trim().length < 1 || content.length > 2000) {
      return res.status(400).json({ error: 'Message must be 1-2000 characters' });
    }

    // Check membership
    const [membership] = await db.select().from(roomMembers)
      .where(and(eq(roomMembers.roomId, roomId), eq(roomMembers.userId, userId), eq(roomMembers.isBanned, false)));
    if (!membership) return res.status(403).json({ error: 'Not a member of this room' });

    // Rate limiting
    const [user] = await db.select().from(users).where(eq(users.id, userId));
    if (user.lastMessageAt) {
      const timeSince = Date.now() - new Date(user.lastMessageAt).getTime();
      if (timeSince < 500) {
        return res.status(429).json({ error: 'Sending messages too quickly' });
      }
    }

    // Calculate expiration for ephemeral messages
    let expiresAt = null;
    if (expiresInMinutes && typeof expiresInMinutes === 'number' && expiresInMinutes > 0) {
      expiresAt = new Date(Date.now() + expiresInMinutes * 60 * 1000);
    }

    // Handle scheduled messages
    let isScheduled = false;
    let scheduledTime = null;
    if (scheduledFor) {
      scheduledTime = new Date(scheduledFor);
      if (scheduledTime > new Date()) {
        isScheduled = true;
      }
    }

    const [message] = await db.insert(messages).values({
      roomId,
      userId,
      content: content.trim(),
      parentId: parentId || null,
      scheduledFor: scheduledTime,
      isScheduled,
      expiresAt,
    }).returning();

    await db.update(users).set({ lastMessageAt: new Date() }).where(eq(users.id, userId));

    const messageWithUser = { ...message, user };

    if (!isScheduled) {
      io.to(`room:${roomId}`).emit('message:created', messageWithUser);

      // Update thread if reply
      if (parentId) {
        const replyCount = await db.select({ count: count() }).from(messages)
          .where(eq(messages.parentId, parentId));
        io.to(`room:${roomId}`).emit('message:thread:updated', { parentId, replyCount: replyCount[0].count });
      }
    }

    res.json(messageWithUser);
  } catch (error) {
    console.error('Send message error:', error);
    res.status(500).json({ error: 'Internal server error' });
  }
});

// Get scheduled messages
app.get('/api/rooms/:roomId/scheduled', authenticateToken, async (req, res) => {
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
  } catch (error) {
    console.error('Get scheduled error:', error);
    res.status(500).json({ error: 'Internal server error' });
  }
});

// Cancel scheduled message
app.delete('/api/messages/:messageId/scheduled', authenticateToken, async (req, res) => {
  try {
    const userId = (req as any).userId;
    const messageId = parseInt(req.params.messageId);

    const [message] = await db.select().from(messages)
      .where(and(eq(messages.id, messageId), eq(messages.userId, userId), eq(messages.isScheduled, true)));
    if (!message) return res.status(404).json({ error: 'Scheduled message not found' });

    await db.delete(messages).where(eq(messages.id, messageId));
    io.to(`user:${userId}`).emit('message:scheduled:cancelled', { messageId, roomId: message.roomId });
    res.json({ message: 'Scheduled message cancelled' });
  } catch (error) {
    console.error('Cancel scheduled error:', error);
    res.status(500).json({ error: 'Internal server error' });
  }
});

// Edit message
app.put('/api/messages/:messageId', authenticateToken, async (req, res) => {
  try {
    const userId = (req as any).userId;
    const messageId = parseInt(req.params.messageId);
    const { content } = req.body;

    if (!content || typeof content !== 'string' || content.trim().length < 1 || content.length > 2000) {
      return res.status(400).json({ error: 'Message must be 1-2000 characters' });
    }

    const [message] = await db.select().from(messages)
      .where(and(eq(messages.id, messageId), eq(messages.userId, userId)));
    if (!message) return res.status(404).json({ error: 'Message not found or not yours' });

    // Save edit history
    await db.insert(messageEdits).values({
      messageId,
      previousContent: message.content,
    });

    const [updatedMessage] = await db.update(messages)
      .set({ content: content.trim(), isEdited: true })
      .where(eq(messages.id, messageId))
      .returning();

    const [user] = await db.select().from(users).where(eq(users.id, userId));
    const messageWithUser = { ...updatedMessage, user };

    io.to(`room:${message.roomId}`).emit('message:updated', messageWithUser);
    res.json(messageWithUser);
  } catch (error) {
    console.error('Edit message error:', error);
    res.status(500).json({ error: 'Internal server error' });
  }
});

// Get edit history
app.get('/api/messages/:messageId/history', authenticateToken, async (req, res) => {
  try {
    const messageId = parseInt(req.params.messageId);
    const history = await db.select().from(messageEdits)
      .where(eq(messageEdits.messageId, messageId))
      .orderBy(desc(messageEdits.editedAt));
    res.json(history);
  } catch (error) {
    console.error('Get edit history error:', error);
    res.status(500).json({ error: 'Internal server error' });
  }
});

// Get thread messages
app.get('/api/messages/:messageId/thread', authenticateToken, async (req, res) => {
  try {
    const messageId = parseInt(req.params.messageId);
    const threadMessages = await db.select({
      message: messages,
      user: users,
    })
      .from(messages)
      .innerJoin(users, eq(users.id, messages.userId))
      .where(eq(messages.parentId, messageId))
      .orderBy(messages.createdAt);

    res.json(threadMessages.map(m => ({ ...m.message, user: m.user })));
  } catch (error) {
    console.error('Get thread error:', error);
    res.status(500).json({ error: 'Internal server error' });
  }
});

// Add reaction
app.post('/api/messages/:messageId/reactions', authenticateToken, async (req, res) => {
  try {
    const userId = (req as any).userId;
    const messageId = parseInt(req.params.messageId);
    const { emoji } = req.body;

    if (!emoji || typeof emoji !== 'string' || emoji.length > 10) {
      return res.status(400).json({ error: 'Invalid emoji' });
    }

    const [message] = await db.select().from(messages).where(eq(messages.id, messageId));
    if (!message) return res.status(404).json({ error: 'Message not found' });

    // Check membership
    const [membership] = await db.select().from(roomMembers)
      .where(and(eq(roomMembers.roomId, message.roomId), eq(roomMembers.userId, userId), eq(roomMembers.isBanned, false)));
    if (!membership) return res.status(403).json({ error: 'Not a member of this room' });

    // Toggle reaction
    const [existing] = await db.select().from(messageReactions)
      .where(and(
        eq(messageReactions.messageId, messageId),
        eq(messageReactions.userId, userId),
        eq(messageReactions.emoji, emoji)
      ));

    if (existing) {
      await db.delete(messageReactions).where(eq(messageReactions.id, existing.id));
    } else {
      await db.insert(messageReactions).values({ messageId, userId, emoji });
    }

    // Get updated reactions
    const reactions = await db.select({
      emoji: messageReactions.emoji,
      count: count(),
      users: sql<string>`array_agg(${users.displayName})`,
    })
      .from(messageReactions)
      .innerJoin(users, eq(users.id, messageReactions.userId))
      .where(eq(messageReactions.messageId, messageId))
      .groupBy(messageReactions.emoji);

    io.to(`room:${message.roomId}`).emit('message:reactions:updated', {
      messageId,
      reactions: reactions.map(r => ({
        emoji: r.emoji,
        count: Number(r.count),
        users: r.users.replace(/[{}]/g, '').split(',').filter(Boolean),
      })),
    });

    res.json({ success: true });
  } catch (error) {
    console.error('Add reaction error:', error);
    res.status(500).json({ error: 'Internal server error' });
  }
});

// Get reactions for message
app.get('/api/messages/:messageId/reactions', authenticateToken, async (req, res) => {
  try {
    const messageId = parseInt(req.params.messageId);
    const reactions = await db.select({
      emoji: messageReactions.emoji,
      count: count(),
      users: sql<string>`array_agg(${users.displayName})`,
    })
      .from(messageReactions)
      .innerJoin(users, eq(users.id, messageReactions.userId))
      .where(eq(messageReactions.messageId, messageId))
      .groupBy(messageReactions.emoji);

    res.json(reactions.map(r => ({
      emoji: r.emoji,
      count: Number(r.count),
      users: r.users.replace(/[{}]/g, '').split(',').filter(Boolean),
    })));
  } catch (error) {
    console.error('Get reactions error:', error);
    res.status(500).json({ error: 'Internal server error' });
  }
});

// Mark message as read
app.post('/api/messages/:messageId/read', authenticateToken, async (req, res) => {
  try {
    const userId = (req as any).userId;
    const messageId = parseInt(req.params.messageId);

    const [message] = await db.select().from(messages).where(eq(messages.id, messageId));
    if (!message) return res.status(404).json({ error: 'Message not found' });

    // Upsert read receipt
    await db.insert(readReceipts)
      .values({ messageId, userId })
      .onConflictDoUpdate({
        target: [readReceipts.messageId, readReceipts.userId],
        set: { readAt: new Date() },
      });

    // Update last read for room
    await db.update(roomMembers)
      .set({ lastReadAt: new Date() })
      .where(and(eq(roomMembers.roomId, message.roomId), eq(roomMembers.userId, userId)));

    // Get read receipts for this message
    const receipts = await db.select({
      user: users,
    })
      .from(readReceipts)
      .innerJoin(users, eq(users.id, readReceipts.userId))
      .where(eq(readReceipts.messageId, messageId));

    io.to(`room:${message.roomId}`).emit('message:read', {
      messageId,
      readers: receipts.map(r => r.user.displayName),
    });

    res.json({ success: true });
  } catch (error) {
    console.error('Mark read error:', error);
    res.status(500).json({ error: 'Internal server error' });
  }
});

// Get read receipts for message
app.get('/api/messages/:messageId/read', authenticateToken, async (req, res) => {
  try {
    const messageId = parseInt(req.params.messageId);
    const receipts = await db.select({
      user: users,
      readAt: readReceipts.readAt,
    })
      .from(readReceipts)
      .innerJoin(users, eq(users.id, readReceipts.userId))
      .where(eq(readReceipts.messageId, messageId));

    res.json(receipts.map(r => ({ user: r.user.displayName, readAt: r.readAt })));
  } catch (error) {
    console.error('Get read receipts error:', error);
    res.status(500).json({ error: 'Internal server error' });
  }
});

// Get unread counts for all rooms
app.get('/api/rooms/unread', authenticateToken, async (req, res) => {
  try {
    const userId = (req as any).userId;

    const memberRooms = await db.select({
      roomId: roomMembers.roomId,
      lastReadAt: roomMembers.lastReadAt,
    })
      .from(roomMembers)
      .where(and(eq(roomMembers.userId, userId), eq(roomMembers.isBanned, false)));

    const unreadCounts: Record<number, number> = {};

    for (const room of memberRooms) {
      const [result] = await db.select({ count: count() })
        .from(messages)
        .where(and(
          eq(messages.roomId, room.roomId),
          gt(messages.createdAt, room.lastReadAt),
          eq(messages.isScheduled, false),
          ne(messages.userId, userId)
        ));
      unreadCounts[room.roomId] = Number(result.count);
    }

    res.json(unreadCounts);
  } catch (error) {
    console.error('Get unread counts error:', error);
    res.status(500).json({ error: 'Internal server error' });
  }
});

// ============ SOCKET.IO ============

io.use((socket, next) => {
  const token = socket.handshake.auth.token;
  if (!token) return next(new Error('Unauthorized'));

  try {
    const decoded = jwt.verify(token, JWT_SECRET) as { userId: string };
    socket.data.userId = decoded.userId;
    next();
  } catch {
    next(new Error('Invalid token'));
  }
});

io.on('connection', async (socket) => {
  const userId = socket.data.userId;
  console.log('User connected:', userId);

  // Join personal room for targeted messages
  socket.join(`user:${userId}`);

  // Update user status to online
  await db.update(users)
    .set({ status: 'online', lastActiveAt: new Date() })
    .where(eq(users.id, userId));

  const [user] = await db.select().from(users).where(eq(users.id, userId));
  io.emit('user:updated', user);

  // Join all rooms user is a member of
  const memberships = await db.select().from(roomMembers)
    .where(and(eq(roomMembers.userId, userId), eq(roomMembers.isBanned, false)));

  for (const membership of memberships) {
    socket.join(`room:${membership.roomId}`);
  }

  // Typing indicator
  socket.on('typing:start', async (roomId: number) => {
    const expiresAt = new Date(Date.now() + TYPING_TIMEOUT_MS);

    await db.insert(typingIndicators)
      .values({ roomId, userId, expiresAt })
      .onConflictDoUpdate({
        target: [typingIndicators.roomId, typingIndicators.userId],
        set: { startedAt: new Date(), expiresAt },
      });

    socket.to(`room:${roomId}`).emit('typing:update', { roomId, userId, isTyping: true });
  });

  socket.on('typing:stop', async (roomId: number) => {
    await db.delete(typingIndicators)
      .where(and(eq(typingIndicators.roomId, roomId), eq(typingIndicators.userId, userId)));

    socket.to(`room:${roomId}`).emit('typing:update', { roomId, userId, isTyping: false });
  });

  // Activity ping (for auto-away)
  socket.on('activity', async () => {
    await db.update(users)
      .set({ lastActiveAt: new Date() })
      .where(eq(users.id, userId));
  });

  // Join room socket channel
  socket.on('room:join', (roomId: number) => {
    socket.join(`room:${roomId}`);
  });

  socket.on('room:leave', (roomId: number) => {
    socket.leave(`room:${roomId}`);
  });

  socket.on('disconnect', async () => {
    console.log('User disconnected:', userId);

    // Set status to offline
    await db.update(users)
      .set({ status: 'offline', lastActiveAt: new Date() })
      .where(eq(users.id, userId));

    const [updatedUser] = await db.select().from(users).where(eq(users.id, userId));
    io.emit('user:updated', updatedUser);

    // Clean up typing indicators
    await db.delete(typingIndicators).where(eq(typingIndicators.userId, userId));
  });
});

// ============ BACKGROUND JOBS ============

// Process scheduled messages
setInterval(async () => {
  try {
    const now = new Date();
    const dueMessages = await db.select().from(messages)
      .where(and(
        eq(messages.isScheduled, true),
        lte(messages.scheduledFor, now)
      ));

    for (const message of dueMessages) {
      await db.update(messages)
        .set({ isScheduled: false })
        .where(eq(messages.id, message.id));

      const [user] = await db.select().from(users).where(eq(users.id, message.userId));
      const messageWithUser = { ...message, isScheduled: false, user };

      io.to(`room:${message.roomId}`).emit('message:created', messageWithUser);
    }
  } catch (error) {
    console.error('Scheduled message processing error:', error);
  }
}, 5000);

// Clean up expired ephemeral messages
setInterval(async () => {
  try {
    const now = new Date();
    const expiredMessages = await db.select().from(messages)
      .where(and(
        lte(messages.expiresAt, now),
        sql`${messages.expiresAt} IS NOT NULL`
      ));

    for (const message of expiredMessages) {
      await db.delete(messages).where(eq(messages.id, message.id));
      io.to(`room:${message.roomId}`).emit('message:deleted', { messageId: message.id, roomId: message.roomId });
    }
  } catch (error) {
    console.error('Ephemeral message cleanup error:', error);
  }
}, 5000);

// Clean up expired typing indicators
setInterval(async () => {
  try {
    const now = new Date();
    const expired = await db.select().from(typingIndicators)
      .where(lte(typingIndicators.expiresAt, now));

    for (const indicator of expired) {
      await db.delete(typingIndicators).where(eq(typingIndicators.id, indicator.id));
      io.to(`room:${indicator.roomId}`).emit('typing:update', { roomId: indicator.roomId, userId: indicator.userId, isTyping: false });
    }
  } catch (error) {
    console.error('Typing indicator cleanup error:', error);
  }
}, 1000);

// Auto-set users to away after inactivity
setInterval(async () => {
  try {
    const threshold = new Date(Date.now() - INACTIVITY_TIMEOUT_MS);
    const inactiveUsers = await db.select().from(users)
      .where(and(
        eq(users.status, 'online'),
        lt(users.lastActiveAt, threshold)
      ));

    for (const user of inactiveUsers) {
      await db.update(users)
        .set({ status: 'away' })
        .where(eq(users.id, user.id));

      const [updated] = await db.select().from(users).where(eq(users.id, user.id));
      io.emit('user:updated', updated);
    }
  } catch (error) {
    console.error('Auto-away error:', error);
  }
}, 30000);

// ============ START SERVER ============

httpServer.listen(PORT, () => {
  console.log(`Server running on port ${PORT}`);
});

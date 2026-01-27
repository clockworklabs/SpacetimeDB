import express from 'express';
import cors from 'cors';
import { createServer } from 'http';
import { Server } from 'socket.io';
import jwt from 'jsonwebtoken';
import cron from 'node-cron';
import { db } from './db/index.js';
import * as schema from './db/schema.js';
import {
  eq,
  and,
  lt,
  lte,
  isNull,
  isNotNull,
  ne,
  or,
  sql,
  inArray,
  desc,
  asc,
} from 'drizzle-orm';

const app = express();
const httpServer = createServer(app);

const JWT_SECRET = process.env.JWT_SECRET || 'chat-app-20260104-120000-secret';
const PORT = process.env.PORT || 3001;
const TYPING_TIMEOUT_SECONDS = 5;
const RATE_LIMIT_MS = 500;
const AUTO_AWAY_MS = 5 * 60 * 1000; // 5 minutes

app.use(cors());
app.use(express.json());

const io = new Server(httpServer, {
  cors: {
    origin: '*',
    methods: ['GET', 'POST'],
  },
});

// =====================
// JWT Authentication
// =====================
function generateToken(userId: string): string {
  return jwt.sign({ userId }, JWT_SECRET, { expiresIn: '7d' });
}

function verifyToken(token: string): { userId: string } | null {
  try {
    return jwt.verify(token, JWT_SECRET) as { userId: string };
  } catch {
    return null;
  }
}

// Express middleware for auth
function authMiddleware(
  req: express.Request,
  res: express.Response,
  next: express.NextFunction
) {
  const authHeader = req.headers.authorization;
  if (!authHeader?.startsWith('Bearer ')) {
    return res.status(401).json({ error: 'Unauthorized' });
  }

  const token = authHeader.slice(7);
  const decoded = verifyToken(token);
  if (!decoded) {
    return res.status(401).json({ error: 'Invalid token' });
  }

  (req as any).userId = decoded.userId;
  next();
}

// Rate limiting check
async function checkRateLimit(userId: string): Promise<boolean> {
  const [user] = await db
    .select()
    .from(schema.users)
    .where(eq(schema.users.id, userId));
  if (!user) return false;

  if (user.lastActionAt) {
    const elapsed = Date.now() - user.lastActionAt.getTime();
    if (elapsed < RATE_LIMIT_MS) {
      return false;
    }
  }

  await db
    .update(schema.users)
    .set({ lastActionAt: new Date(), lastActiveAt: new Date() })
    .where(eq(schema.users.id, userId));

  return true;
}

// =====================
// REST API Routes
// =====================

// Register/Login
app.post('/api/auth/register', async (req, res) => {
  const { displayName } = req.body;

  if (!displayName || typeof displayName !== 'string') {
    return res.status(400).json({ error: 'Display name is required' });
  }

  if (displayName.length < 1 || displayName.length > 50) {
    return res
      .status(400)
      .json({ error: 'Display name must be 1-50 characters' });
  }

  try {
    const [user] = await db
      .insert(schema.users)
      .values({ displayName: displayName.trim() })
      .returning();

    const token = generateToken(user.id);

    io.emit('user:online', { user });

    res.json({ user, token });
  } catch (err) {
    console.error('Register error:', err);
    res.status(500).json({ error: 'Failed to register' });
  }
});

// Get current user
app.get('/api/users/me', authMiddleware, async (req, res) => {
  const userId = (req as any).userId;

  try {
    const [user] = await db
      .select()
      .from(schema.users)
      .where(eq(schema.users.id, userId));
    if (!user) {
      return res.status(404).json({ error: 'User not found' });
    }
    res.json({ user });
  } catch (err) {
    console.error('Get user error:', err);
    res.status(500).json({ error: 'Failed to get user' });
  }
});

// Update display name
app.patch('/api/users/me', authMiddleware, async (req, res) => {
  const userId = (req as any).userId;
  const { displayName } = req.body;

  if (!displayName || typeof displayName !== 'string') {
    return res.status(400).json({ error: 'Display name is required' });
  }

  if (displayName.length < 1 || displayName.length > 50) {
    return res
      .status(400)
      .json({ error: 'Display name must be 1-50 characters' });
  }

  try {
    const [user] = await db
      .update(schema.users)
      .set({ displayName: displayName.trim() })
      .where(eq(schema.users.id, userId))
      .returning();

    io.emit('user:updated', { user });

    res.json({ user });
  } catch (err) {
    console.error('Update user error:', err);
    res.status(500).json({ error: 'Failed to update user' });
  }
});

// Update user status
app.patch('/api/users/me/status', authMiddleware, async (req, res) => {
  const userId = (req as any).userId;
  const { status } = req.body;

  if (!['online', 'away', 'dnd', 'invisible'].includes(status)) {
    return res.status(400).json({ error: 'Invalid status' });
  }

  try {
    const [user] = await db
      .update(schema.users)
      .set({ status, lastActiveAt: new Date() })
      .where(eq(schema.users.id, userId))
      .returning();

    if (status !== 'invisible') {
      io.emit('user:status', {
        userId,
        status,
        lastActiveAt: user.lastActiveAt,
      });
    }

    res.json({ user });
  } catch (err) {
    console.error('Update status error:', err);
    res.status(500).json({ error: 'Failed to update status' });
  }
});

// Get all users
app.get('/api/users', authMiddleware, async (req, res) => {
  try {
    const allUsers = await db.select().from(schema.users);
    res.json({ users: allUsers });
  } catch (err) {
    console.error('Get users error:', err);
    res.status(500).json({ error: 'Failed to get users' });
  }
});

// Search users by display name
app.get('/api/users/search', authMiddleware, async (req, res) => {
  const { q } = req.query;

  if (!q || typeof q !== 'string') {
    return res.status(400).json({ error: 'Search query is required' });
  }

  try {
    const users = await db
      .select()
      .from(schema.users)
      .where(
        sql`LOWER(${schema.users.displayName}) LIKE LOWER(${'%' + q + '%'})`
      );
    res.json({ users });
  } catch (err) {
    console.error('Search users error:', err);
    res.status(500).json({ error: 'Failed to search users' });
  }
});

// Get public rooms
app.get('/api/rooms', authMiddleware, async (req, res) => {
  const userId = (req as any).userId;

  try {
    // Get public rooms and rooms user is a member of
    const publicRooms = await db
      .select()
      .from(schema.rooms)
      .where(eq(schema.rooms.isPrivate, false));

    const memberships = await db
      .select({ roomId: schema.roomMembers.roomId })
      .from(schema.roomMembers)
      .where(
        and(
          eq(schema.roomMembers.userId, userId),
          eq(schema.roomMembers.isBanned, false)
        )
      );

    const memberRoomIds = memberships.map(m => m.roomId);

    let privateRooms: schema.Room[] = [];
    if (memberRoomIds.length > 0) {
      privateRooms = await db
        .select()
        .from(schema.rooms)
        .where(
          and(
            eq(schema.rooms.isPrivate, true),
            inArray(schema.rooms.id, memberRoomIds)
          )
        );
    }

    const allRooms = [...publicRooms, ...privateRooms];
    res.json({ rooms: allRooms });
  } catch (err) {
    console.error('Get rooms error:', err);
    res.status(500).json({ error: 'Failed to get rooms' });
  }
});

// Create room
app.post('/api/rooms', authMiddleware, async (req, res) => {
  const userId = (req as any).userId;
  const { name, isPrivate } = req.body;

  if (!name || typeof name !== 'string') {
    return res.status(400).json({ error: 'Room name is required' });
  }

  if (name.length < 1 || name.length > 100) {
    return res
      .status(400)
      .json({ error: 'Room name must be 1-100 characters' });
  }

  try {
    const [room] = await db
      .insert(schema.rooms)
      .values({
        name: name.trim(),
        isPrivate: !!isPrivate,
        createdBy: userId,
      })
      .returning();

    // Add creator as admin member
    await db
      .insert(schema.roomMembers)
      .values({ roomId: room.id, userId, isAdmin: true });

    if (!isPrivate) {
      io.emit('room:created', { room });
    }

    res.json({ room });
  } catch (err) {
    console.error('Create room error:', err);
    res.status(500).json({ error: 'Failed to create room' });
  }
});

// Create DM
app.post('/api/dms', authMiddleware, async (req, res) => {
  const userId = (req as any).userId;
  const { targetUserId } = req.body;

  if (!targetUserId) {
    return res.status(400).json({ error: 'Target user is required' });
  }

  try {
    // Check if DM already exists
    const existingDms = await db
      .select({ roomId: schema.roomMembers.roomId })
      .from(schema.roomMembers)
      .innerJoin(schema.rooms, eq(schema.rooms.id, schema.roomMembers.roomId))
      .where(
        and(eq(schema.rooms.isDm, true), eq(schema.roomMembers.userId, userId))
      );

    for (const dm of existingDms) {
      const members = await db
        .select()
        .from(schema.roomMembers)
        .where(eq(schema.roomMembers.roomId, dm.roomId));
      if (
        members.length === 2 &&
        members.some(m => m.userId === targetUserId)
      ) {
        const [room] = await db
          .select()
          .from(schema.rooms)
          .where(eq(schema.rooms.id, dm.roomId));
        return res.json({ room });
      }
    }

    // Get target user name
    const [targetUser] = await db
      .select()
      .from(schema.users)
      .where(eq(schema.users.id, targetUserId));
    const [currentUser] = await db
      .select()
      .from(schema.users)
      .where(eq(schema.users.id, userId));

    if (!targetUser) {
      return res.status(404).json({ error: 'User not found' });
    }

    // Create new DM
    const [room] = await db
      .insert(schema.rooms)
      .values({
        name: `${currentUser.displayName} & ${targetUser.displayName}`,
        isPrivate: true,
        isDm: true,
        createdBy: userId,
      })
      .returning();

    // Add both users
    await db.insert(schema.roomMembers).values([
      { roomId: room.id, userId, isAdmin: true },
      { roomId: room.id, userId: targetUserId, isAdmin: true },
    ]);

    io.to(`user:${userId}`).emit('room:created', { room });
    io.to(`user:${targetUserId}`).emit('room:created', { room });

    res.json({ room });
  } catch (err) {
    console.error('Create DM error:', err);
    res.status(500).json({ error: 'Failed to create DM' });
  }
});

// Join room
app.post('/api/rooms/:roomId/join', authMiddleware, async (req, res) => {
  const userId = (req as any).userId;
  const roomId = parseInt(req.params.roomId);

  try {
    const [room] = await db
      .select()
      .from(schema.rooms)
      .where(eq(schema.rooms.id, roomId));
    if (!room) {
      return res.status(404).json({ error: 'Room not found' });
    }

    if (room.isPrivate) {
      return res
        .status(403)
        .json({ error: 'Cannot join private room directly' });
    }

    // Check if banned
    const [existing] = await db
      .select()
      .from(schema.roomMembers)
      .where(
        and(
          eq(schema.roomMembers.roomId, roomId),
          eq(schema.roomMembers.userId, userId)
        )
      );

    if (existing?.isBanned) {
      return res.status(403).json({ error: 'You are banned from this room' });
    }

    if (existing) {
      return res.json({ member: existing });
    }

    const [member] = await db
      .insert(schema.roomMembers)
      .values({ roomId, userId })
      .returning();

    const [user] = await db
      .select()
      .from(schema.users)
      .where(eq(schema.users.id, userId));
    io.to(`room:${roomId}`).emit('room:member:joined', { roomId, user });

    res.json({ member });
  } catch (err) {
    console.error('Join room error:', err);
    res.status(500).json({ error: 'Failed to join room' });
  }
});

// Leave room
app.post('/api/rooms/:roomId/leave', authMiddleware, async (req, res) => {
  const userId = (req as any).userId;
  const roomId = parseInt(req.params.roomId);

  try {
    await db
      .delete(schema.roomMembers)
      .where(
        and(
          eq(schema.roomMembers.roomId, roomId),
          eq(schema.roomMembers.userId, userId)
        )
      );

    io.to(`room:${roomId}`).emit('room:member:left', { roomId, userId });

    res.json({ success: true });
  } catch (err) {
    console.error('Leave room error:', err);
    res.status(500).json({ error: 'Failed to leave room' });
  }
});

// Get room members
app.get('/api/rooms/:roomId/members', authMiddleware, async (req, res) => {
  const userId = (req as any).userId;
  const roomId = parseInt(req.params.roomId);

  try {
    const [room] = await db
      .select()
      .from(schema.rooms)
      .where(eq(schema.rooms.id, roomId));
    if (!room) {
      return res.status(404).json({ error: 'Room not found' });
    }

    // Check access for private rooms
    if (room.isPrivate) {
      const [membership] = await db
        .select()
        .from(schema.roomMembers)
        .where(
          and(
            eq(schema.roomMembers.roomId, roomId),
            eq(schema.roomMembers.userId, userId),
            eq(schema.roomMembers.isBanned, false)
          )
        );
      if (!membership) {
        return res.status(403).json({ error: 'Access denied' });
      }
    }

    const members = await db
      .select({
        member: schema.roomMembers,
        user: schema.users,
      })
      .from(schema.roomMembers)
      .innerJoin(schema.users, eq(schema.users.id, schema.roomMembers.userId))
      .where(
        and(
          eq(schema.roomMembers.roomId, roomId),
          eq(schema.roomMembers.isBanned, false)
        )
      );

    res.json({ members });
  } catch (err) {
    console.error('Get members error:', err);
    res.status(500).json({ error: 'Failed to get members' });
  }
});

// Invite user to private room
app.post('/api/rooms/:roomId/invite', authMiddleware, async (req, res) => {
  const userId = (req as any).userId;
  const roomId = parseInt(req.params.roomId);
  const { username } = req.body;

  try {
    // Check if user is admin
    const [membership] = await db
      .select()
      .from(schema.roomMembers)
      .where(
        and(
          eq(schema.roomMembers.roomId, roomId),
          eq(schema.roomMembers.userId, userId),
          eq(schema.roomMembers.isAdmin, true)
        )
      );

    if (!membership) {
      return res.status(403).json({ error: 'Only admins can invite users' });
    }

    // Find user by display name
    const [targetUser] = await db
      .select()
      .from(schema.users)
      .where(eq(schema.users.displayName, username));

    if (!targetUser) {
      return res.status(404).json({ error: 'User not found' });
    }

    // Check if already member
    const [existingMember] = await db
      .select()
      .from(schema.roomMembers)
      .where(
        and(
          eq(schema.roomMembers.roomId, roomId),
          eq(schema.roomMembers.userId, targetUser.id)
        )
      );

    if (existingMember) {
      return res.status(400).json({ error: 'User is already a member' });
    }

    // Check if already invited
    const [existingInvite] = await db
      .select()
      .from(schema.roomInvitations)
      .where(
        and(
          eq(schema.roomInvitations.roomId, roomId),
          eq(schema.roomInvitations.invitedUserId, targetUser.id),
          eq(schema.roomInvitations.status, 'pending')
        )
      );

    if (existingInvite) {
      return res
        .status(400)
        .json({ error: 'User already has a pending invitation' });
    }

    const [invitation] = await db
      .insert(schema.roomInvitations)
      .values({ roomId, invitedUserId: targetUser.id, invitedBy: userId })
      .returning();

    const [room] = await db
      .select()
      .from(schema.rooms)
      .where(eq(schema.rooms.id, roomId));
    const [inviter] = await db
      .select()
      .from(schema.users)
      .where(eq(schema.users.id, userId));

    io.to(`user:${targetUser.id}`).emit('invitation:received', {
      invitation,
      room,
      inviter,
    });

    res.json({ invitation });
  } catch (err) {
    console.error('Invite error:', err);
    res.status(500).json({ error: 'Failed to send invitation' });
  }
});

// Get my invitations
app.get('/api/invitations', authMiddleware, async (req, res) => {
  const userId = (req as any).userId;

  try {
    const invitations = await db
      .select({
        invitation: schema.roomInvitations,
        room: schema.rooms,
        inviter: schema.users,
      })
      .from(schema.roomInvitations)
      .innerJoin(
        schema.rooms,
        eq(schema.rooms.id, schema.roomInvitations.roomId)
      )
      .innerJoin(
        schema.users,
        eq(schema.users.id, schema.roomInvitations.invitedBy)
      )
      .where(
        and(
          eq(schema.roomInvitations.invitedUserId, userId),
          eq(schema.roomInvitations.status, 'pending')
        )
      );

    res.json({ invitations });
  } catch (err) {
    console.error('Get invitations error:', err);
    res.status(500).json({ error: 'Failed to get invitations' });
  }
});

// Accept/Decline invitation
app.post(
  '/api/invitations/:invitationId/:action',
  authMiddleware,
  async (req, res) => {
    const userId = (req as any).userId;
    const invitationId = parseInt(req.params.invitationId);
    const action = req.params.action;

    if (!['accept', 'decline'].includes(action)) {
      return res.status(400).json({ error: 'Invalid action' });
    }

    try {
      const [invitation] = await db
        .select()
        .from(schema.roomInvitations)
        .where(
          and(
            eq(schema.roomInvitations.id, invitationId),
            eq(schema.roomInvitations.invitedUserId, userId),
            eq(schema.roomInvitations.status, 'pending')
          )
        );

      if (!invitation) {
        return res.status(404).json({ error: 'Invitation not found' });
      }

      if (action === 'accept') {
        await db
          .update(schema.roomInvitations)
          .set({ status: 'accepted' })
          .where(eq(schema.roomInvitations.id, invitationId));

        await db
          .insert(schema.roomMembers)
          .values({ roomId: invitation.roomId, userId });

        const [room] = await db
          .select()
          .from(schema.rooms)
          .where(eq(schema.rooms.id, invitation.roomId));
        const [user] = await db
          .select()
          .from(schema.users)
          .where(eq(schema.users.id, userId));

        io.to(`room:${invitation.roomId}`).emit('room:member:joined', {
          roomId: invitation.roomId,
          user,
        });
        io.to(`user:${userId}`).emit('room:created', { room });

        res.json({ success: true, room });
      } else {
        await db
          .update(schema.roomInvitations)
          .set({ status: 'declined' })
          .where(eq(schema.roomInvitations.id, invitationId));

        res.json({ success: true });
      }
    } catch (err) {
      console.error('Invitation action error:', err);
      res.status(500).json({ error: 'Failed to process invitation' });
    }
  }
);

// Kick user from room
app.post(
  '/api/rooms/:roomId/kick/:targetUserId',
  authMiddleware,
  async (req, res) => {
    const userId = (req as any).userId;
    const roomId = parseInt(req.params.roomId);
    const targetUserId = req.params.targetUserId;

    try {
      const [membership] = await db
        .select()
        .from(schema.roomMembers)
        .where(
          and(
            eq(schema.roomMembers.roomId, roomId),
            eq(schema.roomMembers.userId, userId),
            eq(schema.roomMembers.isAdmin, true)
          )
        );

      if (!membership) {
        return res.status(403).json({ error: 'Only admins can kick users' });
      }

      await db
        .delete(schema.roomMembers)
        .where(
          and(
            eq(schema.roomMembers.roomId, roomId),
            eq(schema.roomMembers.userId, targetUserId)
          )
        );

      io.to(`room:${roomId}`).emit('room:member:kicked', {
        roomId,
        userId: targetUserId,
      });
      io.to(`user:${targetUserId}`).emit('room:kicked', { roomId });

      res.json({ success: true });
    } catch (err) {
      console.error('Kick error:', err);
      res.status(500).json({ error: 'Failed to kick user' });
    }
  }
);

// Ban user from room
app.post(
  '/api/rooms/:roomId/ban/:targetUserId',
  authMiddleware,
  async (req, res) => {
    const userId = (req as any).userId;
    const roomId = parseInt(req.params.roomId);
    const targetUserId = req.params.targetUserId;

    try {
      const [membership] = await db
        .select()
        .from(schema.roomMembers)
        .where(
          and(
            eq(schema.roomMembers.roomId, roomId),
            eq(schema.roomMembers.userId, userId),
            eq(schema.roomMembers.isAdmin, true)
          )
        );

      if (!membership) {
        return res.status(403).json({ error: 'Only admins can ban users' });
      }

      await db
        .update(schema.roomMembers)
        .set({ isBanned: true })
        .where(
          and(
            eq(schema.roomMembers.roomId, roomId),
            eq(schema.roomMembers.userId, targetUserId)
          )
        );

      io.to(`room:${roomId}`).emit('room:member:banned', {
        roomId,
        userId: targetUserId,
      });
      io.to(`user:${targetUserId}`).emit('room:banned', { roomId });

      res.json({ success: true });
    } catch (err) {
      console.error('Ban error:', err);
      res.status(500).json({ error: 'Failed to ban user' });
    }
  }
);

// Promote user to admin
app.post(
  '/api/rooms/:roomId/promote/:targetUserId',
  authMiddleware,
  async (req, res) => {
    const userId = (req as any).userId;
    const roomId = parseInt(req.params.roomId);
    const targetUserId = req.params.targetUserId;

    try {
      const [membership] = await db
        .select()
        .from(schema.roomMembers)
        .where(
          and(
            eq(schema.roomMembers.roomId, roomId),
            eq(schema.roomMembers.userId, userId),
            eq(schema.roomMembers.isAdmin, true)
          )
        );

      if (!membership) {
        return res.status(403).json({ error: 'Only admins can promote users' });
      }

      const [updated] = await db
        .update(schema.roomMembers)
        .set({ isAdmin: true })
        .where(
          and(
            eq(schema.roomMembers.roomId, roomId),
            eq(schema.roomMembers.userId, targetUserId)
          )
        )
        .returning();

      io.to(`room:${roomId}`).emit('room:member:promoted', {
        roomId,
        userId: targetUserId,
      });

      res.json({ success: true, member: updated });
    } catch (err) {
      console.error('Promote error:', err);
      res.status(500).json({ error: 'Failed to promote user' });
    }
  }
);

// Get messages for room
app.get('/api/rooms/:roomId/messages', authMiddleware, async (req, res) => {
  const userId = (req as any).userId;
  const roomId = parseInt(req.params.roomId);

  try {
    const [room] = await db
      .select()
      .from(schema.rooms)
      .where(eq(schema.rooms.id, roomId));
    if (!room) {
      return res.status(404).json({ error: 'Room not found' });
    }

    // Check access for private rooms
    if (room.isPrivate) {
      const [membership] = await db
        .select()
        .from(schema.roomMembers)
        .where(
          and(
            eq(schema.roomMembers.roomId, roomId),
            eq(schema.roomMembers.userId, userId),
            eq(schema.roomMembers.isBanned, false)
          )
        );
      if (!membership) {
        return res.status(403).json({ error: 'Access denied' });
      }
    }

    // Get messages that are not scheduled or whose scheduled time has passed
    const messages = await db
      .select({
        message: schema.messages,
        user: schema.users,
      })
      .from(schema.messages)
      .innerJoin(schema.users, eq(schema.users.id, schema.messages.userId))
      .where(
        and(
          eq(schema.messages.roomId, roomId),
          or(
            isNull(schema.messages.scheduledFor),
            lte(schema.messages.scheduledFor, new Date())
          )
        )
      )
      .orderBy(asc(schema.messages.createdAt))
      .limit(100);

    // Get reactions for these messages
    const messageIds = messages.map(m => m.message.id);
    let reactions: schema.MessageReaction[] = [];
    if (messageIds.length > 0) {
      reactions = await db
        .select()
        .from(schema.messageReactions)
        .where(inArray(schema.messageReactions.messageId, messageIds));
    }

    // Get read receipts
    let receipts: Array<{ receipt: schema.ReadReceipt; user: schema.User }> =
      [];
    if (messageIds.length > 0) {
      receipts = await db
        .select({
          receipt: schema.readReceipts,
          user: schema.users,
        })
        .from(schema.readReceipts)
        .innerJoin(
          schema.users,
          eq(schema.users.id, schema.readReceipts.userId)
        )
        .where(inArray(schema.readReceipts.messageId, messageIds));
    }

    // Get reply counts
    let replyCounts: Array<{ replyToId: number | null; count: number }> = [];
    if (messageIds.length > 0) {
      replyCounts = await db
        .select({
          replyToId: schema.messages.replyToId,
          count: sql<number>`count(*)::int`,
        })
        .from(schema.messages)
        .where(
          and(
            inArray(schema.messages.replyToId, messageIds),
            or(
              isNull(schema.messages.scheduledFor),
              lte(schema.messages.scheduledFor, new Date())
            )
          )
        )
        .groupBy(schema.messages.replyToId);
    }

    res.json({ messages, reactions, receipts, replyCounts });
  } catch (err) {
    console.error('Get messages error:', err);
    res.status(500).json({ error: 'Failed to get messages' });
  }
});

// Get thread replies
app.get(
  '/api/messages/:messageId/replies',
  authMiddleware,
  async (req, res) => {
    const userId = (req as any).userId;
    const messageId = parseInt(req.params.messageId);

    try {
      const [parentMessage] = await db
        .select()
        .from(schema.messages)
        .where(eq(schema.messages.id, messageId));

      if (!parentMessage) {
        return res.status(404).json({ error: 'Message not found' });
      }

      const [room] = await db
        .select()
        .from(schema.rooms)
        .where(eq(schema.rooms.id, parentMessage.roomId));

      if (room.isPrivate) {
        const [membership] = await db
          .select()
          .from(schema.roomMembers)
          .where(
            and(
              eq(schema.roomMembers.roomId, room.id),
              eq(schema.roomMembers.userId, userId),
              eq(schema.roomMembers.isBanned, false)
            )
          );
        if (!membership) {
          return res.status(403).json({ error: 'Access denied' });
        }
      }

      const replies = await db
        .select({
          message: schema.messages,
          user: schema.users,
        })
        .from(schema.messages)
        .innerJoin(schema.users, eq(schema.users.id, schema.messages.userId))
        .where(
          and(
            eq(schema.messages.replyToId, messageId),
            or(
              isNull(schema.messages.scheduledFor),
              lte(schema.messages.scheduledFor, new Date())
            )
          )
        )
        .orderBy(asc(schema.messages.createdAt));

      res.json({ replies });
    } catch (err) {
      console.error('Get replies error:', err);
      res.status(500).json({ error: 'Failed to get replies' });
    }
  }
);

// Send message
app.post('/api/rooms/:roomId/messages', authMiddleware, async (req, res) => {
  const userId = (req as any).userId;
  const roomId = parseInt(req.params.roomId);
  const { content, replyToId, scheduledFor, expiresIn } = req.body;

  if (!content || typeof content !== 'string') {
    return res.status(400).json({ error: 'Content is required' });
  }

  if (content.length < 1 || content.length > 2000) {
    return res.status(400).json({ error: 'Content must be 1-2000 characters' });
  }

  try {
    const canProceed = await checkRateLimit(userId);
    if (!canProceed) {
      return res.status(429).json({ error: 'Rate limited. Please wait.' });
    }

    const [room] = await db
      .select()
      .from(schema.rooms)
      .where(eq(schema.rooms.id, roomId));
    if (!room) {
      return res.status(404).json({ error: 'Room not found' });
    }

    // Check membership for private rooms
    if (room.isPrivate) {
      const [membership] = await db
        .select()
        .from(schema.roomMembers)
        .where(
          and(
            eq(schema.roomMembers.roomId, roomId),
            eq(schema.roomMembers.userId, userId),
            eq(schema.roomMembers.isBanned, false)
          )
        );
      if (!membership) {
        return res.status(403).json({ error: 'Access denied' });
      }
    }

    // Calculate expiration using database time
    let expiresAt: Date | undefined;
    if (expiresIn && typeof expiresIn === 'number' && expiresIn > 0) {
      expiresAt = new Date(Date.now() + expiresIn * 1000);
    }

    let scheduledDate: Date | undefined;
    if (scheduledFor) {
      scheduledDate = new Date(scheduledFor);
      if (scheduledDate <= new Date()) {
        scheduledDate = undefined;
      }
    }

    const [message] = await db
      .insert(schema.messages)
      .values({
        roomId,
        userId,
        content: content.trim(),
        replyToId: replyToId || null,
        scheduledFor: scheduledDate || null,
        expiresAt: expiresAt || null,
      })
      .returning();

    const [user] = await db
      .select()
      .from(schema.users)
      .where(eq(schema.users.id, userId));

    // Clear typing indicator
    await db
      .delete(schema.typingIndicators)
      .where(
        and(
          eq(schema.typingIndicators.roomId, roomId),
          eq(schema.typingIndicators.userId, userId)
        )
      );
    io.to(`room:${roomId}`).emit('typing:stop', { roomId, userId });

    if (!scheduledDate) {
      io.to(`room:${roomId}`).emit('message:created', { message, user });

      if (replyToId) {
        io.to(`room:${roomId}`).emit('thread:reply', {
          parentId: replyToId,
          message,
          user,
        });
      }
    }

    res.json({ message, user });
  } catch (err) {
    console.error('Send message error:', err);
    res.status(500).json({ error: 'Failed to send message' });
  }
});

// Get my scheduled messages
app.get('/api/rooms/:roomId/scheduled', authMiddleware, async (req, res) => {
  const userId = (req as any).userId;
  const roomId = parseInt(req.params.roomId);

  try {
    const scheduled = await db
      .select()
      .from(schema.messages)
      .where(
        and(
          eq(schema.messages.roomId, roomId),
          eq(schema.messages.userId, userId),
          isNotNull(schema.messages.scheduledFor),
          sql`${schema.messages.scheduledFor} > NOW()`
        )
      )
      .orderBy(asc(schema.messages.scheduledFor));

    res.json({ scheduled });
  } catch (err) {
    console.error('Get scheduled error:', err);
    res.status(500).json({ error: 'Failed to get scheduled messages' });
  }
});

// Cancel scheduled message
app.delete(
  '/api/messages/:messageId/scheduled',
  authMiddleware,
  async (req, res) => {
    const userId = (req as any).userId;
    const messageId = parseInt(req.params.messageId);

    try {
      const [message] = await db
        .select()
        .from(schema.messages)
        .where(
          and(
            eq(schema.messages.id, messageId),
            eq(schema.messages.userId, userId),
            isNotNull(schema.messages.scheduledFor),
            sql`${schema.messages.scheduledFor} > NOW()`
          )
        );

      if (!message) {
        return res.status(404).json({ error: 'Scheduled message not found' });
      }

      await db.delete(schema.messages).where(eq(schema.messages.id, messageId));

      res.json({ success: true });
    } catch (err) {
      console.error('Cancel scheduled error:', err);
      res.status(500).json({ error: 'Failed to cancel scheduled message' });
    }
  }
);

// Edit message
app.patch('/api/messages/:messageId', authMiddleware, async (req, res) => {
  const userId = (req as any).userId;
  const messageId = parseInt(req.params.messageId);
  const { content } = req.body;

  if (!content || typeof content !== 'string') {
    return res.status(400).json({ error: 'Content is required' });
  }

  if (content.length < 1 || content.length > 2000) {
    return res.status(400).json({ error: 'Content must be 1-2000 characters' });
  }

  try {
    const [message] = await db
      .select()
      .from(schema.messages)
      .where(
        and(
          eq(schema.messages.id, messageId),
          eq(schema.messages.userId, userId)
        )
      );

    if (!message) {
      return res.status(404).json({ error: 'Message not found' });
    }

    // Save edit history
    await db
      .insert(schema.messageEdits)
      .values({ messageId, previousContent: message.content });

    const [updated] = await db
      .update(schema.messages)
      .set({ content: content.trim(), isEdited: true })
      .where(eq(schema.messages.id, messageId))
      .returning();

    const [user] = await db
      .select()
      .from(schema.users)
      .where(eq(schema.users.id, userId));

    io.to(`room:${message.roomId}`).emit('message:updated', {
      message: updated,
      user,
    });

    res.json({ message: updated });
  } catch (err) {
    console.error('Edit message error:', err);
    res.status(500).json({ error: 'Failed to edit message' });
  }
});

// Get message edit history
app.get(
  '/api/messages/:messageId/history',
  authMiddleware,
  async (req, res) => {
    const messageId = parseInt(req.params.messageId);

    try {
      const edits = await db
        .select()
        .from(schema.messageEdits)
        .where(eq(schema.messageEdits.messageId, messageId))
        .orderBy(desc(schema.messageEdits.editedAt));

      res.json({ edits });
    } catch (err) {
      console.error('Get history error:', err);
      res.status(500).json({ error: 'Failed to get edit history' });
    }
  }
);

// Toggle reaction
app.post(
  '/api/messages/:messageId/reactions',
  authMiddleware,
  async (req, res) => {
    const userId = (req as any).userId;
    const messageId = parseInt(req.params.messageId);
    const { emoji } = req.body;

    if (!emoji || typeof emoji !== 'string' || emoji.length > 10) {
      return res.status(400).json({ error: 'Invalid emoji' });
    }

    try {
      const [message] = await db
        .select()
        .from(schema.messages)
        .where(eq(schema.messages.id, messageId));

      if (!message) {
        return res.status(404).json({ error: 'Message not found' });
      }

      // Check if reaction exists
      const [existing] = await db
        .select()
        .from(schema.messageReactions)
        .where(
          and(
            eq(schema.messageReactions.messageId, messageId),
            eq(schema.messageReactions.userId, userId),
            eq(schema.messageReactions.emoji, emoji)
          )
        );

      if (existing) {
        // Remove reaction
        await db
          .delete(schema.messageReactions)
          .where(eq(schema.messageReactions.id, existing.id));

        io.to(`room:${message.roomId}`).emit('reaction:removed', {
          messageId,
          userId,
          emoji,
        });

        res.json({ action: 'removed' });
      } else {
        // Add reaction
        const [reaction] = await db
          .insert(schema.messageReactions)
          .values({ messageId, userId, emoji })
          .returning();

        const [user] = await db
          .select()
          .from(schema.users)
          .where(eq(schema.users.id, userId));

        io.to(`room:${message.roomId}`).emit('reaction:added', {
          messageId,
          userId,
          emoji,
          user,
        });

        res.json({ action: 'added', reaction });
      }
    } catch (err) {
      console.error('Toggle reaction error:', err);
      res.status(500).json({ error: 'Failed to toggle reaction' });
    }
  }
);

// Get reactions with users
app.get(
  '/api/messages/:messageId/reactions',
  authMiddleware,
  async (req, res) => {
    const messageId = parseInt(req.params.messageId);

    try {
      const reactions = await db
        .select({
          reaction: schema.messageReactions,
          user: schema.users,
        })
        .from(schema.messageReactions)
        .innerJoin(
          schema.users,
          eq(schema.users.id, schema.messageReactions.userId)
        )
        .where(eq(schema.messageReactions.messageId, messageId));

      res.json({ reactions });
    } catch (err) {
      console.error('Get reactions error:', err);
      res.status(500).json({ error: 'Failed to get reactions' });
    }
  }
);

// Mark messages as read
app.post('/api/rooms/:roomId/read', authMiddleware, async (req, res) => {
  const userId = (req as any).userId;
  const roomId = parseInt(req.params.roomId);
  const { messageIds } = req.body;

  if (!Array.isArray(messageIds) || messageIds.length === 0) {
    return res.status(400).json({ error: 'Message IDs required' });
  }

  try {
    // Update last read timestamp
    await db
      .update(schema.roomMembers)
      .set({ lastReadAt: new Date() })
      .where(
        and(
          eq(schema.roomMembers.roomId, roomId),
          eq(schema.roomMembers.userId, userId)
        )
      );

    // Insert read receipts
    for (const messageId of messageIds) {
      await db
        .insert(schema.readReceipts)
        .values({ messageId, userId })
        .onConflictDoNothing();
    }

    const [user] = await db
      .select()
      .from(schema.users)
      .where(eq(schema.users.id, userId));

    io.to(`room:${roomId}`).emit('messages:read', {
      roomId,
      userId,
      messageIds,
      user,
    });

    res.json({ success: true });
  } catch (err) {
    console.error('Mark read error:', err);
    res.status(500).json({ error: 'Failed to mark as read' });
  }
});

// Get unread counts
app.get('/api/rooms/unread', authMiddleware, async (req, res) => {
  const userId = (req as any).userId;

  try {
    const memberships = await db
      .select()
      .from(schema.roomMembers)
      .where(
        and(
          eq(schema.roomMembers.userId, userId),
          eq(schema.roomMembers.isBanned, false)
        )
      );

    const unreadCounts: Record<number, number> = {};

    for (const membership of memberships) {
      const lastReadAt = membership.lastReadAt || new Date(0);

      const [result] = await db
        .select({
          count: sql<number>`count(*)::int`,
        })
        .from(schema.messages)
        .where(
          and(
            eq(schema.messages.roomId, membership.roomId),
            sql`${schema.messages.createdAt} > ${lastReadAt}`,
            ne(schema.messages.userId, userId),
            or(
              isNull(schema.messages.scheduledFor),
              lte(schema.messages.scheduledFor, new Date())
            )
          )
        );

      unreadCounts[membership.roomId] = result?.count || 0;
    }

    res.json({ unreadCounts });
  } catch (err) {
    console.error('Get unread error:', err);
    res.status(500).json({ error: 'Failed to get unread counts' });
  }
});

// =====================
// Socket.io Handlers
// =====================

// Track connected users
const connectedUsers = new Map<string, Set<string>>(); // socketId -> userId

io.use((socket, next) => {
  const token = socket.handshake.auth.token;
  if (!token) {
    return next(new Error('Authentication required'));
  }

  const decoded = verifyToken(token);
  if (!decoded) {
    return next(new Error('Invalid token'));
  }

  (socket as any).userId = decoded.userId;
  next();
});

io.on('connection', async socket => {
  const userId = (socket as any).userId;

  // Track connection
  if (!connectedUsers.has(userId)) {
    connectedUsers.set(userId, new Set());
  }
  connectedUsers.get(userId)!.add(socket.id);

  // Join personal room
  socket.join(`user:${userId}`);

  // Update user status to online
  const [user] = await db
    .select()
    .from(schema.users)
    .where(eq(schema.users.id, userId));
  if (user && user.status !== 'invisible') {
    await db
      .update(schema.users)
      .set({ status: 'online', lastActiveAt: new Date() })
      .where(eq(schema.users.id, userId));

    io.emit('user:status', {
      userId,
      status: 'online',
      lastActiveAt: new Date(),
    });
  }

  // Join all member rooms
  const memberships = await db
    .select()
    .from(schema.roomMembers)
    .where(
      and(
        eq(schema.roomMembers.userId, userId),
        eq(schema.roomMembers.isBanned, false)
      )
    );

  for (const m of memberships) {
    socket.join(`room:${m.roomId}`);
  }

  // Handle room join
  socket.on('room:join', async (roomId: number) => {
    const [room] = await db
      .select()
      .from(schema.rooms)
      .where(eq(schema.rooms.id, roomId));
    if (!room) return;

    if (room.isPrivate) {
      const [membership] = await db
        .select()
        .from(schema.roomMembers)
        .where(
          and(
            eq(schema.roomMembers.roomId, roomId),
            eq(schema.roomMembers.userId, userId),
            eq(schema.roomMembers.isBanned, false)
          )
        );
      if (!membership) return;
    }

    socket.join(`room:${roomId}`);
  });

  // Handle room leave
  socket.on('room:leave', (roomId: number) => {
    socket.leave(`room:${roomId}`);
  });

  // Handle typing start
  socket.on('typing:start', async (roomId: number) => {
    await db
      .insert(schema.typingIndicators)
      .values({ roomId, userId })
      .onConflictDoUpdate({
        target: [
          schema.typingIndicators.roomId,
          schema.typingIndicators.userId,
        ],
        set: { startedAt: new Date() },
      });

    const [user] = await db
      .select()
      .from(schema.users)
      .where(eq(schema.users.id, userId));
    socket.to(`room:${roomId}`).emit('typing:start', { roomId, userId, user });
  });

  // Handle typing stop
  socket.on('typing:stop', async (roomId: number) => {
    await db
      .delete(schema.typingIndicators)
      .where(
        and(
          eq(schema.typingIndicators.roomId, roomId),
          eq(schema.typingIndicators.userId, userId)
        )
      );

    socket.to(`room:${roomId}`).emit('typing:stop', { roomId, userId });
  });

  // Handle activity (for auto-away)
  socket.on('activity', async () => {
    const [user] = await db
      .select()
      .from(schema.users)
      .where(eq(schema.users.id, userId));
    if (user && user.status === 'away') {
      await db
        .update(schema.users)
        .set({ status: 'online', lastActiveAt: new Date() })
        .where(eq(schema.users.id, userId));

      io.emit('user:status', {
        userId,
        status: 'online',
        lastActiveAt: new Date(),
      });
    } else if (user) {
      await db
        .update(schema.users)
        .set({ lastActiveAt: new Date() })
        .where(eq(schema.users.id, userId));
    }
  });

  // Handle disconnect
  socket.on('disconnect', async () => {
    const userSockets = connectedUsers.get(userId);
    if (userSockets) {
      userSockets.delete(socket.id);
      if (userSockets.size === 0) {
        connectedUsers.delete(userId);

        // User fully disconnected
        const [user] = await db
          .select()
          .from(schema.users)
          .where(eq(schema.users.id, userId));
        if (user && user.status !== 'invisible') {
          await db
            .update(schema.users)
            .set({ lastActiveAt: new Date() })
            .where(eq(schema.users.id, userId));

          io.emit('user:offline', { userId, lastActiveAt: new Date() });
        }

        // Clear typing indicators
        await db
          .delete(schema.typingIndicators)
          .where(eq(schema.typingIndicators.userId, userId));
      }
    }
  });
});

// =====================
// Background Jobs
// =====================

// Clean up expired typing indicators
cron.schedule('*/5 * * * * *', async () => {
  const cutoff = new Date(Date.now() - TYPING_TIMEOUT_SECONDS * 1000);

  const expired = await db
    .select()
    .from(schema.typingIndicators)
    .where(lt(schema.typingIndicators.startedAt, cutoff));

  for (const indicator of expired) {
    io.to(`room:${indicator.roomId}`).emit('typing:stop', {
      roomId: indicator.roomId,
      userId: indicator.userId,
    });
  }

  await db
    .delete(schema.typingIndicators)
    .where(lt(schema.typingIndicators.startedAt, cutoff));
});

// Process scheduled messages
cron.schedule('* * * * * *', async () => {
  const now = new Date();

  const scheduled = await db
    .select({
      message: schema.messages,
      user: schema.users,
    })
    .from(schema.messages)
    .innerJoin(schema.users, eq(schema.users.id, schema.messages.userId))
    .where(
      and(
        isNotNull(schema.messages.scheduledFor),
        lte(schema.messages.scheduledFor, now)
      )
    );

  for (const { message, user } of scheduled) {
    await db
      .update(schema.messages)
      .set({ scheduledFor: null })
      .where(eq(schema.messages.id, message.id));

    io.to(`room:${message.roomId}`).emit('message:created', {
      message: { ...message, scheduledFor: null },
      user,
    });

    if (message.replyToId) {
      io.to(`room:${message.roomId}`).emit('thread:reply', {
        parentId: message.replyToId,
        message: { ...message, scheduledFor: null },
        user,
      });
    }
  }
});

// Delete expired messages
cron.schedule('* * * * * *', async () => {
  const now = new Date();

  const expired = await db
    .select()
    .from(schema.messages)
    .where(
      and(
        isNotNull(schema.messages.expiresAt),
        lte(schema.messages.expiresAt, now)
      )
    );

  for (const message of expired) {
    await db.delete(schema.messages).where(eq(schema.messages.id, message.id));

    io.to(`room:${message.roomId}`).emit('message:deleted', {
      messageId: message.id,
      roomId: message.roomId,
    });
  }
});

// Auto-away for inactive users
cron.schedule('*/30 * * * * *', async () => {
  const cutoff = new Date(Date.now() - AUTO_AWAY_MS);

  const users = await db
    .select()
    .from(schema.users)
    .where(
      and(
        eq(schema.users.status, 'online'),
        lt(schema.users.lastActiveAt, cutoff)
      )
    );

  for (const user of users) {
    // Only set away if user is still connected
    if (connectedUsers.has(user.id)) {
      await db
        .update(schema.users)
        .set({ status: 'away' })
        .where(eq(schema.users.id, user.id));

      io.emit('user:status', {
        userId: user.id,
        status: 'away',
        lastActiveAt: user.lastActiveAt,
      });
    }
  }
});

// =====================
// Start Server
// =====================

httpServer.listen(PORT, () => {
  console.log(`Server running on port ${PORT}`);
});

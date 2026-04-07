import express from 'express';
import { createServer } from 'http';
import { Server } from 'socket.io';
import { drizzle } from 'drizzle-orm/node-postgres';
import { Pool } from 'pg';
import * as schema from './schema.js';
import { eq, and, inArray, lte, gt, isNotNull, isNull, or, count as drizzleCount } from 'drizzle-orm';
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

// Activity tracking: roomId -> array of recent message timestamps (ms)
const roomActivity = new Map<number, number[]>();
const ACTIVITY_WINDOW_MS = 5 * 60 * 1000; // 5 minutes
const HOT_THRESHOLD = 5; // 5+ messages in window = hot

function computeActivityLevel(roomId: number): 'hot' | 'active' | null {
  const now = Date.now();
  const recent = (roomActivity.get(roomId) ?? []).filter((t) => now - t < ACTIVITY_WINDOW_MS);
  roomActivity.set(roomId, recent);
  if (recent.length >= HOT_THRESHOLD) return 'hot';
  if (recent.length >= 1) return 'active';
  return null;
}

function recordRoomMessage(roomId: number) {
  const timestamps = roomActivity.get(roomId) ?? [];
  timestamps.push(Date.now());
  roomActivity.set(roomId, timestamps);
  const level = computeActivityLevel(roomId);
  io.emit('room_activity_update', { roomId, level });
}

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

// Get online users (excludes invisible)
app.get('/api/users/online', async (_req, res) => {
  try {
    const users = await db.select().from(schema.users).where(eq(schema.users.online, true));
    // Filter out invisible users from the public online list
    res.json(users.filter((u) => u.status !== 'invisible'));
  } catch {
    res.status(500).json({ error: 'Failed to get online users' });
  }
});

// Get all users for presence list
app.get('/api/users', async (_req, res) => {
  try {
    const users = await db
      .select({
        id: schema.users.id,
        name: schema.users.name,
        online: schema.users.online,
        status: schema.users.status,
        lastSeen: schema.users.lastSeen,
      })
      .from(schema.users)
      .orderBy(schema.users.name);
    res.json(users);
  } catch {
    res.status(500).json({ error: 'Failed to get users' });
  }
});

// Update user status via REST
app.patch('/api/users/:id/status', async (req, res) => {
  const userId = parseInt(req.params.id);
  const { status } = req.body as { status?: string };
  const VALID_STATUSES = ['online', 'away', 'dnd', 'invisible'];
  if (!status || !VALID_STATUSES.includes(status)) {
    return res.status(400).json({ error: 'Invalid status' });
  }

  try {
    const now = new Date();
    const [user] = await db
      .update(schema.users)
      .set({ status, lastSeen: now })
      .where(eq(schema.users.id, userId))
      .returning();

    if (!user) return res.status(404).json({ error: 'User not found' });

    // Broadcast: invisible users appear offline to others
    const broadcastStatus = status === 'invisible' ? 'offline' : status;
    io.emit('user_status', {
      userId,
      name: user.name,
      online: broadcastStatus !== 'offline',
      status: broadcastStatus,
      lastSeen: now,
    });

    res.json({ ok: true, status });
  } catch {
    res.status(500).json({ error: 'Failed to update status' });
  }
});

// List rooms with unread counts (only public rooms + private rooms user is a member of)
app.get('/api/rooms', async (req, res) => {
  const userId = parseInt(req.query.userId as string);
  if (!userId) return res.status(400).json({ error: 'userId required' });

  try {
    const memberships = await db
      .select()
      .from(schema.roomMembers)
      .where(eq(schema.roomMembers.userId, userId));
    const joinedRoomIds = memberships.map((m) => m.roomId);
    const joinedRooms = new Set(joinedRoomIds);

    // Fetch only rooms the user is allowed to see: public OR member of private
    const rooms = joinedRoomIds.length > 0
      ? await db.select().from(schema.rooms)
          .where(or(eq(schema.rooms.isPrivate, false), inArray(schema.rooms.id, joinedRoomIds)))
          .orderBy(schema.rooms.name)
      : await db.select().from(schema.rooms)
          .where(eq(schema.rooms.isPrivate, false))
          .orderBy(schema.rooms.name);

    // For DM rooms, get partner name
    const dmPartnerNames: Record<number, string> = {};
    const dmRoomIds = rooms.filter(r => r.isDm).map(r => r.id);
    if (dmRoomIds.length > 0) {
      const allDmMembers = await db
        .select({ roomId: schema.roomMembers.roomId, userId: schema.roomMembers.userId, name: schema.users.name })
        .from(schema.roomMembers)
        .innerJoin(schema.users, eq(schema.roomMembers.userId, schema.users.id))
        .where(inArray(schema.roomMembers.roomId, dmRoomIds));
      for (const m of allDmMembers) {
        if (m.userId !== userId) dmPartnerNames[m.roomId] = m.name;
      }
    }

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
          dmPartnerName: dmPartnerNames[room.id] ?? null,
        };
      })
    );

    res.json(roomsWithCounts);
  } catch (e) {
    console.error(e);
    res.status(500).json({ error: 'Failed to get rooms' });
  }
});

// Get activity levels for all rooms
app.get('/api/rooms/activity', (_req, res) => {
  const now = Date.now();
  const result: Record<number, 'hot' | 'active'> = {};
  roomActivity.forEach((timestamps, roomId) => {
    const recent = timestamps.filter((t) => now - t < ACTIVITY_WINDOW_MS);
    roomActivity.set(roomId, recent);
    if (recent.length >= HOT_THRESHOLD) result[roomId] = 'hot';
    else if (recent.length >= 1) result[roomId] = 'active';
  });
  res.json(result);
});

// Create room
app.post('/api/rooms', async (req, res) => {
  const { name, userId, isPrivate } = req.body as { name?: string; userId?: number; isPrivate?: boolean };
  if (!name || name.trim().length === 0) {
    return res.status(400).json({ error: 'Room name required' });
  }
  if (name.trim().length > 50) {
    return res.status(400).json({ error: 'Room name must be 50 characters or fewer' });
  }

  try {
    const [room] = await db
      .insert(schema.rooms)
      .values({ name: name.trim(), isPrivate: isPrivate ?? false })
      .returning();

    if (userId) {
      // Creator becomes admin
      await db
        .insert(schema.roomMembers)
        .values({ userId, roomId: room.id, isAdmin: true })
        .onConflictDoNothing();
    }

    const roomWithMeta = { ...room, unreadCount: 0, joined: userId ? true : false, dmPartnerName: null };
    if (!room.isPrivate) {
      // Only broadcast public rooms to all
      io.emit('room_created', roomWithMeta);
    } else if (userId) {
      // Private room: only notify creator
      const creatorSocketId = userSockets.get(userId);
      if (creatorSocketId) {
        io.to(creatorSocketId).emit('room_created', roomWithMeta);
      }
    }
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
    // Check if user is banned
    const [banned] = await db
      .select()
      .from(schema.bannedUsers)
      .where(and(eq(schema.bannedUsers.userId, userId), eq(schema.bannedUsers.roomId, roomId)));
    if (banned) return res.status(403).json({ error: 'You are banned from this room' });

    await db
      .insert(schema.roomMembers)
      .values({ userId, roomId })
      .onConflictDoNothing();

    const [user] = await db.select().from(schema.users).where(eq(schema.users.id, userId));
    if (user) {
      io.to(`room:${roomId}`).emit('member_joined', { userId, name: user.name, isAdmin: false, roomId });
    }

    res.json({ ok: true });
  } catch {
    res.status(500).json({ error: 'Failed to join room' });
  }
});

// Get room members
app.get('/api/rooms/:id/members', async (req, res) => {
  const roomId = parseInt(req.params.id);
  try {
    const members = await db
      .select({
        userId: schema.roomMembers.userId,
        isAdmin: schema.roomMembers.isAdmin,
        name: schema.users.name,
      })
      .from(schema.roomMembers)
      .innerJoin(schema.users, eq(schema.roomMembers.userId, schema.users.id))
      .where(eq(schema.roomMembers.roomId, roomId));
    res.json(members);
  } catch (e) {
    console.error(e);
    res.status(500).json({ error: 'Failed to get members' });
  }
});

// Kick user from room (admin only)
app.post('/api/rooms/:id/kick', async (req, res) => {
  const roomId = parseInt(req.params.id);
  const { adminId, targetUserId } = req.body as { adminId: number; targetUserId: number };

  try {
    // Verify requester is admin
    const [adminMember] = await db
      .select()
      .from(schema.roomMembers)
      .where(and(eq(schema.roomMembers.userId, adminId), eq(schema.roomMembers.roomId, roomId)));
    if (!adminMember?.isAdmin) return res.status(403).json({ error: 'Only admins can kick users' });

    // Cannot kick another admin
    const [targetMember] = await db
      .select()
      .from(schema.roomMembers)
      .where(and(eq(schema.roomMembers.userId, targetUserId), eq(schema.roomMembers.roomId, roomId)));
    if (targetMember?.isAdmin) return res.status(403).json({ error: 'Cannot kick an admin' });

    // Remove from room and ban
    await db
      .delete(schema.roomMembers)
      .where(and(eq(schema.roomMembers.userId, targetUserId), eq(schema.roomMembers.roomId, roomId)));
    await db
      .insert(schema.bannedUsers)
      .values({ userId: targetUserId, roomId })
      .onConflictDoNothing();

    // Notify the room (for other members to update their panel)
    io.to(`room:${roomId}`).emit('user_kicked', { userId: targetUserId, roomId });

    // Emit directly to the kicked user's socket so they are redirected
    // even if their socket is not (yet) in the socket room
    const kickedSocketId = userSockets.get(targetUserId);
    if (kickedSocketId) {
      const kickedSocket = io.sockets.sockets.get(kickedSocketId);
      if (kickedSocket) {
        kickedSocket.emit('kicked_from_room', { roomId });
        kickedSocket.leave(`room:${roomId}`);
      }
    }

    res.json({ ok: true });
  } catch (e) {
    console.error(e);
    res.status(500).json({ error: 'Failed to kick user' });
  }
});

// Promote user to admin (admin only)
app.post('/api/rooms/:id/promote', async (req, res) => {
  const roomId = parseInt(req.params.id);
  const { adminId, targetUserId } = req.body as { adminId: number; targetUserId: number };

  try {
    // Verify requester is admin
    const [adminMember] = await db
      .select()
      .from(schema.roomMembers)
      .where(and(eq(schema.roomMembers.userId, adminId), eq(schema.roomMembers.roomId, roomId)));
    if (!adminMember?.isAdmin) return res.status(403).json({ error: 'Only admins can promote users' });

    // Promote target
    await db
      .update(schema.roomMembers)
      .set({ isAdmin: true })
      .where(and(eq(schema.roomMembers.userId, targetUserId), eq(schema.roomMembers.roomId, roomId)));

    io.to(`room:${roomId}`).emit('user_promoted', { userId: targetUserId, roomId });

    res.json({ ok: true });
  } catch (e) {
    console.error(e);
    res.status(500).json({ error: 'Failed to promote user' });
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

    io.to(`room:${roomId}`).emit('member_left', { userId, roomId });

    res.json({ ok: true });
  } catch {
    res.status(500).json({ error: 'Failed to leave room' });
  }
});

// Invite a user to a private room (admin only)
app.post('/api/rooms/:id/invite', async (req, res) => {
  const roomId = parseInt(req.params.id);
  const { adminId, inviteeName } = req.body as { adminId: number; inviteeName: string };

  if (!inviteeName?.trim()) return res.status(400).json({ error: 'inviteeName required' });

  try {
    // Verify requester is admin
    const [adminMember] = await db
      .select()
      .from(schema.roomMembers)
      .where(and(eq(schema.roomMembers.userId, adminId), eq(schema.roomMembers.roomId, roomId)));
    if (!adminMember?.isAdmin) return res.status(403).json({ error: 'Only admins can invite users' });

    // Find invitee by name
    const [invitee] = await db
      .select()
      .from(schema.users)
      .where(eq(schema.users.name, inviteeName.trim()));
    if (!invitee) return res.status(404).json({ error: 'User not found' });

    // Check if already a member
    const [existing] = await db
      .select()
      .from(schema.roomMembers)
      .where(and(eq(schema.roomMembers.userId, invitee.id), eq(schema.roomMembers.roomId, roomId)));
    if (existing) return res.status(400).json({ error: 'User is already a member' });

    // Check if already banned
    const [banned] = await db
      .select()
      .from(schema.bannedUsers)
      .where(and(eq(schema.bannedUsers.userId, invitee.id), eq(schema.bannedUsers.roomId, roomId)));
    if (banned) return res.status(400).json({ error: 'User is banned from this room' });

    // Check if pending invitation already exists
    const [pendingInv] = await db
      .select()
      .from(schema.roomInvitations)
      .where(and(
        eq(schema.roomInvitations.roomId, roomId),
        eq(schema.roomInvitations.inviteeId, invitee.id),
        eq(schema.roomInvitations.status, 'pending')
      ));
    if (pendingInv) return res.status(400).json({ error: 'Invitation already pending' });

    const [room] = await db.select().from(schema.rooms).where(eq(schema.rooms.id, roomId));
    const [inviter] = await db.select().from(schema.users).where(eq(schema.users.id, adminId));

    const [invitation] = await db
      .insert(schema.roomInvitations)
      .values({ roomId, inviterId: adminId, inviteeId: invitee.id })
      .returning();

    // Notify invitee via socket
    const inviteeSocketId = userSockets.get(invitee.id);
    if (inviteeSocketId) {
      io.to(inviteeSocketId).emit('invitation_received', {
        id: invitation.id,
        roomId,
        roomName: room?.name ?? '',
        inviterName: inviter?.name ?? '',
        createdAt: invitation.createdAt,
      });
    }

    res.json({ ok: true, invitation });
  } catch (e) {
    console.error(e);
    res.status(500).json({ error: 'Failed to invite user' });
  }
});

// Get pending invitations for a user
app.get('/api/invitations', async (req, res) => {
  const userId = parseInt(req.query.userId as string);
  if (!userId) return res.status(400).json({ error: 'userId required' });

  try {
    const invitations = await pool.query<{
      id: number; room_id: number; room_name: string;
      inviter_id: number; inviter_name: string; created_at: Date;
    }>(
      `SELECT ri.id, ri.room_id, r.name as room_name, ri.inviter_id, u.name as inviter_name, ri.created_at
       FROM room_invitations ri
       JOIN rooms r ON r.id = ri.room_id
       JOIN users u ON u.id = ri.inviter_id
       WHERE ri.invitee_id = $1 AND ri.status = 'pending'
       ORDER BY ri.created_at DESC`,
      [userId]
    );
    res.json(invitations.rows.map(r => ({
      id: r.id, roomId: r.room_id, roomName: r.room_name,
      inviterId: r.inviter_id, inviterName: r.inviter_name, createdAt: r.created_at,
    })));
  } catch (e) {
    console.error(e);
    res.status(500).json({ error: 'Failed to get invitations' });
  }
});

// Accept an invitation
app.post('/api/invitations/:id/accept', async (req, res) => {
  const invitationId = parseInt(req.params.id);
  const { userId } = req.body as { userId: number };

  try {
    const [invitation] = await db
      .update(schema.roomInvitations)
      .set({ status: 'accepted' })
      .where(and(
        eq(schema.roomInvitations.id, invitationId),
        eq(schema.roomInvitations.inviteeId, userId),
        eq(schema.roomInvitations.status, 'pending')
      ))
      .returning();

    if (!invitation) return res.status(404).json({ error: 'Invitation not found' });

    // Add user to room
    await db
      .insert(schema.roomMembers)
      .values({ userId, roomId: invitation.roomId })
      .onConflictDoNothing();

    const [room] = await db.select().from(schema.rooms).where(eq(schema.rooms.id, invitation.roomId));
    const [user] = await db.select().from(schema.users).where(eq(schema.users.id, userId));

    // Notify room members
    io.to(`room:${invitation.roomId}`).emit('member_joined', {
      userId, name: user?.name ?? '', isAdmin: false, roomId: invitation.roomId,
    });

    // Get DM partner name if DM
    let dmPartnerName: string | null = null;
    if (room?.isDm) {
      const members = await db
        .select({ userId: schema.roomMembers.userId, name: schema.users.name })
        .from(schema.roomMembers)
        .innerJoin(schema.users, eq(schema.roomMembers.userId, schema.users.id))
        .where(eq(schema.roomMembers.roomId, invitation.roomId));
      const partner = members.find(m => m.userId !== userId);
      dmPartnerName = partner?.name ?? null;
    }

    // Send room info to the new member
    const roomWithMeta = {
      ...room,
      unreadCount: 0,
      joined: true,
      dmPartnerName,
    };
    const userSocketId = userSockets.get(userId);
    if (userSocketId) {
      io.to(userSocketId).emit('room_created', roomWithMeta);
    }

    res.json({ ok: true, room: roomWithMeta });
  } catch (e) {
    console.error(e);
    res.status(500).json({ error: 'Failed to accept invitation' });
  }
});

// Decline an invitation
app.post('/api/invitations/:id/decline', async (req, res) => {
  const invitationId = parseInt(req.params.id);
  const { userId } = req.body as { userId: number };

  try {
    const [invitation] = await db
      .update(schema.roomInvitations)
      .set({ status: 'declined' })
      .where(and(
        eq(schema.roomInvitations.id, invitationId),
        eq(schema.roomInvitations.inviteeId, userId),
        eq(schema.roomInvitations.status, 'pending')
      ))
      .returning();

    if (!invitation) return res.status(404).json({ error: 'Invitation not found' });
    res.json({ ok: true });
  } catch (e) {
    console.error(e);
    res.status(500).json({ error: 'Failed to decline invitation' });
  }
});

// Create or get DM room between two users
app.post('/api/dm', async (req, res) => {
  const { userId, partnerId } = req.body as { userId: number; partnerId: number };
  if (!userId || !partnerId || userId === partnerId) {
    return res.status(400).json({ error: 'userId and partnerId required and must be different' });
  }

  try {
    const [partner] = await db.select().from(schema.users).where(eq(schema.users.id, partnerId));
    if (!partner) return res.status(404).json({ error: 'Partner not found' });

    // Check if DM room already exists between these two users
    const existing = await pool.query<{ id: number }>(
      `SELECT r.id FROM rooms r
       WHERE r.is_dm = true
       AND (SELECT COUNT(*) FROM room_members rm WHERE rm.room_id = r.id AND rm.user_id IN ($1, $2)) = 2
       AND (SELECT COUNT(*) FROM room_members rm WHERE rm.room_id = r.id) = 2
       LIMIT 1`,
      [userId, partnerId]
    );

    if (existing.rows.length > 0) {
      const roomId = existing.rows[0].id;
      const [room] = await db.select().from(schema.rooms).where(eq(schema.rooms.id, roomId));
      const roomWithMeta = { ...room, unreadCount: 0, joined: true, dmPartnerName: partner.name };

      // Ensure both users are still members
      await db.insert(schema.roomMembers).values({ userId, roomId }).onConflictDoNothing();
      await db.insert(schema.roomMembers).values({ userId: partnerId, roomId }).onConflictDoNothing();

      return res.json(roomWithMeta);
    }

    // Create DM room
    const dmName = `dm:${Math.min(userId, partnerId)}-${Math.max(userId, partnerId)}`;
    const [room] = await db
      .insert(schema.rooms)
      .values({ name: dmName, isPrivate: true, isDm: true })
      .returning();

    await db.insert(schema.roomMembers).values({ userId, roomId: room.id }).onConflictDoNothing();
    await db.insert(schema.roomMembers).values({ userId: partnerId, roomId: room.id }).onConflictDoNothing();

    const [user] = await db.select().from(schema.users).where(eq(schema.users.id, userId));

    const roomForUser = { ...room, unreadCount: 0, joined: true, dmPartnerName: partner.name };
    const roomForPartner = { ...room, unreadCount: 0, joined: true, dmPartnerName: user?.name ?? '' };

    // Notify requester
    const userSocketId = userSockets.get(userId);
    if (userSocketId) io.to(userSocketId).emit('room_created', roomForUser);

    // Notify partner
    const partnerSocketId = userSockets.get(partnerId);
    if (partnerSocketId) io.to(partnerSocketId).emit('room_created', roomForPartner);

    res.json(roomForUser);
  } catch (e: unknown) {
    if ((e as { code?: string }).code === '23505') {
      // Race condition: DM already exists, retry lookup
      const existing = await pool.query<{ id: number }>(
        `SELECT r.id FROM rooms r
         WHERE r.is_dm = true
         AND (SELECT COUNT(*) FROM room_members rm WHERE rm.room_id = r.id AND rm.user_id IN ($1, $2)) = 2
         LIMIT 1`,
        [userId, partnerId]
      );
      if (existing.rows.length > 0) {
        const [room] = await db.select().from(schema.rooms).where(eq(schema.rooms.id, existing.rows[0].id));
        const [partner] = await db.select().from(schema.users).where(eq(schema.users.id, partnerId));
        return res.json({ ...room, unreadCount: 0, joined: true, dmPartnerName: partner?.name ?? '' });
      }
    }
    console.error(e);
    res.status(500).json({ error: 'Failed to create DM' });
  }
});

// Get messages for a room (marks all as read for userId)
app.get('/api/rooms/:id/messages', async (req, res) => {
  const roomId = parseInt(req.params.id);
  const userId = parseInt(req.query.userId as string);

  try {
    // Verify user is a member and not banned
    if (userId) {
      const [banned] = await db
        .select()
        .from(schema.bannedUsers)
        .where(and(eq(schema.bannedUsers.userId, userId), eq(schema.bannedUsers.roomId, roomId)));
      if (banned) return res.status(403).json({ error: 'You are banned from this room' });

      const [membership] = await db
        .select()
        .from(schema.roomMembers)
        .where(and(eq(schema.roomMembers.userId, userId), eq(schema.roomMembers.roomId, roomId)));
      if (!membership) return res.status(403).json({ error: 'You are not a member of this room' });
    }

    const msgs = await db
      .select({
        id: schema.messages.id,
        roomId: schema.messages.roomId,
        userId: schema.messages.userId,
        content: schema.messages.content,
        expiresAt: schema.messages.expiresAt,
        editedAt: schema.messages.editedAt,
        parentMessageId: schema.messages.parentMessageId,
        createdAt: schema.messages.createdAt,
        userName: schema.users.name,
      })
      .from(schema.messages)
      .innerJoin(schema.users, eq(schema.messages.userId, schema.users.id))
      .where(
        and(
          eq(schema.messages.roomId, roomId),
          isNull(schema.messages.parentMessageId),
          or(isNull(schema.messages.expiresAt), gt(schema.messages.expiresAt, new Date()))
        )
      )
      .orderBy(schema.messages.createdAt)
      .limit(200);

    // Get reply counts for top-level messages
    let replyCountByMessage: Record<number, number> = {};
    if (msgs.length > 0) {
      const replyCounts = await pool.query<{ parent_message_id: number; count: string }>(
        `SELECT parent_message_id, COUNT(*)::int as count FROM messages WHERE room_id = $1 AND parent_message_id IS NOT NULL GROUP BY parent_message_id`,
        [roomId]
      );
      for (const row of replyCounts.rows) {
        replyCountByMessage[row.parent_message_id] = parseInt(row.count);
      }
    }

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

    // Get reactions for these messages
    let reactionsByMessage: Record<number, { emoji: string; userId: number; userName: string }[]> = {};
    if (msgs.length > 0) {
      const reactions = await db
        .select({
          messageId: schema.messageReactions.messageId,
          userId: schema.messageReactions.userId,
          emoji: schema.messageReactions.emoji,
          userName: schema.users.name,
        })
        .from(schema.messageReactions)
        .innerJoin(schema.users, eq(schema.messageReactions.userId, schema.users.id))
        .where(inArray(schema.messageReactions.messageId, msgs.map((m) => m.id)));

      for (const r of reactions) {
        if (!reactionsByMessage[r.messageId]) reactionsByMessage[r.messageId] = [];
        reactionsByMessage[r.messageId].push({ emoji: r.emoji, userId: r.userId, userName: r.userName });
      }
    }

    const result = msgs.map((m) => ({
      ...m,
      readBy: (receiptsByMessage[m.id] ?? []).filter((r) => r.userId !== m.userId),
      reactions: reactionsByMessage[m.id] ?? [],
      replyCount: replyCountByMessage[m.id] ?? 0,
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

// ─── Message Threading ────────────────────────────────────────────────────────

// Get thread (parent message + all replies)
app.get('/api/messages/:id/thread', async (req, res) => {
  const parentMessageId = parseInt(req.params.id);
  const userId = parseInt(req.query.userId as string);

  try {
    // Get parent message
    const [parentRaw] = await db
      .select({
        id: schema.messages.id,
        roomId: schema.messages.roomId,
        userId: schema.messages.userId,
        content: schema.messages.content,
        expiresAt: schema.messages.expiresAt,
        editedAt: schema.messages.editedAt,
        parentMessageId: schema.messages.parentMessageId,
        createdAt: schema.messages.createdAt,
        userName: schema.users.name,
      })
      .from(schema.messages)
      .innerJoin(schema.users, eq(schema.messages.userId, schema.users.id))
      .where(eq(schema.messages.id, parentMessageId));

    if (!parentRaw) return res.status(404).json({ error: 'Message not found' });

    // Get replies
    const replies = await db
      .select({
        id: schema.messages.id,
        roomId: schema.messages.roomId,
        userId: schema.messages.userId,
        content: schema.messages.content,
        expiresAt: schema.messages.expiresAt,
        editedAt: schema.messages.editedAt,
        parentMessageId: schema.messages.parentMessageId,
        createdAt: schema.messages.createdAt,
        userName: schema.users.name,
      })
      .from(schema.messages)
      .innerJoin(schema.users, eq(schema.messages.userId, schema.users.id))
      .where(eq(schema.messages.parentMessageId, parentMessageId))
      .orderBy(schema.messages.createdAt)
      .limit(200);

    // Get reactions for replies
    let reactionsByMessage: Record<number, { emoji: string; userId: number; userName: string }[]> = {};
    if (replies.length > 0) {
      const reactions = await db
        .select({
          messageId: schema.messageReactions.messageId,
          userId: schema.messageReactions.userId,
          emoji: schema.messageReactions.emoji,
          userName: schema.users.name,
        })
        .from(schema.messageReactions)
        .innerJoin(schema.users, eq(schema.messageReactions.userId, schema.users.id))
        .where(inArray(schema.messageReactions.messageId, replies.map((r) => r.id)));
      for (const r of reactions) {
        if (!reactionsByMessage[r.messageId]) reactionsByMessage[r.messageId] = [];
        reactionsByMessage[r.messageId].push({ emoji: r.emoji, userId: r.userId, userName: r.userName });
      }
    }

    const replyCount = replies.length;
    const parent = { ...parentRaw, readBy: [], reactions: [], replyCount };
    const replyMessages = replies.map((r) => ({
      ...r,
      readBy: [],
      reactions: reactionsByMessage[r.id] ?? [],
      replyCount: 0,
    }));

    // Mark replies as read for the requesting user
    if (userId && replies.length > 0) {
      for (const reply of replies) {
        await db
          .insert(schema.readReceipts)
          .values({ userId, messageId: reply.id })
          .onConflictDoNothing();
      }
    }

    res.json({ parent, replies: replyMessages });
  } catch (e) {
    console.error(e);
    res.status(500).json({ error: 'Failed to get thread' });
  }
});

// ─── Message Editing ──────────────────────────────────────────────────────────

// Edit a message (owner only)
app.patch('/api/messages/:id', async (req, res) => {
  const messageId = parseInt(req.params.id);
  const { userId, content } = req.body as { userId?: number; content?: string };

  if (!userId || !content?.trim()) {
    return res.status(400).json({ error: 'userId and content required' });
  }
  if (content.trim().length > 2000) {
    return res.status(400).json({ error: 'Content too long' });
  }

  try {
    const [message] = await db.select().from(schema.messages).where(eq(schema.messages.id, messageId));
    if (!message) return res.status(404).json({ error: 'Message not found' });
    if (message.userId !== userId) return res.status(403).json({ error: 'Cannot edit another user\'s message' });

    // Save current content to edit history
    await db.insert(schema.messageEdits).values({ messageId, content: message.content });

    // Update message
    const [updated] = await db
      .update(schema.messages)
      .set({ content: content.trim(), editedAt: new Date() })
      .where(eq(schema.messages.id, messageId))
      .returning();

    const [user] = await db.select().from(schema.users).where(eq(schema.users.id, userId));

    const payload = {
      messageId,
      content: updated.content,
      editedAt: updated.editedAt,
      userName: user?.name ?? '',
    };
    io.to(`room:${message.roomId}`).emit('message_edited', payload);

    res.json({ ok: true, ...payload });
  } catch (e) {
    console.error(e);
    res.status(500).json({ error: 'Failed to edit message' });
  }
});

// Get edit history for a message
app.get('/api/messages/:id/history', async (req, res) => {
  const messageId = parseInt(req.params.id);

  try {
    const history = await db
      .select()
      .from(schema.messageEdits)
      .where(eq(schema.messageEdits.messageId, messageId))
      .orderBy(schema.messageEdits.editedAt);
    res.json(history);
  } catch (e) {
    console.error(e);
    res.status(500).json({ error: 'Failed to get edit history' });
  }
});

// ─── Reactions ────────────────────────────────────────────────────────────────

// Toggle a reaction (add if not present, remove if already present)
app.post('/api/messages/:id/reactions', async (req, res) => {
  const messageId = parseInt(req.params.id);
  const { userId, emoji } = req.body as { userId?: number; emoji?: string };

  const ALLOWED_EMOJIS = ['👍', '❤️', '😂', '😮', '😢'];
  if (!userId || !emoji || !ALLOWED_EMOJIS.includes(emoji)) {
    return res.status(400).json({ error: 'userId and valid emoji required' });
  }

  try {
    const [message] = await db.select().from(schema.messages).where(eq(schema.messages.id, messageId));
    if (!message) return res.status(404).json({ error: 'Message not found' });

    const [user] = await db.select().from(schema.users).where(eq(schema.users.id, userId));
    if (!user) return res.status(404).json({ error: 'User not found' });

    // Check if reaction already exists
    const [existing] = await db
      .select()
      .from(schema.messageReactions)
      .where(
        and(
          eq(schema.messageReactions.userId, userId),
          eq(schema.messageReactions.messageId, messageId),
          eq(schema.messageReactions.emoji, emoji)
        )
      );

    let added: boolean;
    if (existing) {
      // Remove reaction (toggle off)
      await db
        .delete(schema.messageReactions)
        .where(
          and(
            eq(schema.messageReactions.userId, userId),
            eq(schema.messageReactions.messageId, messageId),
            eq(schema.messageReactions.emoji, emoji)
          )
        );
      added = false;
    } else {
      // Add reaction
      await db
        .insert(schema.messageReactions)
        .values({ userId, messageId, emoji })
        .onConflictDoNothing();
      added = true;
    }

    // Broadcast to the room
    const payload = { messageId, userId, userName: user.name, emoji, added };
    io.to(`room:${message.roomId}`).emit('reaction_updated', payload);

    res.json({ ok: true, added });
  } catch (e) {
    console.error(e);
    res.status(500).json({ error: 'Failed to toggle reaction' });
  }
});

// ─── Scheduled Messages ────────────────────────────────────────────────────────

// Create a scheduled message
app.post('/api/scheduled-messages', async (req, res) => {
  const { roomId, userId, content, scheduledFor } = req.body as {
    roomId?: number;
    userId?: number;
    content?: string;
    scheduledFor?: string;
  };

  if (!roomId || !userId || !content?.trim() || !scheduledFor) {
    return res.status(400).json({ error: 'roomId, userId, content, and scheduledFor are required' });
  }

  const scheduleDate = new Date(scheduledFor);
  if (isNaN(scheduleDate.getTime()) || scheduleDate <= new Date()) {
    return res.status(400).json({ error: 'scheduledFor must be a future date' });
  }

  if (content.trim().length > 2000) {
    return res.status(400).json({ error: 'Content too long' });
  }

  try {
    const [scheduled] = await db
      .insert(schema.scheduledMessages)
      .values({ roomId, userId, content: content.trim(), scheduledFor: scheduleDate })
      .returning();
    res.json(scheduled);
  } catch (e) {
    console.error(e);
    res.status(500).json({ error: 'Failed to schedule message' });
  }
});

// Get pending scheduled messages for a user
app.get('/api/scheduled-messages', async (req, res) => {
  const userId = parseInt(req.query.userId as string);
  if (!userId) return res.status(400).json({ error: 'userId required' });

  try {
    const scheduled = await db
      .select({
        id: schema.scheduledMessages.id,
        roomId: schema.scheduledMessages.roomId,
        userId: schema.scheduledMessages.userId,
        content: schema.scheduledMessages.content,
        scheduledFor: schema.scheduledMessages.scheduledFor,
        createdAt: schema.scheduledMessages.createdAt,
        roomName: schema.rooms.name,
      })
      .from(schema.scheduledMessages)
      .innerJoin(schema.rooms, eq(schema.scheduledMessages.roomId, schema.rooms.id))
      .where(
        and(
          eq(schema.scheduledMessages.userId, userId),
          eq(schema.scheduledMessages.sent, false),
          eq(schema.scheduledMessages.cancelled, false)
        )
      )
      .orderBy(schema.scheduledMessages.scheduledFor);
    res.json(scheduled);
  } catch (e) {
    console.error(e);
    res.status(500).json({ error: 'Failed to get scheduled messages' });
  }
});

// Cancel a scheduled message
app.delete('/api/scheduled-messages/:id', async (req, res) => {
  const id = parseInt(req.params.id);
  const { userId } = req.body as { userId: number };

  try {
    const [updated] = await db
      .update(schema.scheduledMessages)
      .set({ cancelled: true })
      .where(
        and(
          eq(schema.scheduledMessages.id, id),
          eq(schema.scheduledMessages.userId, userId),
          eq(schema.scheduledMessages.sent, false),
          eq(schema.scheduledMessages.cancelled, false)
        )
      )
      .returning();

    if (!updated) return res.status(404).json({ error: 'Scheduled message not found or already sent/cancelled' });
    res.json({ ok: true });
  } catch (e) {
    console.error(e);
    res.status(500).json({ error: 'Failed to cancel scheduled message' });
  }
});

// Background job: send due scheduled messages every 10 seconds
setInterval(async () => {
  try {
    const due = await db
      .select()
      .from(schema.scheduledMessages)
      .where(
        and(
          eq(schema.scheduledMessages.sent, false),
          eq(schema.scheduledMessages.cancelled, false),
          lte(schema.scheduledMessages.scheduledFor, new Date())
        )
      );

    for (const scheduled of due) {
      // Mark as sent first to avoid double-sending
      const [updated] = await db
        .update(schema.scheduledMessages)
        .set({ sent: true })
        .where(
          and(
            eq(schema.scheduledMessages.id, scheduled.id),
            eq(schema.scheduledMessages.sent, false)
          )
        )
        .returning();

      if (!updated) continue;

      const [user] = await db.select().from(schema.users).where(eq(schema.users.id, scheduled.userId));
      if (!user) continue;

      const [message] = await db
        .insert(schema.messages)
        .values({ roomId: scheduled.roomId, userId: scheduled.userId, content: scheduled.content })
        .returning();

      const fullMessage = {
        ...message,
        userName: user.name,
        readBy: [] as { userId: number; userName: string }[],
        reactions: [] as { emoji: string; userId: number; userName: string }[],
        editedAt: null as Date | null,
      };

      io.to(`room:${scheduled.roomId}`).emit('message', fullMessage);

      // Notify members not in the room
      const activeSocketIds = io.sockets.adapter.rooms.get(`room:${scheduled.roomId}`) ?? new Set<string>();
      const members = await db.select().from(schema.roomMembers).where(eq(schema.roomMembers.roomId, scheduled.roomId));
      for (const member of members) {
        if (member.userId === scheduled.userId) continue;
        const memberSocketId = userSockets.get(member.userId);
        if (memberSocketId && !activeSocketIds.has(memberSocketId)) {
          io.to(memberSocketId).emit('message', fullMessage);
        }
      }

      // Notify the author that their scheduled message was sent
      const authorSocketId = userSockets.get(scheduled.userId);
      if (authorSocketId) {
        io.to(authorSocketId).emit('scheduled_message_sent', { id: scheduled.id });
      }
    }
  } catch (e) {
    console.error('Scheduled message job error:', e);
  }
}, 10000);

// Background job: delete expired ephemeral messages every 5 seconds
setInterval(async () => {
  try {
    const expired = await db
      .select({ id: schema.messages.id, roomId: schema.messages.roomId })
      .from(schema.messages)
      .where(and(isNotNull(schema.messages.expiresAt), lte(schema.messages.expiresAt!, new Date())));

    if (expired.length === 0) return;

    // Notify rooms before deleting
    for (const msg of expired) {
      io.to(`room:${msg.roomId}`).emit('message_expired', { messageId: msg.id, roomId: msg.roomId });
    }

    await db
      .delete(schema.messages)
      .where(and(isNotNull(schema.messages.expiresAt), lte(schema.messages.expiresAt!, new Date())));
  } catch (e) {
    console.error('Ephemeral cleanup job error:', e);
  }
}, 5000);

// ─── Socket.io ────────────────────────────────────────────────────────────────

io.on('connection', (socket) => {
  console.log('Client connected:', socket.id);

  socket.on('register', async ({ userId, userName }: { userId: number; userName: string }) => {
    connectedUsers.set(socket.id, { id: userId, name: userName });
    userSockets.set(userId, socket.id);

    await db
      .update(schema.users)
      .set({ online: true, status: 'online', lastSeen: new Date() })
      .where(eq(schema.users.id, userId));

    io.emit('user_status', { userId, online: true, name: userName, status: 'online' });
  });

  socket.on('set_status', async ({ status }: { status: string }) => {
    const user = connectedUsers.get(socket.id);
    if (!user) return;
    const VALID_STATUSES = ['online', 'away', 'dnd', 'invisible'];
    if (!VALID_STATUSES.includes(status)) return;

    const statusNow = new Date();
    const [updated] = await db
      .update(schema.users)
      .set({ status, lastSeen: statusNow })
      .where(eq(schema.users.id, user.id))
      .returning();

    if (!updated) return;

    // Invisible users appear offline to others
    const broadcastStatus = status === 'invisible' ? 'offline' : status;
    io.emit('user_status', {
      userId: user.id,
      name: user.name,
      online: broadcastStatus !== 'offline',
      status: broadcastStatus,
      lastSeen: statusNow,
    });
  });

  socket.on('join_room', ({ roomId }: { roomId: number }) => {
    socket.join(`room:${roomId}`);
  });

  socket.on('leave_room', ({ roomId }: { roomId: number }) => {
    socket.leave(`room:${roomId}`);
    const user = connectedUsers.get(socket.id);
    if (user) stopTyping(user.id, user.name, roomId);
  });

  socket.on('join_thread', ({ parentMessageId }: { parentMessageId: number }) => {
    socket.join(`thread:${parentMessageId}`);
  });

  socket.on('leave_thread', ({ parentMessageId }: { parentMessageId: number }) => {
    socket.leave(`thread:${parentMessageId}`);
  });

  socket.on(
    'send_message',
    async ({ roomId, content, expiresInMs, parentMessageId }: { roomId: number; content: string; expiresInMs?: number; parentMessageId?: number }) => {
      const user = connectedUsers.get(socket.id);
      if (!user) return;

      // Rate limit: 500ms between messages
      const now = Date.now();
      const last = lastMessageTime.get(user.id) ?? 0;
      if (now - last < 500) return;
      lastMessageTime.set(user.id, now);

      if (!content?.trim() || content.trim().length > 2000) return;

      // Check if user is banned from this room
      const [banned] = await db
        .select()
        .from(schema.bannedUsers)
        .where(and(eq(schema.bannedUsers.userId, user.id), eq(schema.bannedUsers.roomId, roomId)));
      if (banned) return;

      // Check if user is a member of this room
      const [membership] = await db
        .select()
        .from(schema.roomMembers)
        .where(and(eq(schema.roomMembers.userId, user.id), eq(schema.roomMembers.roomId, roomId)));
      if (!membership) return;

      const expiresAt = expiresInMs && expiresInMs > 0 ? new Date(Date.now() + expiresInMs) : null;

      // Update lastSeen on activity
      await db.update(schema.users).set({ lastSeen: new Date() }).where(eq(schema.users.id, user.id));

      const [message] = await db
        .insert(schema.messages)
        .values({
          roomId,
          userId: user.id,
          content: content.trim(),
          ...(expiresAt ? { expiresAt } : {}),
          ...(parentMessageId ? { parentMessageId } : {}),
        })
        .returning();

      const fullMessage = {
        ...message,
        userName: user.name,
        readBy: [] as { userId: number; userName: string }[],
        reactions: [] as { emoji: string; userId: number; userName: string }[],
        editedAt: null as Date | null,
        replyCount: 0,
      };

      if (parentMessageId) {
        // Thread reply: emit to thread room subscribers
        io.to(`thread:${parentMessageId}`).emit('thread_reply', fullMessage);

        // Count total replies and notify main room to update reply count badge
        const countResult = await pool.query<{ count: string }>(
          'SELECT COUNT(*)::int as count FROM messages WHERE parent_message_id = $1',
          [parentMessageId]
        );
        const replyCount = parseInt(countResult.rows[0]?.count ?? '1');
        io.to(`room:${roomId}`).emit('reply_count_updated', { messageId: parentMessageId, replyCount });
      } else {
        // Top-level message: emit to room
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
        // Track room activity for activity indicators
        recordRoomMessage(roomId);
      }
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

      const now = new Date();
      await db
        .update(schema.users)
        .set({ online: false, status: 'offline', lastSeen: now })
        .where(eq(schema.users.id, user.id));

      io.emit('user_status', { userId: user.id, online: false, name: user.name, status: 'offline', lastSeen: now });

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

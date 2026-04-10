import 'dotenv/config';
import express from 'express';
import { createServer } from 'http';
import { Server } from 'socket.io';
import cors from 'cors';
import { drizzle } from 'drizzle-orm/node-postgres';
import pg from 'pg';
import { eq, and, desc, gt, count, sql, isNull, lte, isNotNull } from 'drizzle-orm';
import * as schema from './schema.js';

const { Pool } = pg;

const pool = new Pool({ connectionString: process.env.DATABASE_URL });
const db = drizzle(pool, { schema });


const app = express();
const httpServer = createServer(app);

const io = new Server(httpServer, {
  cors: { origin: 'http://localhost:6273', credentials: true },
});

app.use(cors({ origin: 'http://localhost:6273', credentials: true }));
app.use(express.json());

// In-memory: socket -> user mapping, and per-room typing state
const socketToUser = new Map<string, { userId: number; username: string }>();
const onlineUsers = new Map<number, { username: string; socketId: string; status: string; lastActiveAt: Date }>();
// roomId -> Map<userId, timeout>
const typingTimers = new Map<number, Map<number, ReturnType<typeof setTimeout>>>();
// Pending room invitations: inviteId -> invite info
let _inviteCounter = 0;
const pendingInvites = new Map<string, { inviteId: string; roomId: number; inviteeId: number; adminId: number; roomName: string; inviterUsername: string }>();

// ── REST Routes ─────────────────────────────────────────────────────────────

// Users
app.post('/api/users', async (req, res) => {
  const { username } = req.body as { username?: string };
  if (!username || username.trim().length < 1 || username.trim().length > 32) {
    return res.status(400).json({ error: 'Username must be 1-32 characters' });
  }
  const name = username.trim();
  try {
    const existing = await db.select().from(schema.users).where(eq(schema.users.username, name));
    if (existing.length > 0) return res.json(existing[0]);
    const [user] = await db.insert(schema.users).values({ username: name }).returning();
    return res.json(user);
  } catch (err) {
    return res.status(500).json({ error: 'Failed to create user' });
  }
});

app.get('/api/users', async (_req, res) => {
  const users = await db.select().from(schema.users);
  return res.json(users);
});

// Update user status (presence)
app.put('/api/users/:userId/status', async (req, res) => {
  const userId = parseInt(req.params.userId);
  const { status } = req.body as { status?: string };
  const VALID_STATUSES = ['online', 'away', 'dnd', 'invisible'];
  if (!status || !VALID_STATUSES.includes(status)) {
    return res.status(400).json({ error: 'Invalid status. Must be one of: online, away, dnd, invisible' });
  }
  try {
    const [updated] = await db.update(schema.users)
      .set({ status, lastActiveAt: new Date() })
      .where(eq(schema.users.id, userId))
      .returning();
    if (!updated) return res.status(404).json({ error: 'User not found' });

    // Update in-memory map
    const entry = onlineUsers.get(userId);
    if (entry) {
      entry.status = status;
      entry.lastActiveAt = new Date();
    }

    // Broadcast presence update to all
    io.emit('user_presence_update', {
      userId,
      username: updated.username,
      status,
      lastActiveAt: updated.lastActiveAt,
    });

    return res.json({ ok: true });
  } catch (err) {
    return res.status(500).json({ error: 'Failed to update status' });
  }
});

// Create anonymous user (guest)
app.post('/api/users/anonymous', async (req, res) => {
  try {
    let username = '';
    let attempts = 0;
    while (attempts < 20) {
      const suffix = Math.random().toString(16).slice(2, 6).toUpperCase();
      username = `Guest-${suffix}`;
      const existing = await db.select().from(schema.users).where(eq(schema.users.username, username));
      if (existing.length === 0) break;
      attempts++;
    }
    if (!username) return res.status(500).json({ error: 'Could not generate guest name' });
    const [user] = await db.insert(schema.users).values({ username, isAnonymous: true }).returning();
    return res.json(user);
  } catch (err) {
    return res.status(500).json({ error: 'Failed to create guest user' });
  }
});

// Register (upgrade anonymous user to registered)
app.post('/api/users/:userId/register', async (req, res) => {
  const userId = parseInt(req.params.userId);
  const { username } = req.body as { username?: string };
  if (!username || username.trim().length < 1 || username.trim().length > 32) {
    return res.status(400).json({ error: 'Username must be 1-32 characters' });
  }
  const name = username.trim();
  try {
    const [currentUser] = await db.select().from(schema.users).where(eq(schema.users.id, userId));
    if (!currentUser) return res.status(404).json({ error: 'User not found' });
    if (!currentUser.isAnonymous) return res.status(400).json({ error: 'User is already registered' });

    // Check if target username is taken by a registered user
    const existing = await db.select().from(schema.users).where(eq(schema.users.username, name));
    if (existing.length > 0 && existing[0].id !== userId && !existing[0].isAnonymous) {
      return res.status(409).json({ error: 'Username already taken' });
    }

    // If target username belongs to another anonymous user, delete that ghost
    if (existing.length > 0 && existing[0].id !== userId && existing[0].isAnonymous) {
      await db.delete(schema.users).where(eq(schema.users.id, existing[0].id));
    }

    const [updated] = await db.update(schema.users)
      .set({ username: name, isAnonymous: false })
      .where(eq(schema.users.id, userId))
      .returning();

    // Update in-memory online users map
    const entry = onlineUsers.get(userId);
    if (entry) {
      entry.username = name;
      socketToUser.set(entry.socketId, { userId, username: name });
    }

    // Broadcast identity change
    io.emit('user_identity_updated', { userId, oldUsername: currentUser.username, newUsername: name, isAnonymous: false });
    broadcastOnlineUsers();

    return res.json(updated);
  } catch (err) {
    return res.status(500).json({ error: 'Failed to register user' });
  }
});

// Rooms
app.get('/api/rooms', async (req, res) => {
  const userId = req.query.userId ? parseInt(req.query.userId as string) : null;
  const allRooms = await db.select().from(schema.rooms).orderBy(schema.rooms.createdAt);
  if (!userId) return res.json(allRooms.filter(r => !r.isPrivate));
  const memberships = await db.select({ roomId: schema.roomMembers.roomId }).from(schema.roomMembers).where(eq(schema.roomMembers.userId, userId));
  const memberRoomIds = new Set(memberships.map(m => m.roomId));
  const visible = allRooms.filter(r => !r.isPrivate || memberRoomIds.has(r.id));
  return res.json(visible);
});

app.post('/api/rooms', async (req, res) => {
  const { name, userId, isPrivate } = req.body as { name?: string; userId?: number; isPrivate?: boolean };
  if (!name || name.trim().length < 1 || name.trim().length > 64) {
    return res.status(400).json({ error: 'Room name must be 1-64 characters' });
  }
  const roomName = name.trim();
  try {
    const existing = await db.select().from(schema.rooms).where(eq(schema.rooms.name, roomName));
    if (existing.length > 0) return res.json(existing[0]);
    const [room] = await db.insert(schema.rooms).values({ name: roomName, isPrivate: !!isPrivate, ...(userId ? { creatorId: userId } : {}) }).returning();
    // Auto-join creator as admin
    if (userId) {
      await db.insert(schema.roomMembers).values({ userId, roomId: room.id, role: 'admin' }).onConflictDoNothing();
    }
    if (room.isPrivate) {
      // Only notify the creator for private rooms
      const creatorSocket = userId ? onlineUsers.get(userId) : null;
      if (creatorSocket) io.to(creatorSocket.socketId).emit('room_created', room);
    } else {
      io.emit('room_created', room);
    }
    return res.json(room);
  } catch (err) {
    return res.status(500).json({ error: 'Failed to create room' });
  }
});

// Join / Leave room
app.post('/api/rooms/:roomId/join', async (req, res) => {
  const roomId = parseInt(req.params.roomId);
  const { userId } = req.body as { userId?: number };
  if (!userId) return res.status(400).json({ error: 'userId required' });
  try {
    // Check if banned
    const ban = await db.select().from(schema.roomBans)
      .where(and(eq(schema.roomBans.userId, userId), eq(schema.roomBans.roomId, roomId)));
    if (ban.length > 0) return res.status(403).json({ error: 'You are banned from this room' });

    const existing = await db.select().from(schema.roomMembers)
      .where(and(eq(schema.roomMembers.userId, userId), eq(schema.roomMembers.roomId, roomId)));
    if (existing.length === 0) {
      await db.insert(schema.roomMembers).values({ userId, roomId });
      const [user] = await db.select().from(schema.users).where(eq(schema.users.id, userId));
      io.to(`room:${roomId}`).emit('member_added', { userId, roomId, role: 'member', username: user?.username });
    }
    return res.json({ ok: true });
  } catch (err) {
    return res.status(500).json({ error: 'Failed to join room' });
  }
});

app.post('/api/rooms/:roomId/leave', async (req, res) => {
  const roomId = parseInt(req.params.roomId);
  const { userId } = req.body as { userId?: number };
  if (!userId) return res.status(400).json({ error: 'userId required' });
  try {
    await db.delete(schema.roomMembers)
      .where(and(eq(schema.roomMembers.userId, userId), eq(schema.roomMembers.roomId, roomId)));
    io.to(`room:${roomId}`).emit('member_removed', { userId, roomId });
    return res.json({ ok: true });
  } catch (err) {
    return res.status(500).json({ error: 'Failed to leave room' });
  }
});

app.get('/api/rooms/:roomId/members', async (req, res) => {
  const roomId = parseInt(req.params.roomId);
  const members = await db.select({
    userId: schema.roomMembers.userId,
    role: schema.roomMembers.role,
    username: schema.users.username,
    status: schema.users.status,
  })
    .from(schema.roomMembers)
    .innerJoin(schema.users, eq(schema.roomMembers.userId, schema.users.id))
    .where(eq(schema.roomMembers.roomId, roomId));
  return res.json(members);
});

// Kick user from room
app.post('/api/rooms/:roomId/kick', async (req, res) => {
  const roomId = parseInt(req.params.roomId);
  const { adminId, targetUserId } = req.body as { adminId?: number; targetUserId?: number };
  if (!adminId || !targetUserId) return res.status(400).json({ error: 'adminId and targetUserId required' });
  try {
    // Check requester is admin
    const [admin] = await db.select().from(schema.roomMembers)
      .where(and(eq(schema.roomMembers.userId, adminId), eq(schema.roomMembers.roomId, roomId), eq(schema.roomMembers.role, 'admin')));
    if (!admin) return res.status(403).json({ error: 'Not an admin' });

    // Remove from room
    await db.delete(schema.roomMembers)
      .where(and(eq(schema.roomMembers.userId, targetUserId), eq(schema.roomMembers.roomId, roomId)));

    // Notify target user
    const targetSocket = onlineUsers.get(targetUserId);
    if (targetSocket) {
      io.to(targetSocket.socketId).emit('kicked_from_room', { roomId });
    }

    // Notify room members
    io.to(`room:${roomId}`).emit('member_removed', { userId: targetUserId, roomId });
    return res.json({ ok: true });
  } catch (err) {
    return res.status(500).json({ error: 'Failed to kick user' });
  }
});

// Ban user from room
app.post('/api/rooms/:roomId/ban', async (req, res) => {
  const roomId = parseInt(req.params.roomId);
  const { adminId, targetUserId } = req.body as { adminId?: number; targetUserId?: number };
  if (!adminId || !targetUserId) return res.status(400).json({ error: 'adminId and targetUserId required' });
  try {
    const [admin] = await db.select().from(schema.roomMembers)
      .where(and(eq(schema.roomMembers.userId, adminId), eq(schema.roomMembers.roomId, roomId), eq(schema.roomMembers.role, 'admin')));
    if (!admin) return res.status(403).json({ error: 'Not an admin' });

    // Insert ban
    await db.insert(schema.roomBans)
      .values({ userId: targetUserId, roomId, bannedBy: adminId })
      .onConflictDoNothing();

    // Remove from room members
    await db.delete(schema.roomMembers)
      .where(and(eq(schema.roomMembers.userId, targetUserId), eq(schema.roomMembers.roomId, roomId)));

    const targetSocket = onlineUsers.get(targetUserId);
    if (targetSocket) {
      io.to(targetSocket.socketId).emit('kicked_from_room', { roomId, banned: true });
    }
    io.to(`room:${roomId}`).emit('member_removed', { userId: targetUserId, roomId });
    return res.json({ ok: true });
  } catch (err) {
    return res.status(500).json({ error: 'Failed to ban user' });
  }
});

// Promote user to admin
app.post('/api/rooms/:roomId/promote', async (req, res) => {
  const roomId = parseInt(req.params.roomId);
  const { adminId, targetUserId } = req.body as { adminId?: number; targetUserId?: number };
  if (!adminId || !targetUserId) return res.status(400).json({ error: 'adminId and targetUserId required' });
  try {
    const [admin] = await db.select().from(schema.roomMembers)
      .where(and(eq(schema.roomMembers.userId, adminId), eq(schema.roomMembers.roomId, roomId), eq(schema.roomMembers.role, 'admin')));
    if (!admin) return res.status(403).json({ error: 'Not an admin' });

    await db.update(schema.roomMembers)
      .set({ role: 'admin' })
      .where(and(eq(schema.roomMembers.userId, targetUserId), eq(schema.roomMembers.roomId, roomId)));

    const [targetUser] = await db.select().from(schema.users).where(eq(schema.users.id, targetUserId));
    io.to(`room:${roomId}`).emit('member_role_changed', { userId: targetUserId, role: 'admin', username: targetUser?.username, roomId });
    return res.json({ ok: true });
  } catch (err) {
    return res.status(500).json({ error: 'Failed to promote user' });
  }
});

// Invite user to private room (sends pending invite with Accept/Decline)
app.post('/api/rooms/:roomId/invite', async (req, res) => {
  const roomId = parseInt(req.params.roomId);
  const { adminId, inviteeUsername } = req.body as { adminId?: number; inviteeUsername?: string };
  if (!adminId || !inviteeUsername) return res.status(400).json({ error: 'adminId and inviteeUsername required' });
  try {
    const [adminMember] = await db.select().from(schema.roomMembers)
      .where(and(eq(schema.roomMembers.userId, adminId), eq(schema.roomMembers.roomId, roomId), eq(schema.roomMembers.role, 'admin')));
    if (!adminMember) return res.status(403).json({ error: 'Not an admin' });

    const [invitee] = await db.select().from(schema.users).where(eq(schema.users.username, inviteeUsername.trim()));
    if (!invitee) return res.status(404).json({ error: 'User not found' });

    // Check if already a member
    const [existing] = await db.select().from(schema.roomMembers)
      .where(and(eq(schema.roomMembers.userId, invitee.id), eq(schema.roomMembers.roomId, roomId)));
    if (existing) return res.json({ ok: true, userId: invitee.id, username: invitee.username, alreadyMember: true });

    const [room] = await db.select().from(schema.rooms).where(eq(schema.rooms.id, roomId));
    if (!room) return res.status(404).json({ error: 'Room not found' });

    const [inviter] = await db.select().from(schema.users).where(eq(schema.users.id, adminId));

    // Create pending invite (do NOT add to room_members yet)
    const inviteId = `inv-${++_inviteCounter}-${Date.now()}`;
    pendingInvites.set(inviteId, {
      inviteId,
      roomId,
      inviteeId: invitee.id,
      adminId,
      roomName: room.name,
      inviterUsername: inviter?.username || 'someone',
    });

    // Notify the invited user — they must Accept or Decline
    const inviteeSocket = onlineUsers.get(invitee.id);
    if (inviteeSocket) {
      io.to(inviteeSocket.socketId).emit('room_invite_received', {
        inviteId,
        roomId,
        roomName: room.name,
        inviterUsername: inviter?.username || 'someone',
      });
    }

    return res.json({ ok: true, userId: invitee.id, username: invitee.username });
  } catch (err) {
    return res.status(500).json({ error: 'Failed to invite user' });
  }
});

// Accept a pending room invite
app.post('/api/invites/:inviteId/accept', async (req, res) => {
  const { inviteId } = req.params;
  const { userId } = req.body as { userId?: number };
  const invite = pendingInvites.get(inviteId);
  if (!invite) return res.status(404).json({ error: 'Invite not found or already handled' });
  if (userId !== invite.inviteeId) return res.status(403).json({ error: 'Not the invited user' });
  try {
    pendingInvites.delete(inviteId);
    await db.insert(schema.roomMembers)
      .values({ userId: invite.inviteeId, roomId: invite.roomId, role: 'member' })
      .onConflictDoNothing();
    const [invitee] = await db.select().from(schema.users).where(eq(schema.users.id, invite.inviteeId));
    const [room] = await db.select().from(schema.rooms).where(eq(schema.rooms.id, invite.roomId));
    io.to(`room:${invite.roomId}`).emit('member_added', { userId: invite.inviteeId, roomId: invite.roomId, role: 'member', username: invitee?.username });
    // Tell the invitee to add the room to their list
    const inviteeSocket = onlineUsers.get(invite.inviteeId);
    if (inviteeSocket && room) {
      io.to(inviteeSocket.socketId).emit('room_invited', room);
    }
    return res.json({ ok: true });
  } catch (err) {
    return res.status(500).json({ error: 'Failed to accept invite' });
  }
});

// Decline a pending room invite
app.post('/api/invites/:inviteId/decline', async (req, res) => {
  const { inviteId } = req.params;
  const { userId } = req.body as { userId?: number };
  const invite = pendingInvites.get(inviteId);
  if (!invite) return res.status(404).json({ error: 'Invite not found or already handled' });
  if (userId !== invite.inviteeId) return res.status(403).json({ error: 'Not the invited user' });
  pendingInvites.delete(inviteId);
  return res.json({ ok: true });
});

// Direct Messages — create or find DM room between two users
app.post('/api/dm', async (req, res) => {
  const { userId, targetUserId } = req.body as { userId?: number; targetUserId?: number };
  if (!userId || !targetUserId) return res.status(400).json({ error: 'userId and targetUserId required' });
  if (userId === targetUserId) return res.status(400).json({ error: 'Cannot DM yourself' });
  const [a, b] = [userId, targetUserId].sort((x, y) => x - y);
  const dmName = `__dm_${a}_${b}__`;
  try {
    const existing = await db.select().from(schema.rooms).where(eq(schema.rooms.name, dmName));
    if (existing.length > 0) {
      // Ensure both users are members
      await db.insert(schema.roomMembers).values({ userId, roomId: existing[0].id, role: 'member' }).onConflictDoNothing();
      await db.insert(schema.roomMembers).values({ userId: targetUserId, roomId: existing[0].id, role: 'member' }).onConflictDoNothing();
      return res.json(existing[0]);
    }
    const [room] = await db.insert(schema.rooms).values({ name: dmName, isPrivate: true, creatorId: userId }).returning();
    await db.insert(schema.roomMembers).values({ userId, roomId: room.id, role: 'member' }).onConflictDoNothing();
    await db.insert(schema.roomMembers).values({ userId: targetUserId, roomId: room.id, role: 'member' }).onConflictDoNothing();
    // Notify both users
    const userSocket = onlineUsers.get(userId);
    const targetSocket = onlineUsers.get(targetUserId);
    if (userSocket) io.to(userSocket.socketId).emit('room_invited', room);
    if (targetSocket) io.to(targetSocket.socketId).emit('room_invited', room);
    return res.json(room);
  } catch (err) {
    return res.status(500).json({ error: 'Failed to create DM' });
  }
});

// Messages
app.get('/api/rooms/:roomId/messages', async (req, res) => {
  const roomId = parseInt(req.params.roomId);
  const now = new Date();
  const msgs = await db.select({
    id: schema.messages.id,
    roomId: schema.messages.roomId,
    userId: schema.messages.userId,
    content: schema.messages.content,
    createdAt: schema.messages.createdAt,
    expiresAt: schema.messages.expiresAt,
    editedAt: schema.messages.editedAt,
    username: schema.users.username,
    replyCount: sql<number>`(SELECT COUNT(*) FROM messages r WHERE r.parent_message_id = ${schema.messages.id})::int`.as('reply_count'),
  })
    .from(schema.messages)
    .innerJoin(schema.users, eq(schema.messages.userId, schema.users.id))
    .where(and(
      eq(schema.messages.roomId, roomId),
      isNull(schema.messages.parentMessageId),
      sql`(${schema.messages.expiresAt} IS NULL OR ${schema.messages.expiresAt} > ${now})`
    ))
    .orderBy(schema.messages.createdAt)
    .limit(200);
  return res.json(msgs);
});

// Thread replies
app.get('/api/messages/:messageId/replies', async (req, res) => {
  const messageId = parseInt(req.params.messageId);
  const now = new Date();
  try {
    const replies = await db.select({
      id: schema.messages.id,
      roomId: schema.messages.roomId,
      userId: schema.messages.userId,
      content: schema.messages.content,
      createdAt: schema.messages.createdAt,
      expiresAt: schema.messages.expiresAt,
      editedAt: schema.messages.editedAt,
      username: schema.users.username,
      parentMessageId: schema.messages.parentMessageId,
    })
      .from(schema.messages)
      .innerJoin(schema.users, eq(schema.messages.userId, schema.users.id))
      .where(and(
        eq(schema.messages.parentMessageId, messageId),
        sql`(${schema.messages.expiresAt} IS NULL OR ${schema.messages.expiresAt} > ${now})`
      ))
      .orderBy(schema.messages.createdAt);
    return res.json(replies);
  } catch (err) {
    return res.status(500).json({ error: 'Failed to fetch replies' });
  }
});

app.post('/api/messages/:messageId/replies', async (req, res) => {
  const parentMessageId = parseInt(req.params.messageId);
  const { userId, content } = req.body as { userId?: number; content?: string };
  if (!userId || !content || content.trim().length === 0) {
    return res.status(400).json({ error: 'userId and content required' });
  }
  if (content.trim().length > 2000) {
    return res.status(400).json({ error: 'Reply too long (max 2000 chars)' });
  }
  try {
    const [parent] = await db.select().from(schema.messages).where(eq(schema.messages.id, parentMessageId));
    if (!parent) return res.status(404).json({ error: 'Parent message not found' });
    const [msg] = await db.insert(schema.messages)
      .values({ roomId: parent.roomId, userId, content: content.trim(), parentMessageId })
      .returning();
    const [user] = await db.select().from(schema.users).where(eq(schema.users.id, userId));
    const fullMsg = { ...msg, username: user.username };
    io.to(`room:${parent.roomId}`).emit('new_reply', { ...fullMsg, parentMessageId });
    return res.json(fullMsg);
  } catch (err) {
    return res.status(500).json({ error: 'Failed to post reply' });
  }
});

app.post('/api/rooms/:roomId/messages', async (req, res) => {
  const roomId = parseInt(req.params.roomId);
  const { userId, content, expiresInSeconds } = req.body as { userId?: number; content?: string; expiresInSeconds?: number };
  if (!userId || !content || content.trim().length === 0) {
    return res.status(400).json({ error: 'userId and content required' });
  }
  if (content.trim().length > 2000) {
    return res.status(400).json({ error: 'Message too long (max 2000 chars)' });
  }
  let expiresAt: Date | null = null;
  if (expiresInSeconds && expiresInSeconds > 0) {
    expiresAt = new Date(Date.now() + expiresInSeconds * 1000);
  }
  try {
    const [msg] = await db.insert(schema.messages)
      .values({ roomId, userId, content: content.trim(), ...(expiresAt ? { expiresAt } : {}) })
      .returning();
    const [user] = await db.select().from(schema.users).where(eq(schema.users.id, userId));
    const fullMsg = { ...msg, username: user.username };
    io.to(`room:${roomId}`).emit('new_message', fullMsg);
    broadcastRoomActivity(roomId);
    return res.json(fullMsg);
  } catch (err) {
    return res.status(500).json({ error: 'Failed to send message' });
  }
});

// Read receipts
app.post('/api/messages/read', async (req, res) => {
  const { userId, roomId, messageId } = req.body as { userId?: number; roomId?: number; messageId?: number };
  if (!userId || !roomId || !messageId) {
    return res.status(400).json({ error: 'userId, roomId, messageId required' });
  }
  try {
    // Upsert last read position
    await db.insert(schema.userRoomLastRead)
      .values({ userId, roomId, lastReadMessageId: messageId })
      .onConflictDoUpdate({
        target: [schema.userRoomLastRead.userId, schema.userRoomLastRead.roomId],
        set: { lastReadMessageId: messageId, updatedAt: new Date() },
      });

    // Get all messages up to messageId in this room
    const msgs = await db.select({ id: schema.messages.id })
      .from(schema.messages)
      .where(and(eq(schema.messages.roomId, roomId), sql`${schema.messages.id} <= ${messageId}`));

    // Insert read receipts for all unread messages
    for (const msg of msgs) {
      await db.insert(schema.messageReads)
        .values({ userId, messageId: msg.id })
        .onConflictDoNothing();
    }

    // Fetch who read the latest message
    const readers = await db.select({ userId: schema.messageReads.userId, username: schema.users.username })
      .from(schema.messageReads)
      .innerJoin(schema.users, eq(schema.messageReads.userId, schema.users.id))
      .where(eq(schema.messageReads.messageId, messageId));

    io.to(`room:${roomId}`).emit('read_receipt_update', { messageId, readers });

    return res.json({ ok: true });
  } catch (err) {
    return res.status(500).json({ error: 'Failed to mark as read' });
  }
});

app.get('/api/rooms/:roomId/read-receipts', async (req, res) => {
  const roomId = parseInt(req.params.roomId);
  const { userId } = req.query as { userId?: string };
  if (!userId) return res.status(400).json({ error: 'userId required' });

  // For each message in the room, who has read it
  const msgs = await db.select({ id: schema.messages.id })
    .from(schema.messages)
    .where(eq(schema.messages.roomId, roomId));

  const result: Record<number, { userId: number; username: string }[]> = {};
  for (const msg of msgs) {
    const readers = await db.select({ userId: schema.messageReads.userId, username: schema.users.username })
      .from(schema.messageReads)
      .innerJoin(schema.users, eq(schema.messageReads.userId, schema.users.id))
      .where(eq(schema.messageReads.messageId, msg.id));
    result[msg.id] = readers;
  }
  return res.json(result);
});

// Unread counts per room for a user
app.get('/api/users/:userId/unread', async (req, res) => {
  const userId = parseInt(req.params.userId);

  // Get all rooms user is a member of
  const memberships = await db.select({ roomId: schema.roomMembers.roomId })
    .from(schema.roomMembers)
    .where(eq(schema.roomMembers.userId, userId));

  const unread: Record<number, number> = {};
  for (const { roomId } of memberships) {
    const lastRead = await db.select().from(schema.userRoomLastRead)
      .where(and(eq(schema.userRoomLastRead.userId, userId), eq(schema.userRoomLastRead.roomId, roomId)));
    const lastReadId = lastRead[0]?.lastReadMessageId ?? 0;
    const [result] = await db.select({ count: count() })
      .from(schema.messages)
      .where(and(
        eq(schema.messages.roomId, roomId),
        gt(schema.messages.id, lastReadId),
        isNull(schema.messages.parentMessageId)
      ));
    unread[roomId] = Number(result.count);
  }
  return res.json(unread);
});

// Message Reactions
app.get('/api/rooms/:roomId/reactions', async (req, res) => {
  const roomId = parseInt(req.params.roomId);
  const reactions = await db.select({
    id: schema.messageReactions.id,
    messageId: schema.messageReactions.messageId,
    userId: schema.messageReactions.userId,
    emoji: schema.messageReactions.emoji,
    username: schema.users.username,
  })
    .from(schema.messageReactions)
    .innerJoin(schema.users, eq(schema.messageReactions.userId, schema.users.id))
    .innerJoin(schema.messages, eq(schema.messageReactions.messageId, schema.messages.id))
    .where(eq(schema.messages.roomId, roomId));
  return res.json(reactions);
});

app.post('/api/messages/:messageId/reactions', async (req, res) => {
  const messageId = parseInt(req.params.messageId);
  const { userId, emoji } = req.body as { userId?: number; emoji?: string };
  if (!userId || !emoji) return res.status(400).json({ error: 'userId and emoji required' });
  const ALLOWED_EMOJI = ['👍', '❤️', '😂', '😮', '😢'];
  if (!ALLOWED_EMOJI.includes(emoji)) return res.status(400).json({ error: 'Invalid emoji' });

  try {
    // Get message roomId for socket broadcast
    const [message] = await db.select({ roomId: schema.messages.roomId })
      .from(schema.messages)
      .where(eq(schema.messages.id, messageId));
    if (!message) return res.status(404).json({ error: 'Message not found' });

    // Toggle: remove if exists, add if not
    const existing = await db.select()
      .from(schema.messageReactions)
      .where(and(
        eq(schema.messageReactions.messageId, messageId),
        eq(schema.messageReactions.userId, userId),
        eq(schema.messageReactions.emoji, emoji)
      ));

    if (existing.length > 0) {
      await db.delete(schema.messageReactions)
        .where(and(
          eq(schema.messageReactions.messageId, messageId),
          eq(schema.messageReactions.userId, userId),
          eq(schema.messageReactions.emoji, emoji)
        ));
    } else {
      await db.insert(schema.messageReactions)
        .values({ messageId, userId, emoji });
    }

    // Fetch updated reactions for this message
    const reactions = await db.select({
      messageId: schema.messageReactions.messageId,
      userId: schema.messageReactions.userId,
      emoji: schema.messageReactions.emoji,
      username: schema.users.username,
    })
      .from(schema.messageReactions)
      .innerJoin(schema.users, eq(schema.messageReactions.userId, schema.users.id))
      .where(eq(schema.messageReactions.messageId, messageId));

    io.to(`room:${message.roomId}`).emit('reaction_update', { messageId, reactions });
    return res.json({ ok: true });
  } catch (err) {
    return res.status(500).json({ error: 'Failed to toggle reaction' });
  }
});

// Message Editing
app.put('/api/messages/:messageId', async (req, res) => {
  const messageId = parseInt(req.params.messageId);
  const { userId, content } = req.body as { userId?: number; content?: string };
  if (!userId || !content || content.trim().length === 0) {
    return res.status(400).json({ error: 'userId and content required' });
  }
  if (content.trim().length > 2000) {
    return res.status(400).json({ error: 'Message too long (max 2000 chars)' });
  }
  try {
    const [existing] = await db.select().from(schema.messages).where(eq(schema.messages.id, messageId));
    if (!existing) return res.status(404).json({ error: 'Message not found' });
    if (existing.userId !== userId) return res.status(403).json({ error: 'Cannot edit another user\'s message' });

    // Store previous content in edit history
    await db.insert(schema.messageEdits).values({
      messageId,
      userId,
      previousContent: existing.content,
    });

    // Update message
    const now = new Date();
    const [updated] = await db.update(schema.messages)
      .set({ content: content.trim(), editedAt: now })
      .where(eq(schema.messages.id, messageId))
      .returning();

    const [user] = await db.select().from(schema.users).where(eq(schema.users.id, userId));
    const fullMsg = { ...updated, username: user.username };
    io.to(`room:${existing.roomId}`).emit('message_edited', fullMsg);
    return res.json(fullMsg);
  } catch (err) {
    return res.status(500).json({ error: 'Failed to edit message' });
  }
});

app.get('/api/messages/:messageId/edits', async (req, res) => {
  const messageId = parseInt(req.params.messageId);
  try {
    const edits = await db.select({
      id: schema.messageEdits.id,
      previousContent: schema.messageEdits.previousContent,
      editedAt: schema.messageEdits.editedAt,
      username: schema.users.username,
    })
      .from(schema.messageEdits)
      .innerJoin(schema.users, eq(schema.messageEdits.userId, schema.users.id))
      .where(eq(schema.messageEdits.messageId, messageId))
      .orderBy(schema.messageEdits.editedAt);
    return res.json(edits);
  } catch (err) {
    return res.status(500).json({ error: 'Failed to fetch edit history' });
  }
});

// Scheduled Messages
app.post('/api/rooms/:roomId/scheduled-messages', async (req, res) => {
  const roomId = parseInt(req.params.roomId);
  const { userId, content, scheduledAt } = req.body as { userId?: number; content?: string; scheduledAt?: string };
  if (!userId || !content || !scheduledAt) {
    return res.status(400).json({ error: 'userId, content, and scheduledAt required' });
  }
  if (content.trim().length === 0 || content.trim().length > 2000) {
    return res.status(400).json({ error: 'Content must be 1-2000 characters' });
  }
  const scheduledTime = new Date(scheduledAt);
  if (isNaN(scheduledTime.getTime()) || scheduledTime <= new Date()) {
    return res.status(400).json({ error: 'scheduledAt must be a future time' });
  }
  try {
    const [msg] = await db.insert(schema.scheduledMessages)
      .values({ roomId, userId, content: content.trim(), scheduledAt: scheduledTime })
      .returning();
    return res.json(msg);
  } catch (err) {
    return res.status(500).json({ error: 'Failed to schedule message' });
  }
});

app.get('/api/users/:userId/scheduled-messages', async (req, res) => {
  const userId = parseInt(req.params.userId);
  const pending = await db.select({
    id: schema.scheduledMessages.id,
    roomId: schema.scheduledMessages.roomId,
    content: schema.scheduledMessages.content,
    scheduledAt: schema.scheduledMessages.scheduledAt,
    createdAt: schema.scheduledMessages.createdAt,
    roomName: schema.rooms.name,
  })
    .from(schema.scheduledMessages)
    .innerJoin(schema.rooms, eq(schema.scheduledMessages.roomId, schema.rooms.id))
    .where(and(eq(schema.scheduledMessages.userId, userId), isNull(schema.scheduledMessages.sentAt)));
  return res.json(pending);
});

app.delete('/api/scheduled-messages/:id', async (req, res) => {
  const id = parseInt(req.params.id);
  const { userId } = req.body as { userId?: number };
  if (!userId) return res.status(400).json({ error: 'userId required' });
  try {
    const [deleted] = await db.delete(schema.scheduledMessages)
      .where(and(eq(schema.scheduledMessages.id, id), eq(schema.scheduledMessages.userId, userId), isNull(schema.scheduledMessages.sentAt)))
      .returning();
    if (!deleted) return res.status(404).json({ error: 'Scheduled message not found or already sent' });
    return res.json({ ok: true });
  } catch (err) {
    return res.status(500).json({ error: 'Failed to cancel scheduled message' });
  }
});

// ── Drafts ───────────────────────────────────────────────────────────────────

// Get all drafts for a user
app.get('/api/users/:userId/drafts', async (req, res) => {
  const userId = parseInt(req.params.userId);
  try {
    const userDrafts = await db.select({
      roomId: schema.drafts.roomId,
      content: schema.drafts.content,
      updatedAt: schema.drafts.updatedAt,
    }).from(schema.drafts).where(eq(schema.drafts.userId, userId));
    return res.json(userDrafts);
  } catch (err) {
    return res.status(500).json({ error: 'Failed to fetch drafts' });
  }
});

// Upsert draft for a user/room
app.put('/api/users/:userId/drafts/:roomId', async (req, res) => {
  const userId = parseInt(req.params.userId);
  const roomId = parseInt(req.params.roomId);
  const { content } = req.body as { content?: string };
  if (content === undefined) return res.status(400).json({ error: 'content required' });
  try {
    if (content.trim() === '') {
      // Delete empty draft
      await db.delete(schema.drafts)
        .where(and(eq(schema.drafts.userId, userId), eq(schema.drafts.roomId, roomId)));
    } else {
      await db.insert(schema.drafts)
        .values({ userId, roomId, content, updatedAt: new Date() })
        .onConflictDoUpdate({
          target: [schema.drafts.userId, schema.drafts.roomId],
          set: { content, updatedAt: new Date() },
        });
    }
    // Broadcast to other sessions of the same user
    const userEntry = onlineUsers.get(userId);
    if (userEntry) {
      io.to(userEntry.socketId).emit('draft_update', { roomId, content });
    }
    return res.json({ ok: true });
  } catch (err) {
    return res.status(500).json({ error: 'Failed to save draft' });
  }
});

// ── Room Activity Helpers ────────────────────────────────────────────────────

async function computeRoomActivity(roomId: number): Promise<{ level: string; recentCount: number }> {
  const fiveMinutesAgo = new Date(Date.now() - 5 * 60 * 1000);
  const [result] = await db.select({ count: count() })
    .from(schema.messages)
    .where(and(
      eq(schema.messages.roomId, roomId),
      gt(schema.messages.createdAt, fiveMinutesAgo),
      isNull(schema.messages.parentMessageId),
    ));
  const recentCount = Number(result.count);
  const level = recentCount >= 5 ? 'hot' : recentCount >= 1 ? 'active' : 'quiet';
  return { level, recentCount };
}

async function broadcastRoomActivity(roomId: number) {
  try {
    const activity = await computeRoomActivity(roomId);
    io.emit('room_activity_update', { roomId, ...activity });
  } catch {}
}

// REST: get activity for all rooms
app.get('/api/rooms/activity', async (_req, res) => {
  try {
    const allRooms = await db.select({ id: schema.rooms.id }).from(schema.rooms);
    const results: Record<number, { level: string; recentCount: number }> = {};
    for (const room of allRooms) {
      results[room.id] = await computeRoomActivity(room.id);
    }
    return res.json(results);
  } catch (err) {
    return res.status(500).json({ error: 'Failed to fetch activity' });
  }
});

// Background job: send scheduled messages
setInterval(async () => {
  try {
    const due = await db.select()
      .from(schema.scheduledMessages)
      .where(and(isNull(schema.scheduledMessages.sentAt), lte(schema.scheduledMessages.scheduledAt, new Date())));

    for (const scheduled of due) {
      // Insert actual message
      const [msg] = await db.insert(schema.messages)
        .values({ roomId: scheduled.roomId, userId: scheduled.userId, content: scheduled.content })
        .returning();
      const [user] = await db.select().from(schema.users).where(eq(schema.users.id, scheduled.userId));
      const fullMsg = { ...msg, username: user.username };
      io.to(`room:${scheduled.roomId}`).emit('new_message', fullMsg);
      broadcastRoomActivity(scheduled.roomId);

      // Mark as sent
      await db.update(schema.scheduledMessages)
        .set({ sentAt: new Date() })
        .where(eq(schema.scheduledMessages.id, scheduled.id));

      // Notify the author
      const authorSocket = onlineUsers.get(scheduled.userId);
      if (authorSocket) {
        io.to(authorSocket.socketId).emit('scheduled_message_sent', { id: scheduled.id, roomId: scheduled.roomId });
      }
    }
  } catch (err) {
    console.error('Scheduled message error:', err);
  }
}, 5000);

// Background job: delete expired ephemeral messages
setInterval(async () => {
  try {
    const expired = await db.select({ id: schema.messages.id, roomId: schema.messages.roomId })
      .from(schema.messages)
      .where(and(isNotNull(schema.messages.expiresAt), lte(schema.messages.expiresAt, new Date())));

    for (const msg of expired) {
      await db.delete(schema.messages).where(eq(schema.messages.id, msg.id));
      io.to(`room:${msg.roomId}`).emit('message_deleted', { messageId: msg.id, roomId: msg.roomId });
    }
  } catch (err) {
    console.error('Ephemeral message cleanup error:', err);
  }
}, 3000);

// ── Socket.io ────────────────────────────────────────────────────────────────

io.on('connection', (socket) => {
  socket.on('user_connected', async (data: { userId: number; username: string }) => {
    socketToUser.set(socket.id, data);
    const now = new Date();
    onlineUsers.set(data.userId, { username: data.username, socketId: socket.id, status: 'online', lastActiveAt: now });
    // Set status to online in DB
    try {
      await db.update(schema.users)
        .set({ status: 'online', lastActiveAt: now })
        .where(eq(schema.users.id, data.userId));
    } catch {}
    broadcastOnlineUsers();
  });

  socket.on('join_room', (roomId: number) => {
    socket.join(`room:${roomId}`);
  });

  socket.on('leave_room', (roomId: number) => {
    socket.leave(`room:${roomId}`);
    clearTyping(roomId, socket);
  });

  socket.on('typing_start', (data: { roomId: number }) => {
    const user = socketToUser.get(socket.id);
    if (!user) return;
    const { roomId } = data;

    if (!typingTimers.has(roomId)) typingTimers.set(roomId, new Map());
    const roomTimers = typingTimers.get(roomId)!;

    // Broadcast that user started typing
    socket.to(`room:${roomId}`).emit('user_typing', { userId: user.userId, username: user.username, roomId });

    // Auto-expire after 4 seconds
    if (roomTimers.has(user.userId)) clearTimeout(roomTimers.get(user.userId)!);
    roomTimers.set(user.userId, setTimeout(() => {
      roomTimers.delete(user.userId);
      io.to(`room:${roomId}`).emit('user_stopped_typing', { userId: user.userId, username: user.username, roomId });
    }, 4000));
  });

  socket.on('typing_stop', (data: { roomId: number }) => {
    const user = socketToUser.get(socket.id);
    if (!user) return;
    clearTypingForUser(data.roomId, user.userId, user.username);
  });

  socket.on('disconnect', async () => {
    const user = socketToUser.get(socket.id);
    if (user) {
      onlineUsers.delete(user.userId);
      socketToUser.delete(socket.id);
      // Clear all typing timers for this user
      typingTimers.forEach((roomTimers, roomId) => {
        if (roomTimers.has(user.userId)) {
          clearTimeout(roomTimers.get(user.userId)!);
          roomTimers.delete(user.userId);
          io.to(`room:${roomId}`).emit('user_stopped_typing', { userId: user.userId, username: user.username, roomId });
        }
      });
      // Set offline + update lastActiveAt in DB
      const now = new Date();
      try {
        await db.update(schema.users)
          .set({ status: 'offline', lastActiveAt: now })
          .where(eq(schema.users.id, user.userId));
      } catch {}
      io.emit('user_presence_update', { userId: user.userId, username: user.username, status: 'offline', lastActiveAt: now });
      broadcastOnlineUsers();
    }
  });
});

function broadcastOnlineUsers() {
  io.emit('online_users', Array.from(onlineUsers.entries()).map(([id, u]) => ({
    id,
    username: u.username,
    status: u.status,
    lastActiveAt: u.lastActiveAt,
  })));
}

// Background job: auto-set online users to "away" after 5 minutes of inactivity
setInterval(async () => {
  const fiveMinutesAgo = new Date(Date.now() - 5 * 60 * 1000);
  for (const [userId, entry] of onlineUsers.entries()) {
    if (entry.status === 'online' && entry.lastActiveAt < fiveMinutesAgo) {
      entry.status = 'away';
      try {
        await db.update(schema.users)
          .set({ status: 'away' })
          .where(eq(schema.users.id, userId));
      } catch {}
      io.emit('user_presence_update', { userId, username: entry.username, status: 'away', lastActiveAt: entry.lastActiveAt });
    }
  }
}, 60000);

function clearTyping(roomId: number, socket: import('socket.io').Socket) {
  const user = socketToUser.get(socket.id);
  if (!user) return;
  clearTypingForUser(roomId, user.userId, user.username);
}

function clearTypingForUser(roomId: number, userId: number, username: string) {
  const roomTimers = typingTimers.get(roomId);
  if (roomTimers?.has(userId)) {
    clearTimeout(roomTimers.get(userId)!);
    roomTimers.delete(userId);
    io.to(`room:${roomId}`).emit('user_stopped_typing', { userId, username, roomId });
  }
}

// Background job: refresh room activity indicators every 60 seconds so they decay naturally
setInterval(async () => {
  try {
    const allRooms = await db.select({ id: schema.rooms.id }).from(schema.rooms);
    for (const room of allRooms) {
      await broadcastRoomActivity(room.id);
    }
  } catch {}
}, 60000);

const PORT = parseInt(process.env.PORT || '6001');
httpServer.listen(PORT, () => {
  console.log(`Server running on http://localhost:${PORT}`);
});

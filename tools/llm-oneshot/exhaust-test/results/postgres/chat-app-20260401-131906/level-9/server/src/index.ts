import 'dotenv/config';
import express from 'express';
import cors from 'cors';
import { createServer } from 'http';
import { Server } from 'socket.io';
import { drizzle } from 'drizzle-orm/node-postgres';
import pg from 'pg';
import { eq, and, inArray, gt, lt, lte, desc, sql, isNull, or } from 'drizzle-orm';
import * as schema from './schema.js';

const { Pool } = pg;

const pool = new Pool({
  connectionString: process.env.DATABASE_URL || 'postgresql://spacetime:spacetime@localhost:5433/spacetime',
});

const db = drizzle(pool, { schema });

const app = express();
const httpServer = createServer(app);
const io = new Server(httpServer, {
  cors: {
    origin: 'http://localhost:5173',
    methods: ['GET', 'POST'],
  },
});

app.use(cors({ origin: 'http://localhost:5173' }));
app.use(express.json());

// ── In-memory state ──────────────────────────────────────────────────────────
const onlineUsers = new Map<string, number>(); // socketId → userId
const typingTimers = new Map<string, ReturnType<typeof setTimeout>>(); // `roomId:userId` → timer
const typingUsers = new Map<number, Set<number>>(); // roomId → Set<userId>
const userNameCache = new Map<number, string>(); // userId → name
const lastMessageTime = new Map<number, number>(); // userId → timestamp (rate limiting)

function getOnlineUserIds(): number[] {
  return [...new Set(onlineUsers.values())];
}

// ── Users ─────────────────────────────────────────────────────────────────────
app.post('/api/users', async (req, res) => {
  const { name } = req.body as { name: string };
  if (!name || typeof name !== 'string') {
    res.status(400).json({ error: 'Name is required' });
    return;
  }
  const trimmed = name.trim().slice(0, 32);
  if (!trimmed) { res.status(400).json({ error: 'Name cannot be empty' }); return; }

  try {
    const existing = await db.select().from(schema.users).where(eq(schema.users.name, trimmed)).limit(1);
    if (existing.length > 0) {
      userNameCache.set(existing[0].id, existing[0].name);
      res.json(existing[0]);
      return;
    }
    const [user] = await db.insert(schema.users).values({ name: trimmed }).returning();
    userNameCache.set(user.id, user.name);
    io.emit('user:registered', user);
    res.json(user);
  } catch (e: any) {
    if (e.code === '23505') {
      const existing = await db.select().from(schema.users).where(eq(schema.users.name, trimmed)).limit(1);
      if (existing.length > 0) {
        userNameCache.set(existing[0].id, existing[0].name);
        res.json(existing[0]);
        return;
      }
    }
    res.status(500).json({ error: 'Failed to create user' });
  }
});

app.get('/api/users', async (_req, res) => {
  const users = await db.select().from(schema.users);
  res.json(users);
});

app.get('/api/users/online', (_req, res) => {
  res.json(getOnlineUserIds());
});

// Get user statuses for all known users
app.get('/api/users/statuses', async (_req, res) => {
  const users = await db.select({
    id: schema.users.id,
    status: schema.users.status,
    lastActiveAt: schema.users.lastActiveAt,
  }).from(schema.users);
  res.json(users);
});

// Set user status
app.patch('/api/users/:id/status', async (req, res) => {
  const userId = parseInt(req.params.id);
  const { status } = req.body as { status: string };
  const VALID_STATUSES = ['online', 'away', 'do-not-disturb', 'invisible'];
  if (!VALID_STATUSES.includes(status)) {
    res.status(400).json({ error: 'Invalid status' });
    return;
  }
  const [updated] = await db.update(schema.users)
    .set({ status, lastActiveAt: new Date() })
    .where(eq(schema.users.id, userId))
    .returning({ id: schema.users.id, status: schema.users.status, lastActiveAt: schema.users.lastActiveAt });
  if (!updated) { res.status(404).json({ error: 'User not found' }); return; }
  io.emit('user:status', { userId, status, lastActiveAt: updated.lastActiveAt });
  res.json(updated);
});

// ── Rooms ─────────────────────────────────────────────────────────────────────
async function getRoomWithMeta(roomId: number, userId?: number) {
  const room = await db.select().from(schema.rooms).where(eq(schema.rooms.id, roomId)).limit(1);
  if (!room.length) return null;

  const members = await db.select({ userId: schema.roomMembers.userId })
    .from(schema.roomMembers)
    .where(eq(schema.roomMembers.roomId, roomId));

  const admins = await db.select({ userId: schema.roomAdmins.userId })
    .from(schema.roomAdmins)
    .where(eq(schema.roomAdmins.roomId, roomId));

  let unreadCount = 0;
  if (userId) {
    const lastRead = await db.select().from(schema.lastReadPositions)
      .where(and(eq(schema.lastReadPositions.userId, userId), eq(schema.lastReadPositions.roomId, roomId)))
      .limit(1);

    if (lastRead.length === 0) {
      const [{ count }] = await db.select({ count: sql<number>`count(*)::int` })
        .from(schema.messages).where(eq(schema.messages.roomId, roomId));
      unreadCount = count ?? 0;
    } else if (lastRead[0].lastMessageId) {
      const [{ count }] = await db.select({ count: sql<number>`count(*)::int` })
        .from(schema.messages)
        .where(and(eq(schema.messages.roomId, roomId), gt(schema.messages.id, lastRead[0].lastMessageId!)));
      unreadCount = count ?? 0;
    }
  }

  return { ...room[0], memberIds: members.map(m => m.userId), adminIds: admins.map(a => a.userId), unreadCount };
}

app.get('/api/rooms', async (req, res) => {
  const { userId } = req.query as { userId?: string };
  const uid = userId ? parseInt(userId) : undefined;

  let roomList: typeof schema.rooms.$inferSelect[];
  if (uid) {
    // Return public rooms + private rooms the user is a member of
    const allRooms = await db.select().from(schema.rooms).orderBy(schema.rooms.id);
    const memberRows = await db.select({ roomId: schema.roomMembers.roomId })
      .from(schema.roomMembers).where(eq(schema.roomMembers.userId, uid));
    const memberRoomIds = new Set(memberRows.map(r => r.roomId));
    roomList = allRooms.filter(r => !r.isPrivate || memberRoomIds.has(r.id));
  } else {
    roomList = await db.select().from(schema.rooms)
      .where(eq(schema.rooms.isPrivate, false)).orderBy(schema.rooms.id);
  }

  const result = await Promise.all(roomList.map(r => getRoomWithMeta(r.id, uid)));
  res.json(result.filter(Boolean));
});

app.post('/api/rooms', async (req, res) => {
  const { name, userId, isPrivate } = req.body as { name: string; userId: number; isPrivate?: boolean };
  if (!name || !userId) { res.status(400).json({ error: 'Name and userId required' }); return; }
  const trimmed = name.trim().slice(0, 64);
  if (!trimmed) { res.status(400).json({ error: 'Room name cannot be empty' }); return; }

  try {
    const [room] = await db.insert(schema.rooms).values({ name: trimmed, createdBy: userId, isPrivate: !!isPrivate }).returning();
    await db.insert(schema.roomMembers).values({ userId, roomId: room.id });
    await db.insert(schema.roomAdmins).values({ userId, roomId: room.id });
    const full = await getRoomWithMeta(room.id, userId);
    // Private rooms: only notify creator; public rooms: broadcast to all
    if (isPrivate) {
      io.to(`user:${userId}`).emit('room:created', full);
    } else {
      io.emit('room:created', full);
    }
    res.json(full);
  } catch (e: any) {
    if (e.code === '23505') { res.status(409).json({ error: 'Room name already exists' }); return; }
    res.status(500).json({ error: 'Failed to create room' });
  }
});

app.post('/api/rooms/:id/join', async (req, res) => {
  const roomId = parseInt(req.params.id);
  const { userId } = req.body as { userId: number };
  try {
    const [room] = await db.select().from(schema.rooms).where(eq(schema.rooms.id, roomId)).limit(1);
    if (!room) { res.status(404).json({ error: 'Room not found' }); return; }
    if (room.isPrivate) {
      res.status(403).json({ error: 'This is a private room. You need an invitation to join.' });
      return;
    }
    const ban = await db.select().from(schema.roomBans)
      .where(and(eq(schema.roomBans.userId, userId), eq(schema.roomBans.roomId, roomId)))
      .limit(1);
    if (ban.length > 0) {
      res.status(403).json({ error: 'You are banned from this room' });
      return;
    }
    await db.insert(schema.roomMembers).values({ userId, roomId }).onConflictDoNothing();
    io.emit('room:membership', { roomId, userId, action: 'join' });
    res.json({ success: true });
  } catch {
    res.status(500).json({ error: 'Failed to join room' });
  }
});

// ── Permissions ───────────────────────────────────────────────────────────────
app.post('/api/rooms/:id/kick', async (req, res) => {
  const roomId = parseInt(req.params.id);
  const { adminId, targetUserId } = req.body as { adminId: number; targetUserId: number };

  // Verify requester is an admin
  const admin = await db.select().from(schema.roomAdmins)
    .where(and(eq(schema.roomAdmins.userId, adminId), eq(schema.roomAdmins.roomId, roomId)))
    .limit(1);
  if (admin.length === 0) { res.status(403).json({ error: 'Not an admin' }); return; }

  // Cannot kick another admin (unless you're the room creator)
  const room = await db.select().from(schema.rooms).where(eq(schema.rooms.id, roomId)).limit(1);
  if (!room.length) { res.status(404).json({ error: 'Room not found' }); return; }

  const targetAdmin = await db.select().from(schema.roomAdmins)
    .where(and(eq(schema.roomAdmins.userId, targetUserId), eq(schema.roomAdmins.roomId, roomId)))
    .limit(1);
  if (targetAdmin.length > 0 && room[0].createdBy !== adminId) {
    res.status(403).json({ error: 'Cannot kick another admin' }); return;
  }

  // Remove from members and admins
  await db.delete(schema.roomMembers).where(and(eq(schema.roomMembers.userId, targetUserId), eq(schema.roomMembers.roomId, roomId)));
  await db.delete(schema.roomAdmins).where(and(eq(schema.roomAdmins.userId, targetUserId), eq(schema.roomAdmins.roomId, roomId)));

  // Add to bans
  await db.insert(schema.roomBans).values({ userId: targetUserId, roomId, bannedBy: adminId }).onConflictDoNothing();

  // Force socket leave for kicked user
  await io.in(`user:${targetUserId}`).socketsLeave(`room:${roomId}`);

  // Notify kicked user
  io.to(`user:${targetUserId}`).emit('permission:kicked', { roomId, by: adminId });

  // Notify room of membership change
  io.emit('room:membership', { roomId, userId: targetUserId, action: 'leave' });

  res.json({ success: true });
});

app.post('/api/rooms/:id/promote', async (req, res) => {
  const roomId = parseInt(req.params.id);
  const { adminId, targetUserId } = req.body as { adminId: number; targetUserId: number };

  // Verify requester is an admin
  const admin = await db.select().from(schema.roomAdmins)
    .where(and(eq(schema.roomAdmins.userId, adminId), eq(schema.roomAdmins.roomId, roomId)))
    .limit(1);
  if (admin.length === 0) { res.status(403).json({ error: 'Not an admin' }); return; }

  // Target must be a member
  const member = await db.select().from(schema.roomMembers)
    .where(and(eq(schema.roomMembers.userId, targetUserId), eq(schema.roomMembers.roomId, roomId)))
    .limit(1);
  if (member.length === 0) { res.status(400).json({ error: 'User is not a member' }); return; }

  await db.insert(schema.roomAdmins).values({ userId: targetUserId, roomId }).onConflictDoNothing();

  io.to(`room:${roomId}`).emit('permission:promoted', { roomId, userId: targetUserId, by: adminId });
  res.json({ success: true });
});

app.post('/api/rooms/:id/leave', async (req, res) => {
  const roomId = parseInt(req.params.id);
  const { userId } = req.body as { userId: number };
  await db.delete(schema.roomMembers).where(
    and(eq(schema.roomMembers.userId, userId), eq(schema.roomMembers.roomId, roomId))
  );
  io.emit('room:membership', { roomId, userId, action: 'leave' });
  res.json({ success: true });
});

// ── Invitations ───────────────────────────────────────────────────────────────
// Invite a user to a private room by username
app.post('/api/rooms/:id/invite', async (req, res) => {
  const roomId = parseInt(req.params.id);
  const { inviterId, inviteeUsername } = req.body as { inviterId: number; inviteeUsername: string };

  if (!inviteeUsername) { res.status(400).json({ error: 'inviteeUsername required' }); return; }

  // Verify inviter is a member
  const member = await db.select().from(schema.roomMembers)
    .where(and(eq(schema.roomMembers.userId, inviterId), eq(schema.roomMembers.roomId, roomId)))
    .limit(1);
  if (member.length === 0) { res.status(403).json({ error: 'Not a room member' }); return; }

  // Find invitee by username
  const [invitee] = await db.select().from(schema.users)
    .where(eq(schema.users.name, inviteeUsername.trim())).limit(1);
  if (!invitee) { res.status(404).json({ error: 'User not found' }); return; }

  // Check if already a member
  const alreadyMember = await db.select().from(schema.roomMembers)
    .where(and(eq(schema.roomMembers.userId, invitee.id), eq(schema.roomMembers.roomId, roomId)))
    .limit(1);
  if (alreadyMember.length > 0) { res.status(409).json({ error: 'User is already a member' }); return; }

  // Check if pending invite already exists
  const existing = await db.select().from(schema.roomInvitations)
    .where(and(
      eq(schema.roomInvitations.roomId, roomId),
      eq(schema.roomInvitations.inviteeId, invitee.id),
      eq(schema.roomInvitations.status, 'pending'),
    )).limit(1);
  if (existing.length > 0) { res.status(409).json({ error: 'Invitation already pending' }); return; }

  const [room] = await db.select().from(schema.rooms).where(eq(schema.rooms.id, roomId)).limit(1);
  const inviter = await db.select().from(schema.users).where(eq(schema.users.id, inviterId)).limit(1);

  const [invitation] = await db.insert(schema.roomInvitations)
    .values({ roomId, inviterId, inviteeId: invitee.id }).returning();

  // Notify invitee in real-time
  io.to(`user:${invitee.id}`).emit('invitation:received', {
    id: invitation.id,
    roomId,
    roomName: room?.name ?? '',
    inviterId,
    inviterName: inviter[0]?.name ?? '',
    status: 'pending',
    createdAt: invitation.createdAt,
  });

  res.json({ success: true, invitationId: invitation.id });
});

// Get pending invitations for a user
app.get('/api/users/:userId/invitations', async (req, res) => {
  const userId = parseInt(req.params.userId);
  const invitations = await db.select({
    id: schema.roomInvitations.id,
    roomId: schema.roomInvitations.roomId,
    inviterId: schema.roomInvitations.inviterId,
    inviteeId: schema.roomInvitations.inviteeId,
    status: schema.roomInvitations.status,
    createdAt: schema.roomInvitations.createdAt,
    roomName: schema.rooms.name,
    inviterName: schema.users.name,
  })
    .from(schema.roomInvitations)
    .innerJoin(schema.rooms, eq(schema.rooms.id, schema.roomInvitations.roomId))
    .innerJoin(schema.users, eq(schema.users.id, schema.roomInvitations.inviterId))
    .where(and(eq(schema.roomInvitations.inviteeId, userId), eq(schema.roomInvitations.status, 'pending')));
  res.json(invitations);
});

// Accept an invitation
app.post('/api/invitations/:id/accept', async (req, res) => {
  const invitationId = parseInt(req.params.id);
  const { userId } = req.body as { userId: number };

  const [inv] = await db.select().from(schema.roomInvitations)
    .where(and(eq(schema.roomInvitations.id, invitationId), eq(schema.roomInvitations.inviteeId, userId)))
    .limit(1);
  if (!inv) { res.status(404).json({ error: 'Invitation not found' }); return; }
  if (inv.status !== 'pending') { res.status(400).json({ error: 'Invitation is not pending' }); return; }

  await db.update(schema.roomInvitations).set({ status: 'accepted' }).where(eq(schema.roomInvitations.id, invitationId));
  await db.insert(schema.roomMembers).values({ userId, roomId: inv.roomId }).onConflictDoNothing();

  const full = await getRoomWithMeta(inv.roomId, userId);

  // Notify existing room members
  io.to(`room:${inv.roomId}`).emit('room:membership', { roomId: inv.roomId, userId, action: 'join' });

  res.json({ room: full });
});

// Decline an invitation
app.post('/api/invitations/:id/decline', async (req, res) => {
  const invitationId = parseInt(req.params.id);
  const { userId } = req.body as { userId: number };

  const [inv] = await db.select().from(schema.roomInvitations)
    .where(and(eq(schema.roomInvitations.id, invitationId), eq(schema.roomInvitations.inviteeId, userId)))
    .limit(1);
  if (!inv) { res.status(404).json({ error: 'Invitation not found' }); return; }

  await db.update(schema.roomInvitations).set({ status: 'declined' }).where(eq(schema.roomInvitations.id, invitationId));
  res.json({ success: true });
});

// Start or get a DM with another user
app.post('/api/dms', async (req, res) => {
  const { userId, targetUserId } = req.body as { userId: number; targetUserId: number };
  if (!userId || !targetUserId || userId === targetUserId) {
    res.status(400).json({ error: 'Invalid user IDs' }); return;
  }

  // Check if DM room already exists (both users are members of a DM room)
  const existing = await db.execute<{ room_id: number }>(sql`
    SELECT rm1.room_id
    FROM room_members rm1
    JOIN room_members rm2 ON rm1.room_id = rm2.room_id
    JOIN rooms r ON rm1.room_id = r.id
    WHERE rm1.user_id = ${userId}
      AND rm2.user_id = ${targetUserId}
      AND r.is_dm = true
    LIMIT 1
  `);

  if (existing.rows.length > 0) {
    const roomId = existing.rows[0].room_id;
    const full = await getRoomWithMeta(roomId, userId);
    res.json(full);
    return;
  }

  // Create new DM room
  const u1 = Math.min(userId, targetUserId);
  const u2 = Math.max(userId, targetUserId);
  const dmName = `__dm_${u1}_${u2}`;

  try {
    const [room] = await db.insert(schema.rooms)
      .values({ name: dmName, createdBy: userId, isPrivate: true, isDm: true })
      .returning();
    await db.insert(schema.roomMembers).values({ userId, roomId: room.id });
    await db.insert(schema.roomMembers).values({ userId: targetUserId, roomId: room.id });

    const full = await getRoomWithMeta(room.id, userId);

    // Notify both users
    io.to(`user:${userId}`).emit('room:created', full);
    io.to(`user:${targetUserId}`).emit('room:created', await getRoomWithMeta(room.id, targetUserId));

    res.json(full);
  } catch (e: any) {
    if (e.code === '23505') {
      // DM already exists (race condition), fetch it
      const retry = await db.execute<{ room_id: number }>(sql`
        SELECT rm1.room_id
        FROM room_members rm1
        JOIN room_members rm2 ON rm1.room_id = rm2.room_id
        JOIN rooms r ON rm1.room_id = r.id
        WHERE rm1.user_id = ${userId}
          AND rm2.user_id = ${targetUserId}
          AND r.is_dm = true
        LIMIT 1
      `);
      if (retry.rows.length > 0) {
        const full = await getRoomWithMeta(retry.rows[0].room_id, userId);
        res.json(full);
        return;
      }
    }
    res.status(500).json({ error: 'Failed to create DM' });
  }
});

// ── Messages ──────────────────────────────────────────────────────────────────
// Helper: load reactions for a set of message IDs
async function getReactionsByMsgIds(msgIds: number[]): Promise<Map<number, { emoji: string; userIds: number[] }[]>> {
  const reactions = await db.select({
    messageId: schema.messageReactions.messageId,
    userId: schema.messageReactions.userId,
    emoji: schema.messageReactions.emoji,
  }).from(schema.messageReactions).where(inArray(schema.messageReactions.messageId, msgIds));

  const map = new Map<number, Map<string, number[]>>();
  for (const r of reactions) {
    let byMsg = map.get(r.messageId);
    if (!byMsg) { byMsg = new Map(); map.set(r.messageId, byMsg); }
    const users = byMsg.get(r.emoji) ?? [];
    users.push(r.userId);
    byMsg.set(r.emoji, users);
  }

  const result = new Map<number, { emoji: string; userIds: number[] }[]>();
  for (const [msgId, byEmoji] of map) {
    result.set(msgId, [...byEmoji.entries()].map(([emoji, userIds]) => ({ emoji, userIds })));
  }
  return result;
}

app.get('/api/rooms/:id/messages', async (req, res) => {
  const roomId = parseInt(req.params.id);
  const now = new Date();
  // Only return root messages (not thread replies)
  const msgs = await db.select().from(schema.messages)
    .where(and(
      eq(schema.messages.roomId, roomId),
      isNull(schema.messages.parentMessageId),
      or(isNull(schema.messages.expiresAt), gt(schema.messages.expiresAt, now))
    ))
    .orderBy(schema.messages.createdAt)
    .limit(200);

  if (msgs.length === 0) { res.json([]); return; }

  const msgIds = msgs.map(m => m.id);
  const reads = await db.select({ messageId: schema.messageReads.messageId, userId: schema.messageReads.userId })
    .from(schema.messageReads)
    .where(inArray(schema.messageReads.messageId, msgIds));

  const readsByMsg = new Map<number, number[]>();
  for (const r of reads) {
    const arr = readsByMsg.get(r.messageId) ?? [];
    arr.push(r.userId);
    readsByMsg.set(r.messageId, arr);
  }

  const reactionsByMsg = await getReactionsByMsgIds(msgIds);

  // Get reply counts and latest preview per root message
  const replyCountMap = new Map<number, number>();
  const replyPreviewMap = new Map<number, string>();
  const replyCounts = await db.select({
    parentMessageId: schema.messages.parentMessageId,
    count: sql<number>`count(*)::int`,
  })
    .from(schema.messages)
    .where(inArray(schema.messages.parentMessageId, msgIds))
    .groupBy(schema.messages.parentMessageId);
  for (const row of replyCounts) {
    if (row.parentMessageId !== null) replyCountMap.set(row.parentMessageId, row.count);
  }
  // Latest reply content per parent (ordered by time, last wins)
  const allReplies = await db.select({
    parentMessageId: schema.messages.parentMessageId,
    content: schema.messages.content,
  })
    .from(schema.messages)
    .where(inArray(schema.messages.parentMessageId, msgIds))
    .orderBy(schema.messages.createdAt);
  for (const r of allReplies) {
    if (r.parentMessageId !== null) replyPreviewMap.set(r.parentMessageId, r.content);
  }

  res.json(msgs.map(m => ({
    ...m,
    readBy: readsByMsg.get(m.id) ?? [],
    reactions: reactionsByMsg.get(m.id) ?? [],
    replyCount: replyCountMap.get(m.id) ?? 0,
    replyPreview: replyPreviewMap.get(m.id) ?? null,
  })));
});

app.post('/api/rooms/:id/messages', async (req, res) => {
  const roomId = parseInt(req.params.id);
  const { userId, content, expiresAfterSeconds, parentMessageId } = req.body as { userId: number; content: string; expiresAfterSeconds?: number; parentMessageId?: number };

  if (!content || typeof content !== 'string') { res.status(400).json({ error: 'Content required' }); return; }
  const trimmed = content.trim().slice(0, 2000);
  if (!trimmed) { res.status(400).json({ error: 'Content cannot be empty' }); return; }

  // Rate limit: 1 message per 500ms per user
  const now = Date.now();
  const last = lastMessageTime.get(userId) ?? 0;
  if (now - last < 500) { res.status(429).json({ error: 'Slow down' }); return; }
  lastMessageTime.set(userId, now);

  const expiresAt = expiresAfterSeconds && expiresAfterSeconds > 0
    ? new Date(Date.now() + expiresAfterSeconds * 1000)
    : undefined;

  const parentId = parentMessageId ?? null;
  const [msg] = await db.insert(schema.messages).values({ roomId, userId, content: trimmed, expiresAt, parentMessageId: parentId }).returning();

  // Auto-mark as read by sender
  await db.insert(schema.messageReads).values({ messageId: msg.id, userId }).onConflictDoNothing();

  const msgWithReads = { ...msg, readBy: [userId], reactions: [], replyCount: 0, replyPreview: null };

  if (parentId !== null) {
    // Thread reply: notify room of new reply (with updated count)
    const [{ count }] = await db.select({ count: sql<number>`count(*)::int` })
      .from(schema.messages)
      .where(eq(schema.messages.parentMessageId, parentId));
    io.to(`room:${roomId}`).emit('thread:reply', {
      parentMessageId: parentId,
      reply: msgWithReads,
      replyCount: count ?? 1,
      replyPreview: trimmed,
    });
  } else {
    // Root message: update last-read and broadcast
    await db.insert(schema.lastReadPositions)
      .values({ userId, roomId, lastMessageId: msg.id })
      .onConflictDoUpdate({
        target: [schema.lastReadPositions.userId, schema.lastReadPositions.roomId],
        set: { lastMessageId: msg.id, updatedAt: sql`now()` },
      });

    io.to(`room:${roomId}`).emit('message:new', msgWithReads);

    // Notify other room members of unread update
    const members = await db.select({ userId: schema.roomMembers.userId })
      .from(schema.roomMembers).where(eq(schema.roomMembers.roomId, roomId));

    for (const member of members) {
      if (member.userId === userId) continue;
      const lastRead = await db.select().from(schema.lastReadPositions)
        .where(and(eq(schema.lastReadPositions.userId, member.userId), eq(schema.lastReadPositions.roomId, roomId)))
        .limit(1);

      let unread = 1;
      if (lastRead.length > 0 && lastRead[0].lastMessageId) {
        const [{ count }] = await db.select({ count: sql<number>`count(*)::int` })
          .from(schema.messages)
          .where(and(eq(schema.messages.roomId, roomId), gt(schema.messages.id, lastRead[0].lastMessageId!), isNull(schema.messages.parentMessageId)));
        unread = count ?? 1;
      }
      io.to(`user:${member.userId}`).emit('unread:update', { roomId, count: unread });
    }
  }

  res.json(msgWithReads);
});

// Mark all messages in room as read
app.post('/api/rooms/:id/read', async (req, res) => {
  const roomId = parseInt(req.params.id);
  const { userId } = req.body as { userId: number };

  const [latestMsg] = await db.select().from(schema.messages)
    .where(eq(schema.messages.roomId, roomId))
    .orderBy(desc(schema.messages.id)).limit(1);

  if (!latestMsg) { res.json({ success: true }); return; }

  const lastRead = await db.select().from(schema.lastReadPositions)
    .where(and(eq(schema.lastReadPositions.userId, userId), eq(schema.lastReadPositions.roomId, roomId)))
    .limit(1);

  let unreadMsgs: { id: number }[] = [];
  if (lastRead.length === 0) {
    unreadMsgs = await db.select({ id: schema.messages.id })
      .from(schema.messages).where(eq(schema.messages.roomId, roomId));
  } else if (lastRead[0].lastMessageId) {
    unreadMsgs = await db.select({ id: schema.messages.id })
      .from(schema.messages)
      .where(and(eq(schema.messages.roomId, roomId), gt(schema.messages.id, lastRead[0].lastMessageId!)));
  }

  if (unreadMsgs.length > 0) {
    await db.insert(schema.messageReads)
      .values(unreadMsgs.map(m => ({ messageId: m.id, userId })))
      .onConflictDoNothing();
    for (const m of unreadMsgs) {
      io.to(`room:${roomId}`).emit('reads:update', { messageId: m.id, userId });
    }
  }

  await db.insert(schema.lastReadPositions)
    .values({ userId, roomId, lastMessageId: latestMsg.id })
    .onConflictDoUpdate({
      target: [schema.lastReadPositions.userId, schema.lastReadPositions.roomId],
      set: { lastMessageId: latestMsg.id, updatedAt: sql`now()` },
    });

  io.to(`user:${userId}`).emit('unread:update', { roomId, count: 0 });
  res.json({ success: true });
});

// ── Reactions ─────────────────────────────────────────────────────────────────
// Toggle reaction: adds if not present, removes if already reacted
app.post('/api/messages/:id/reactions', async (req, res) => {
  const messageId = parseInt(req.params.id);
  const { userId, emoji } = req.body as { userId: number; emoji: string };

  const ALLOWED_EMOJIS = ['👍', '❤️', '😂', '😮', '😢'];
  if (!ALLOWED_EMOJIS.includes(emoji)) { res.status(400).json({ error: 'Invalid emoji' }); return; }

  // Check message exists and get its roomId
  const [msg] = await db.select({ id: schema.messages.id, roomId: schema.messages.roomId })
    .from(schema.messages).where(eq(schema.messages.id, messageId)).limit(1);
  if (!msg) { res.status(404).json({ error: 'Message not found' }); return; }

  const existing = await db.select().from(schema.messageReactions)
    .where(and(
      eq(schema.messageReactions.messageId, messageId),
      eq(schema.messageReactions.userId, userId),
      eq(schema.messageReactions.emoji, emoji),
    )).limit(1);

  if (existing.length > 0) {
    await db.delete(schema.messageReactions).where(and(
      eq(schema.messageReactions.messageId, messageId),
      eq(schema.messageReactions.userId, userId),
      eq(schema.messageReactions.emoji, emoji),
    ));
  } else {
    await db.insert(schema.messageReactions).values({ messageId, userId, emoji }).onConflictDoNothing();
  }

  // Load all reactions for this message and broadcast
  const allReactions = await db.select({
    userId: schema.messageReactions.userId,
    emoji: schema.messageReactions.emoji,
  }).from(schema.messageReactions).where(eq(schema.messageReactions.messageId, messageId));

  const byEmoji = new Map<string, number[]>();
  for (const r of allReactions) {
    const users = byEmoji.get(r.emoji) ?? [];
    users.push(r.userId);
    byEmoji.set(r.emoji, users);
  }
  const reactions = [...byEmoji.entries()].map(([e, userIds]) => ({ emoji: e, userIds }));

  io.to(`room:${msg.roomId}`).emit('reaction:update', { messageId, reactions });
  res.json({ reactions });
});

// ── Message Editing ───────────────────────────────────────────────────────────
app.patch('/api/messages/:id', async (req, res) => {
  const messageId = parseInt(req.params.id);
  const { userId, content } = req.body as { userId: number; content: string };

  if (!content || typeof content !== 'string') { res.status(400).json({ error: 'Content required' }); return; }
  const trimmed = content.trim().slice(0, 2000);
  if (!trimmed) { res.status(400).json({ error: 'Content cannot be empty' }); return; }

  const [msg] = await db.select().from(schema.messages)
    .where(eq(schema.messages.id, messageId)).limit(1);
  if (!msg) { res.status(404).json({ error: 'Message not found' }); return; }
  if (msg.userId !== userId) { res.status(403).json({ error: 'You can only edit your own messages' }); return; }

  // Store previous content as edit history
  await db.insert(schema.messageEdits).values({
    messageId,
    userId,
    previousContent: msg.content,
  });

  const now = new Date();
  const [updated] = await db.update(schema.messages)
    .set({ content: trimmed, editedAt: now, isEdited: true })
    .where(eq(schema.messages.id, messageId))
    .returning();

  io.to(`room:${msg.roomId}`).emit('message:edited', {
    messageId,
    content: trimmed,
    editedAt: now.toISOString(),
  });

  res.json(updated);
});

app.get('/api/messages/:id/edits', async (req, res) => {
  const messageId = parseInt(req.params.id);
  const edits = await db.select().from(schema.messageEdits)
    .where(eq(schema.messageEdits.messageId, messageId))
    .orderBy(schema.messageEdits.editedAt);
  res.json(edits);
});

// ── Threading ─────────────────────────────────────────────────────────────────
app.get('/api/messages/:id/thread', async (req, res) => {
  const messageId = parseInt(req.params.id);
  const replies = await db.select().from(schema.messages)
    .where(eq(schema.messages.parentMessageId, messageId))
    .orderBy(schema.messages.createdAt)
    .limit(200);

  if (replies.length === 0) { res.json([]); return; }

  const replyIds = replies.map(r => r.id);
  const reads = await db.select({ messageId: schema.messageReads.messageId, userId: schema.messageReads.userId })
    .from(schema.messageReads)
    .where(inArray(schema.messageReads.messageId, replyIds));

  const readsByMsg = new Map<number, number[]>();
  for (const r of reads) {
    const arr = readsByMsg.get(r.messageId) ?? [];
    arr.push(r.userId);
    readsByMsg.set(r.messageId, arr);
  }

  const reactionsByMsg = await getReactionsByMsgIds(replyIds);

  res.json(replies.map(r => ({
    ...r,
    readBy: readsByMsg.get(r.id) ?? [],
    reactions: reactionsByMsg.get(r.id) ?? [],
    replyCount: 0,
    replyPreview: null,
  })));
});

// ── Scheduled Messages ────────────────────────────────────────────────────────
app.post('/api/rooms/:id/scheduled', async (req, res) => {
  const roomId = parseInt(req.params.id);
  const { userId, content, scheduledFor } = req.body as { userId: number; content: string; scheduledFor: string };

  if (!content || !scheduledFor) { res.status(400).json({ error: 'Content and scheduledFor required' }); return; }
  const trimmed = content.trim().slice(0, 2000);
  if (!trimmed) { res.status(400).json({ error: 'Content cannot be empty' }); return; }

  const scheduledDate = new Date(scheduledFor);
  if (isNaN(scheduledDate.getTime()) || scheduledDate <= new Date()) {
    res.status(400).json({ error: 'scheduledFor must be a future date' });
    return;
  }

  try {
    const [msg] = await db.insert(schema.scheduledMessages)
      .values({ roomId, userId, content: trimmed, scheduledFor: scheduledDate })
      .returning();
    res.json(msg);
  } catch {
    res.status(500).json({ error: 'Failed to schedule message' });
  }
});

app.get('/api/users/:userId/scheduled', async (req, res) => {
  const userId = parseInt(req.params.userId);
  const msgs = await db.select().from(schema.scheduledMessages)
    .where(and(eq(schema.scheduledMessages.userId, userId), eq(schema.scheduledMessages.status, 'pending')))
    .orderBy(schema.scheduledMessages.scheduledFor);
  res.json(msgs);
});

app.delete('/api/scheduled/:id', async (req, res) => {
  const id = parseInt(req.params.id);
  const { userId } = req.body as { userId: number };

  const [msg] = await db.select().from(schema.scheduledMessages)
    .where(and(eq(schema.scheduledMessages.id, id), eq(schema.scheduledMessages.userId, userId)))
    .limit(1);

  if (!msg) { res.status(404).json({ error: 'Not found' }); return; }
  if (msg.status !== 'pending') { res.status(400).json({ error: 'Message is not pending' }); return; }

  await db.update(schema.scheduledMessages)
    .set({ status: 'cancelled' })
    .where(eq(schema.scheduledMessages.id, id));

  res.json({ success: true });
});

// Background scheduler: check for due messages every 5 seconds
setInterval(async () => {
  try {
    const now = new Date();
    const due = await db.select().from(schema.scheduledMessages)
      .where(and(eq(schema.scheduledMessages.status, 'pending'), lt(schema.scheduledMessages.scheduledFor, now)));

    for (const scheduled of due) {
      // Mark as sent first to prevent double-delivery
      const updated = await db.update(schema.scheduledMessages)
        .set({ status: 'sent' })
        .where(and(eq(schema.scheduledMessages.id, scheduled.id), eq(schema.scheduledMessages.status, 'pending')))
        .returning();
      if (updated.length === 0) continue; // Already handled

      const [msg] = await db.insert(schema.messages)
        .values({ roomId: scheduled.roomId, userId: scheduled.userId, content: scheduled.content })
        .returning();

      await db.insert(schema.messageReads).values({ messageId: msg.id, userId: scheduled.userId }).onConflictDoNothing();
      await db.insert(schema.lastReadPositions)
        .values({ userId: scheduled.userId, roomId: scheduled.roomId, lastMessageId: msg.id })
        .onConflictDoUpdate({
          target: [schema.lastReadPositions.userId, schema.lastReadPositions.roomId],
          set: { lastMessageId: msg.id, updatedAt: sql`now()` },
        });

      const msgWithReads = { ...msg, readBy: [scheduled.userId], reactions: [] };
      io.to(`room:${scheduled.roomId}`).emit('message:new', msgWithReads);
      io.to(`user:${scheduled.userId}`).emit('scheduled:sent', { id: scheduled.id });

      const members = await db.select({ userId: schema.roomMembers.userId })
        .from(schema.roomMembers).where(eq(schema.roomMembers.roomId, scheduled.roomId));

      for (const member of members) {
        if (member.userId === scheduled.userId) continue;
        const lastRead = await db.select().from(schema.lastReadPositions)
          .where(and(eq(schema.lastReadPositions.userId, member.userId), eq(schema.lastReadPositions.roomId, scheduled.roomId)))
          .limit(1);
        let unread = 1;
        if (lastRead.length > 0 && lastRead[0].lastMessageId) {
          const [{ count }] = await db.select({ count: sql<number>`count(*)::int` })
            .from(schema.messages)
            .where(and(eq(schema.messages.roomId, scheduled.roomId), gt(schema.messages.id, lastRead[0].lastMessageId!)));
          unread = count ?? 1;
        }
        io.to(`user:${member.userId}`).emit('unread:update', { roomId: scheduled.roomId, count: unread });
      }
    }
  } catch (e) {
    console.error('Scheduler error:', e);
  }
}, 5000);

// Background: delete expired ephemeral messages every 5 seconds
setInterval(async () => {
  try {
    const now = new Date();
    const expired = await db.select({ id: schema.messages.id, roomId: schema.messages.roomId })
      .from(schema.messages)
      .where(and(
        lte(schema.messages.expiresAt, now),
      ));

    for (const msg of expired) {
      // Delete thread replies first (FK), then root message
      const threadReplies = await db.select({ id: schema.messages.id })
        .from(schema.messages).where(eq(schema.messages.parentMessageId, msg.id));
      for (const reply of threadReplies) {
        await db.delete(schema.messageReads).where(eq(schema.messageReads.messageId, reply.id));
        await db.delete(schema.messageReactions).where(eq(schema.messageReactions.messageId, reply.id));
        await db.delete(schema.messageEdits).where(eq(schema.messageEdits.messageId, reply.id));
        await db.delete(schema.messages).where(eq(schema.messages.id, reply.id));
      }
      // Delete reads, reactions, edits first (FK), then message
      await db.delete(schema.messageReads).where(eq(schema.messageReads.messageId, msg.id));
      await db.delete(schema.messageReactions).where(eq(schema.messageReactions.messageId, msg.id));
      await db.delete(schema.messageEdits).where(eq(schema.messageEdits.messageId, msg.id));
      const deleted = await db.delete(schema.messages).where(eq(schema.messages.id, msg.id)).returning();
      if (deleted.length > 0) {
        io.to(`room:${msg.roomId}`).emit('message:deleted', { messageId: msg.id, roomId: msg.roomId });
      }
    }
  } catch (e) {
    console.error('Ephemeral cleanup error:', e);
  }
}, 5000);

// ── Socket.io ─────────────────────────────────────────────────────────────────
io.on('connection', (socket) => {
  socket.on('user:online', async ({ userId }: { userId: number }) => {
    onlineUsers.set(socket.id, userId);
    socket.join(`user:${userId}`);

    if (!userNameCache.has(userId)) {
      const [user] = await db.select().from(schema.users).where(eq(schema.users.id, userId)).limit(1);
      if (user) userNameCache.set(userId, user.name);
    }

    // Mark user as online and update lastActiveAt
    const now = new Date();
    const [updated] = await db.update(schema.users)
      .set({ lastActiveAt: now })
      .where(eq(schema.users.id, userId))
      .returning({ id: schema.users.id, status: schema.users.status, lastActiveAt: schema.users.lastActiveAt });
    if (updated) {
      io.emit('user:status', { userId, status: updated.status, lastActiveAt: updated.lastActiveAt });
    }

    io.emit('users:online', getOnlineUserIds());
  });

  socket.on('user:activity', async ({ userId }: { userId: number }) => {
    // Update lastActiveAt and reset away status if user was away
    const now = new Date();
    const [user] = await db.select({ status: schema.users.status })
      .from(schema.users).where(eq(schema.users.id, userId)).limit(1);
    if (!user) return;

    const newStatus = user.status === 'away' ? 'online' : user.status;
    const [updated] = await db.update(schema.users)
      .set({ lastActiveAt: now, status: newStatus })
      .where(eq(schema.users.id, userId))
      .returning({ id: schema.users.id, status: schema.users.status, lastActiveAt: schema.users.lastActiveAt });
    if (updated && newStatus !== user.status) {
      io.emit('user:status', { userId, status: updated.status, lastActiveAt: updated.lastActiveAt });
    }
  });

  socket.on('room:subscribe', ({ roomId }: { roomId: number }) => {
    socket.join(`room:${roomId}`);
  });

  socket.on('room:unsubscribe', ({ roomId }: { roomId: number }) => {
    socket.leave(`room:${roomId}`);
  });

  socket.on('typing:start', ({ roomId, userId }: { roomId: number; userId: number }) => {
    const key = `${roomId}:${userId}`;
    const existing = typingTimers.get(key);
    if (existing) clearTimeout(existing);

    let typing = typingUsers.get(roomId);
    if (!typing) { typing = new Set(); typingUsers.set(roomId, typing); }
    typing.add(userId);

    const names = [...typing].map(uid => userNameCache.get(uid) ?? 'Someone');
    io.to(`room:${roomId}`).emit('typing:update', { roomId, typingUserIds: [...typing], typingNames: names });

    const timer = setTimeout(() => {
      const t = typingUsers.get(roomId);
      if (t) {
        t.delete(userId);
        const names = [...t].map(uid => userNameCache.get(uid) ?? 'Someone');
        io.to(`room:${roomId}`).emit('typing:update', { roomId, typingUserIds: [...t], typingNames: names });
      }
      typingTimers.delete(key);
    }, 3000);

    typingTimers.set(key, timer);
  });

  socket.on('typing:stop', ({ roomId, userId }: { roomId: number; userId: number }) => {
    const key = `${roomId}:${userId}`;
    const existing = typingTimers.get(key);
    if (existing) { clearTimeout(existing); typingTimers.delete(key); }

    const typing = typingUsers.get(roomId);
    if (typing) {
      typing.delete(userId);
      const names = [...typing].map(uid => userNameCache.get(uid) ?? 'Someone');
      io.to(`room:${roomId}`).emit('typing:update', { roomId, typingUserIds: [...typing], typingNames: names });
    }
  });

  socket.on('disconnect', async () => {
    const userId = onlineUsers.get(socket.id);
    onlineUsers.delete(socket.id);
    io.emit('users:online', getOnlineUserIds());

    if (userId) {
      for (const [roomId, typing] of typingUsers) {
        if (typing.has(userId)) {
          typing.delete(userId);
          const names = [...typing].map(uid => userNameCache.get(uid) ?? 'Someone');
          io.to(`room:${roomId}`).emit('typing:update', { roomId, typingUserIds: [...typing], typingNames: names });
        }
      }

      // Update lastActiveAt on disconnect
      const now = new Date();
      await db.update(schema.users)
        .set({ lastActiveAt: now })
        .where(eq(schema.users.id, userId));
      io.emit('user:status', { userId, status: 'offline', lastActiveAt: now });
    }
  });
});

const PORT = parseInt(process.env.PORT || '3001');
httpServer.listen(PORT, () => {
  console.log(`Server running on port ${PORT}`);
});

import 'dotenv/config';
import express, { Request, Response } from 'express';
import { createServer } from 'http';
import { Server } from 'socket.io';
import cors from 'cors';
import mongoose from 'mongoose';
import { User, Room, Message, ScheduledMessage, Invitation, Draft } from './models.js';

const app = express();
const httpServer = createServer(app);

const io = new Server(httpServer, {
  cors: {
    origin: 'http://localhost:6373',
    methods: ['GET', 'POST'],
  },
  // Increase per-socket buffer to avoid dropped events under burst load
  perMessageDeflate: false,
});

app.use(cors({ origin: 'http://localhost:6373' }));
// Use a larger JSON body limit only if needed; default 100kb is fine for chat
app.use(express.json());

const DB_URL = process.env.DATABASE_URL ?? 'mongodb://localhost:6437/chat-app';
await mongoose.connect(DB_URL, {
  // Allow more concurrent operations under heavy load
  maxPoolSize: 20,
  minPoolSize: 5,
  // Reduce the time spent waiting for a connection from the pool
  serverSelectionTimeoutMS: 5000,
  socketTimeoutMS: 45000,
});
console.log('Connected to MongoDB');

// roomId -> { userName -> timeout }
const typingTimers = new Map<string, Map<string, ReturnType<typeof setTimeout>>>();

// userName -> Set of active socketIds (tracks multi-tab presence)
const userSockets = new Map<string, Set<string>>();

// roomId -> array of recent message timestamps (for activity tracking)
// We keep this as a plain number[] (ms since epoch) instead of Date[] to avoid
// object allocation on every push and comparison.
const roomActivityTimestamps = new Map<string, number[]>();
// roomId -> last emitted activity level (to detect changes during decay)
const lastEmittedActivityLevel = new Map<string, 'hot' | 'active' | ''>();

function getActivityLevel(roomId: string): 'hot' | 'active' | '' {
  const timestamps = roomActivityTimestamps.get(roomId);
  if (!timestamps || timestamps.length === 0) return '';
  const now = Date.now();
  const cutoff5 = now - 5 * 60 * 1000;
  const cutoff2 = now - 2 * 60 * 1000;
  let count5 = 0;
  let count2 = 0;
  for (let i = 0; i < timestamps.length; i++) {
    const t = timestamps[i];
    if (t >= cutoff5) {
      count5++;
      if (t >= cutoff2) count2++;
    }
  }
  if (count5 >= 5) return 'hot';
  if (count2 >= 1) return 'active';
  return '';
}

function trackMessageActivity(roomId: string): void {
  let timestamps = roomActivityTimestamps.get(roomId);
  if (!timestamps) {
    timestamps = [];
    roomActivityTimestamps.set(roomId, timestamps);
  }
  const now = Date.now();
  timestamps.push(now);
  // Lazy trim: only prune when array grows large (> 50 entries) to amortize allocation cost
  if (timestamps.length > 50) {
    const cutoff = now - 10 * 60 * 1000;
    let start = 0;
    while (start < timestamps.length && timestamps[start] < cutoff) start++;
    if (start > 0) timestamps.splice(0, start);
  }
  const level = getActivityLevel(roomId);
  lastEmittedActivityLevel.set(roomId, level);
  // Emit asynchronously — do not block the HTTP response path
  setImmediate(() => {
    io.emit('room-activity', { roomId, level });
  });
}

function clearTyping(roomId: string, userName: string): void {
  const roomMap = typingTimers.get(roomId);
  if (!roomMap) return;
  const timer = roomMap.get(userName);
  if (timer !== undefined) {
    clearTimeout(timer);
    roomMap.delete(userName);
  }
}

function broadcastTyping(roomId: string): void {
  const roomMap = typingTimers.get(roomId);
  const users = roomMap ? [...roomMap.keys()] : [];
  io.to(roomId).emit('typing-update', { roomId, typingUsers: users });
}

function emitToUsers(users: string[], event: string, data: unknown): void {
  for (const user of users) {
    const sockets = userSockets.get(user);
    if (sockets) {
      for (const socketId of sockets) {
        io.to(socketId).emit(event, data);
      }
    }
  }
}

function emitRoomUpdated(room: { _id: mongoose.Types.ObjectId | string; members: string[]; isPrivate?: boolean; isDM?: boolean }, data: unknown): void {
  if (room.isPrivate || room.isDM) {
    emitToUsers(room.members, 'room-updated', data);
  } else {
    io.emit('room-updated', data);
  }
}

app.get('/api/health', (_req: Request, res: Response): void => {
  res.json({ ok: true });
});

app.get('/api/rooms/activity', (_req: Request, res: Response): void => {
  const activity: Record<string, 'hot' | 'active' | ''> = {};
  for (const [roomId] of roomActivityTimestamps.entries()) {
    const level = getActivityLevel(roomId);
    if (level) activity[roomId] = level;
  }
  res.json({ activity });
});

app.post('/api/users', async (req: Request, res: Response): Promise<void> => {
  const raw = req.body?.name;
  const name = typeof raw === 'string' ? raw.trim().slice(0, 32) : '';
  if (!name) {
    res.status(400).json({ error: 'Name is required (max 32 chars)' });
    return;
  }
  try {
    let user = await User.findOne({ name });
    if (!user) {
      user = await User.create({ name });
    }
    res.json({ user: { id: user._id, name: user.name } });
  } catch (err: unknown) {
    const mongoErr = err as { code?: number };
    if (mongoErr.code === 11000) {
      const user = await User.findOne({ name });
      res.json({ user: { id: user!._id, name: user!.name } });
    } else {
      res.status(500).json({ error: 'Server error' });
    }
  }
});

app.get('/api/users', async (_req: Request, res: Response): Promise<void> => {
  const users = await User.find({}).select('name status lastSeen online');
  res.json({ users });
});

app.patch('/api/users/:userName/status', async (req: Request, res: Response): Promise<void> => {
  const { status } = req.body;
  const validStatuses = ['online', 'away', 'dnd', 'invisible'];
  if (!validStatuses.includes(status)) {
    res.status(400).json({ error: 'Invalid status' });
    return;
  }
  const updateFields: { status: string; lastSeen?: Date } = { status };
  if (status === 'away' || status === 'invisible') updateFields.lastSeen = new Date();
  const user = await User.findOneAndUpdate(
    { name: req.params.userName },
    updateFields,
    { new: true }
  );
  if (!user) { res.status(404).json({ error: 'User not found' }); return; }
  const allUsers = await User.find({}).select('name status lastSeen online');
  io.emit('online-users', { users: allUsers });
  res.json({ user });
});

app.get('/api/rooms', async (req: Request, res: Response): Promise<void> => {
  const userName = typeof req.query.userName === 'string' ? req.query.userName.trim() : '';
  let rooms;
  if (userName) {
    rooms = await Room.find({
      $or: [
        { isPrivate: false, isDM: { $ne: true } },
        { members: userName },
      ],
    }).sort({ createdAt: 1 });
  } else {
    rooms = await Room.find({ isPrivate: { $ne: true }, isDM: { $ne: true } }).sort({ createdAt: 1 });
  }
  res.json({ rooms });
});

app.post('/api/rooms', async (req: Request, res: Response): Promise<void> => {
  const name = typeof req.body?.name === 'string' ? req.body.name.trim().slice(0, 64) : '';
  const createdBy = typeof req.body?.createdBy === 'string' ? req.body.createdBy.trim() : '';
  const isPrivate = req.body?.isPrivate === true;
  if (!name || !createdBy) {
    res.status(400).json({ error: 'name and createdBy are required' });
    return;
  }
  try {
    const room = await Room.create({ name, createdBy, members: [createdBy], admins: [createdBy], isPrivate });
    if (isPrivate) {
      emitToUsers([createdBy], 'room-created', { room });
    } else {
      io.emit('room-created', { room });
    }
    res.json({ room });
  } catch (err: unknown) {
    const mongoErr = err as { code?: number };
    if (mongoErr.code === 11000) {
      res.status(409).json({ error: 'Room name already taken' });
    } else {
      res.status(500).json({ error: 'Server error' });
    }
  }
});

app.post('/api/rooms/:roomId/join', async (req: Request, res: Response): Promise<void> => {
  const userName = typeof req.body?.userName === 'string' ? req.body.userName.trim() : '';
  if (!userName) { res.status(400).json({ error: 'userName required' }); return; }
  const existing = await Room.findById(req.params.roomId);
  if (!existing) { res.status(404).json({ error: 'Room not found' }); return; }
  if ((existing.banned ?? []).includes(userName)) {
    res.status(403).json({ error: 'You have been banned from this room' });
    return;
  }
  if (existing.isPrivate || existing.isDM) {
    res.status(403).json({ error: 'This is a private room. Request an invitation.' });
    return;
  }
  const room = await Room.findByIdAndUpdate(
    req.params.roomId,
    { $addToSet: { members: userName } },
    { new: true }
  );
  io.emit('room-updated', { room });
  res.json({ room });
});

app.post('/api/rooms/:roomId/leave', async (req: Request, res: Response): Promise<void> => {
  const userName = typeof req.body?.userName === 'string' ? req.body.userName.trim() : '';
  if (!userName) { res.status(400).json({ error: 'userName required' }); return; }
  const room = await Room.findByIdAndUpdate(
    req.params.roomId,
    { $pull: { members: userName } },
    { new: true }
  );
  if (!room) { res.status(404).json({ error: 'Room not found' }); return; }
  emitRoomUpdated(room, { room });
  res.json({ room });
});

app.get('/api/rooms/:roomId/messages', async (req: Request, res: Response): Promise<void> => {
  // Use lean() to skip Mongoose hydration for a read-only endpoint — returns plain JS objects,
  // which is significantly faster under load than full Document instances.
  const messages = await Message.find({ roomId: req.params.roomId, parentId: null })
    .sort({ createdAt: 1 })
    .limit(100)
    .lean();
  res.json({ messages });
});

app.post('/api/rooms/:roomId/messages', async (req: Request, res: Response): Promise<void> => {
  const sender = typeof req.body?.sender === 'string' ? req.body.sender.trim() : '';
  const text = typeof req.body?.text === 'string' ? req.body.text.trim().slice(0, 2000) : '';
  if (!sender || !text) {
    res.status(400).json({ error: 'sender and text are required' });
    return;
  }
  const ttlSecondsRaw = req.body?.ttlSeconds;
  const ttlSeconds = typeof ttlSecondsRaw === 'number' && ttlSecondsRaw > 0 ? Math.min(ttlSecondsRaw, 86400) : null;
  const expiresAt = ttlSeconds ? new Date(Date.now() + ttlSeconds * 1000) : undefined;
  const msg = await Message.create({
    roomId: req.params.roomId,
    sender,
    text,
    readBy: [sender],
    ...(expiresAt ? { expiresAt } : {}),
  });

  // Respond to the HTTP client immediately before broadcasting — this minimises
  // measured POST latency since the client only needs the created message object.
  res.json({ message: msg });

  // Broadcast and activity tracking happen after the response is flushed.
  // setImmediate defers until the current I/O event completes, ensuring the
  // response write is queued to the socket first.
  setImmediate(() => {
    io.to(req.params.roomId).emit('message', { message: msg });
    trackMessageActivity(req.params.roomId);
  });
});

app.post('/api/rooms/:roomId/read', async (req: Request, res: Response): Promise<void> => {
  const userName = typeof req.body?.userName === 'string' ? req.body.userName.trim() : '';
  if (!userName) { res.status(400).json({ error: 'userName required' }); return; }
  const roomId = req.params.roomId;
  await Message.updateMany(
    { roomId, readBy: { $ne: userName } },
    { $addToSet: { readBy: userName } }
  );
  const messages = await Message.find({ roomId, parentId: null }).sort({ createdAt: 1 }).limit(100);
  io.to(roomId).emit('read-receipts-updated', { roomId, messages });
  res.json({ ok: true });
});

app.get('/api/rooms/:roomId/unread', async (req: Request, res: Response): Promise<void> => {
  const userName = req.query.userName;
  if (typeof userName !== 'string' || !userName) {
    res.status(400).json({ error: 'userName query param required' });
    return;
  }
  const count = await Message.countDocuments({
    roomId: req.params.roomId,
    sender: { $ne: userName },
    readBy: { $ne: userName },
  });
  res.json({ count });
});

app.post('/api/rooms/:roomId/scheduled', async (req: Request, res: Response): Promise<void> => {
  const sender = typeof req.body?.sender === 'string' ? req.body.sender.trim() : '';
  const text = typeof req.body?.text === 'string' ? req.body.text.trim().slice(0, 2000) : '';
  const scheduledAtRaw = req.body?.scheduledAt;
  if (!sender || !text || !scheduledAtRaw) {
    res.status(400).json({ error: 'sender, text, and scheduledAt are required' });
    return;
  }
  const scheduledAt = new Date(scheduledAtRaw as string);
  if (isNaN(scheduledAt.getTime()) || scheduledAt <= new Date()) {
    res.status(400).json({ error: 'scheduledAt must be a future date' });
    return;
  }
  const scheduled = await ScheduledMessage.create({ roomId: req.params.roomId, sender, text, scheduledAt });
  res.json({ scheduled });
});

app.get('/api/rooms/:roomId/scheduled', async (req: Request, res: Response): Promise<void> => {
  const userName = req.query.userName;
  if (typeof userName !== 'string' || !userName) {
    res.status(400).json({ error: 'userName query param required' });
    return;
  }
  const scheduled = await ScheduledMessage.find({
    roomId: req.params.roomId,
    sender: userName,
    sent: false,
  }).sort({ scheduledAt: 1 });
  res.json({ scheduled });
});

app.delete('/api/scheduled/:id', async (req: Request, res: Response): Promise<void> => {
  await ScheduledMessage.findByIdAndDelete(req.params.id);
  res.json({ ok: true });
});

app.patch('/api/messages/:messageId', async (req: Request, res: Response): Promise<void> => {
  const userName = typeof req.body?.userName === 'string' ? req.body.userName.trim() : '';
  const newText = typeof req.body?.text === 'string' ? req.body.text.trim().slice(0, 2000) : '';
  if (!userName || !newText) {
    res.status(400).json({ error: 'userName and text are required' });
    return;
  }
  const msg = await Message.findById(req.params.messageId);
  if (!msg) { res.status(404).json({ error: 'Message not found' }); return; }
  if (msg.sender !== userName) { res.status(403).json({ error: 'Cannot edit another user\'s message' }); return; }
  msg.editHistory.push({ text: msg.text, editedAt: new Date() });
  msg.text = newText;
  msg.isEdited = true;
  await msg.save();
  io.to(msg.roomId.toString()).emit('message-updated', { message: msg });
  res.json({ message: msg });
});

app.post('/api/rooms/:roomId/kick', async (req: Request, res: Response): Promise<void> => {
  const adminUser = typeof req.body?.adminUser === 'string' ? req.body.adminUser.trim() : '';
  const targetUser = typeof req.body?.targetUser === 'string' ? req.body.targetUser.trim() : '';
  if (!adminUser || !targetUser) {
    res.status(400).json({ error: 'adminUser and targetUser are required' });
    return;
  }
  const room = await Room.findById(req.params.roomId);
  if (!room) { res.status(404).json({ error: 'Room not found' }); return; }
  if (!(room.admins ?? []).includes(adminUser)) { res.status(403).json({ error: 'Not an admin' }); return; }
  if ((room.admins ?? []).includes(targetUser)) { res.status(400).json({ error: 'Cannot kick an admin' }); return; }

  room.members = room.members.filter((m) => m !== targetUser);
  if (!(room.banned ?? []).includes(targetUser)) room.banned.push(targetUser);
  await room.save();

  const kickedSockets = userSockets.get(targetUser);
  if (kickedSockets) {
    for (const socketId of kickedSockets) {
      const kickedSocket = io.sockets.sockets.get(socketId);
      if (kickedSocket) {
        kickedSocket.leave(req.params.roomId);
        kickedSocket.emit('kicked-from-room', { roomId: req.params.roomId, roomName: room.name });
      }
    }
  }

  emitRoomUpdated(room, { room });
  res.json({ room });
});

app.post('/api/rooms/:roomId/promote', async (req: Request, res: Response): Promise<void> => {
  const adminUser = typeof req.body?.adminUser === 'string' ? req.body.adminUser.trim() : '';
  const targetUser = typeof req.body?.targetUser === 'string' ? req.body.targetUser.trim() : '';
  if (!adminUser || !targetUser) {
    res.status(400).json({ error: 'adminUser and targetUser are required' });
    return;
  }
  const room = await Room.findById(req.params.roomId);
  if (!room) { res.status(404).json({ error: 'Room not found' }); return; }
  if (!(room.admins ?? []).includes(adminUser)) { res.status(403).json({ error: 'Not an admin' }); return; }
  if (!(room.admins ?? []).includes(targetUser)) room.admins.push(targetUser);
  await room.save();

  emitRoomUpdated(room, { room });
  res.json({ room });
});

app.get('/api/messages/:messageId/thread', async (req: Request, res: Response): Promise<void> => {
  const replies = await Message.find({ parentId: req.params.messageId }).sort({ createdAt: 1 });
  res.json({ replies });
});

app.post('/api/messages/:messageId/reply', async (req: Request, res: Response): Promise<void> => {
  const sender = typeof req.body?.sender === 'string' ? req.body.sender.trim() : '';
  const text = typeof req.body?.text === 'string' ? req.body.text.trim().slice(0, 2000) : '';
  if (!sender || !text) { res.status(400).json({ error: 'sender and text are required' }); return; }
  const parent = await Message.findById(req.params.messageId);
  if (!parent) { res.status(404).json({ error: 'Message not found' }); return; }
  const reply = await Message.create({
    roomId: parent.roomId,
    sender,
    text,
    readBy: [sender],
    parentId: parent._id,
  });
  parent.replyCount = (parent.replyCount ?? 0) + 1;
  parent.lastReplyPreview = text.slice(0, 100);
  parent.lastReplySender = sender;
  await parent.save();
  const roomId = parent.roomId.toString();
  io.to(roomId).emit('thread-updated', {
    parentId: parent._id.toString(),
    replyCount: parent.replyCount,
    lastReplyPreview: parent.lastReplyPreview,
    lastReplySender: parent.lastReplySender,
  });
  io.to(`thread-${parent._id.toString()}`).emit('thread-reply', { reply });
  res.json({ reply });
});

app.post('/api/messages/:messageId/react', async (req: Request, res: Response): Promise<void> => {
  const userName = typeof req.body?.userName === 'string' ? req.body.userName.trim() : '';
  const emoji = typeof req.body?.emoji === 'string' ? req.body.emoji.trim() : '';
  if (!userName || !emoji) {
    res.status(400).json({ error: 'userName and emoji are required' });
    return;
  }
  const msg = await Message.findById(req.params.messageId);
  if (!msg) { res.status(404).json({ error: 'Message not found' }); return; }

  const entry = msg.reactions.find((r) => r.emoji === emoji);
  if (entry) {
    const idx = entry.users.indexOf(userName);
    if (idx >= 0) entry.users.splice(idx, 1);
    else entry.users.push(userName);
  } else {
    msg.reactions.push({ emoji, users: [userName] });
  }
  msg.reactions = msg.reactions.filter((r) => r.users.length > 0);
  await msg.save();
  io.to(msg.roomId.toString()).emit('reaction-updated', { message: msg });
  res.json({ message: msg });
});

// Private room invitation endpoints
app.post('/api/rooms/:roomId/invite', async (req: Request, res: Response): Promise<void> => {
  const invitedBy = typeof req.body?.invitedBy === 'string' ? req.body.invitedBy.trim() : '';
  const invitedUser = typeof req.body?.invitedUser === 'string' ? req.body.invitedUser.trim() : '';
  if (!invitedBy || !invitedUser) {
    res.status(400).json({ error: 'invitedBy and invitedUser are required' });
    return;
  }
  if (invitedBy === invitedUser) {
    res.status(400).json({ error: 'Cannot invite yourself' });
    return;
  }
  const room = await Room.findById(req.params.roomId);
  if (!room) { res.status(404).json({ error: 'Room not found' }); return; }
  if (!room.members.includes(invitedBy)) { res.status(403).json({ error: 'Not a member of this room' }); return; }
  if (room.members.includes(invitedUser)) { res.status(400).json({ error: 'User is already a member' }); return; }

  const target = await User.findOne({ name: invitedUser });
  if (!target) { res.status(404).json({ error: 'User not found' }); return; }

  const existing = await Invitation.findOne({ roomId: room._id, invitedUser, status: 'pending' });
  if (existing) { res.status(400).json({ error: 'User already has a pending invitation' }); return; }

  const invitation = await Invitation.create({
    roomId: room._id,
    roomName: room.isDM ? invitedBy : room.name,
    invitedUser,
    invitedBy,
  });

  emitToUsers([invitedUser], 'invitation-received', { invitation });
  res.json({ invitation });
});

app.get('/api/invitations', async (req: Request, res: Response): Promise<void> => {
  const userName = typeof req.query.userName === 'string' ? req.query.userName.trim() : '';
  if (!userName) { res.status(400).json({ error: 'userName required' }); return; }
  const invitations = await Invitation.find({ invitedUser: userName, status: 'pending' }).sort({ createdAt: -1 });
  res.json({ invitations });
});

app.post('/api/invitations/:id/accept', async (req: Request, res: Response): Promise<void> => {
  const invitation = await Invitation.findById(req.params.id);
  if (!invitation) { res.status(404).json({ error: 'Invitation not found' }); return; }
  if (invitation.status !== 'pending') { res.status(400).json({ error: 'Invitation already processed' }); return; }

  const room = await Room.findByIdAndUpdate(
    invitation.roomId,
    { $addToSet: { members: invitation.invitedUser } },
    { new: true }
  );
  if (!room) { res.status(404).json({ error: 'Room not found' }); return; }

  invitation.status = 'accepted';
  await invitation.save();

  // Auto-join the accepted user's sockets to the room and notify them
  const userSocketIds = userSockets.get(invitation.invitedUser);
  if (userSocketIds) {
    for (const sid of userSocketIds) {
      const sock = io.sockets.sockets.get(sid);
      if (sock) {
        sock.join(room._id.toString());
        sock.emit('room-accessible', { room });
      }
    }
  }

  // Notify all members (who are in the room socket channel) of the updated member list
  io.to(room._id.toString()).emit('room-updated', { room });

  res.json({ room });
});

app.post('/api/invitations/:id/decline', async (req: Request, res: Response): Promise<void> => {
  const invitation = await Invitation.findByIdAndUpdate(
    req.params.id,
    { status: 'declined' },
    { new: true }
  );
  if (!invitation) { res.status(404).json({ error: 'Invitation not found' }); return; }
  res.json({ ok: true });
});

// Create or retrieve a DM room between two users
app.post('/api/dm', async (req: Request, res: Response): Promise<void> => {
  const user1 = typeof req.body?.user1 === 'string' ? req.body.user1.trim() : '';
  const user2 = typeof req.body?.user2 === 'string' ? req.body.user2.trim() : '';
  if (!user1 || !user2 || user1 === user2) {
    res.status(400).json({ error: 'user1 and user2 are required and must be different' });
    return;
  }
  const dmUsers = [user1, user2].sort();
  const dmName = `__dm__${dmUsers[0]}__${dmUsers[1]}`;

  let room = await Room.findOne({ name: dmName });
  if (!room) {
    room = await Room.create({
      name: dmName,
      createdBy: user1,
      members: dmUsers,
      admins: [],
      isPrivate: true,
      isDM: true,
      dmUsers,
    });
    // Notify both users about the new DM room
    emitToUsers(dmUsers, 'room-created', { room });
  }

  // Auto-join both users' sockets to the DM room socket channel
  for (const user of dmUsers) {
    const sockets = userSockets.get(user);
    if (sockets) {
      for (const sid of sockets) {
        const sock = io.sockets.sockets.get(sid);
        if (sock) sock.join(room._id.toString());
      }
    }
  }

  res.json({ room });
});

async function generateAnonName(): Promise<string> {
  const chars = 'ABCDEFGHJKLMNPQRSTUVWXYZabcdefghjkmnpqrstuvwxyz23456789';
  let name: string;
  let exists: boolean;
  do {
    let suffix = '';
    for (let i = 0; i < 6; i++) suffix += chars[Math.floor(Math.random() * chars.length)];
    name = `Anon_${suffix}`;
    exists = !!(await User.findOne({ name }));
  } while (exists);
  return name;
}

app.post('/api/anon-user', async (_req: Request, res: Response): Promise<void> => {
  try {
    const name = await generateAnonName();
    const user = await User.create({ name, isAnonymous: true });
    res.json({ user: { id: user._id, name: user.name } });
  } catch {
    res.status(500).json({ error: 'Failed to create guest session' });
  }
});

app.post('/api/users/:userName/register', async (req: Request, res: Response): Promise<void> => {
  const anonName = req.params.userName;
  const newName = typeof req.body?.newName === 'string' ? req.body.newName.trim().slice(0, 32) : '';
  if (!newName) { res.status(400).json({ error: 'newName is required' }); return; }

  const anonUser = await User.findOne({ name: anonName });
  if (!anonUser) { res.status(404).json({ error: 'User not found' }); return; }
  if (!anonUser.isAnonymous) { res.status(400).json({ error: 'User is already registered' }); return; }

  const existing = await User.findOne({ name: newName });
  if (existing) { res.status(409).json({ error: 'Username already taken' }); return; }

  // Find DM rooms before updating so we can rename them
  const dmRooms = await Room.find({ isDM: true, dmUsers: anonName });

  // Migrate messages
  await Message.updateMany({ sender: anonName }, { $set: { sender: newName } });
  await Message.updateMany(
    { readBy: anonName },
    { $set: { 'readBy.$[el]': newName } },
    { arrayFilters: [{ el: anonName }] }
  );
  await Message.updateMany(
    { 'reactions.users': anonName },
    { $set: { 'reactions.$[].users.$[u]': newName } },
    { arrayFilters: [{ u: anonName }] }
  );

  // Migrate rooms
  await Room.updateMany({ members: anonName }, { $set: { 'members.$[el]': newName } }, { arrayFilters: [{ el: anonName }] });
  await Room.updateMany({ admins: anonName }, { $set: { 'admins.$[el]': newName } }, { arrayFilters: [{ el: anonName }] });
  await Room.updateMany({ banned: anonName }, { $set: { 'banned.$[el]': newName } }, { arrayFilters: [{ el: anonName }] });
  await Room.updateMany({ createdBy: anonName }, { $set: { createdBy: newName } });
  await Room.updateMany({ dmUsers: anonName }, { $set: { 'dmUsers.$[el]': newName } }, { arrayFilters: [{ el: anonName }] });

  // Rename DM rooms (their name includes the usernames)
  for (const dmRoom of dmRooms) {
    const updatedUsers = dmRoom.dmUsers.map((u) => (u === anonName ? newName : u));
    const sorted = [...updatedUsers].sort();
    await Room.updateOne({ _id: dmRoom._id }, { $set: { name: `__dm__${sorted[0]}__${sorted[1]}` } });
  }

  // Migrate scheduled messages, drafts, invitations
  await ScheduledMessage.updateMany({ sender: anonName }, { $set: { sender: newName } });
  await Draft.updateMany({ userName: anonName }, { $set: { userName: newName } });
  await Invitation.updateMany({ invitedUser: anonName }, { $set: { invitedUser: newName } });
  await Invitation.updateMany({ invitedBy: anonName }, { $set: { invitedBy: newName } });

  // Rename the user document
  await User.updateOne({ name: anonName }, { $set: { name: newName, isAnonymous: false } });

  // Broadcast updates
  const updatedRooms = await Room.find({ $or: [{ members: newName }, { createdBy: newName }] });
  for (const room of updatedRooms) {
    if (room.isPrivate || room.isDM) {
      emitToUsers(room.members, 'room-updated', { room });
    } else {
      io.emit('room-updated', { room });
    }
  }
  const allUsers = await User.find({}).select('name status lastSeen online');
  io.emit('online-users', { users: allUsers });

  res.json({ user: { id: anonUser._id, name: newName } });
});

app.get('/api/drafts', async (req: Request, res: Response): Promise<void> => {
  const userName = typeof req.query.userName === 'string' ? req.query.userName.trim() : '';
  if (!userName) { res.status(400).json({ error: 'userName required' }); return; }
  const rows = await Draft.find({ userName });
  const drafts: Record<string, string> = {};
  for (const row of rows) drafts[row.roomId] = row.text;
  res.json({ drafts });
});

io.on('connection', (socket) => {
  let currentUser: string | null = null;

  socket.on('authenticate', async ({ userName }: { userName: string }) => {
    currentUser = userName;
    if (!userSockets.has(userName)) userSockets.set(userName, new Set());
    userSockets.get(userName)!.add(socket.id);
    await User.findOneAndUpdate(
      { name: userName },
      { online: true, socketId: socket.id, lastSeen: new Date() },
      { upsert: true, new: true }
    );
    const allUsers = await User.find({}).select('name status lastSeen online');
    io.emit('online-users', { users: allUsers });

    // Auto-join private/DM rooms the user is already a member of
    const privateRooms = await Room.find({ members: userName, $or: [{ isPrivate: true }, { isDM: true }] }).select('_id');
    for (const room of privateRooms) {
      socket.join(room._id.toString());
    }
  });

  socket.on('join-room', (roomId: string) => {
    socket.join(roomId);
  });

  socket.on('join-thread', (messageId: string) => {
    socket.join(`thread-${messageId}`);
  });

  socket.on('leave-thread', (messageId: string) => {
    socket.leave(`thread-${messageId}`);
  });

  socket.on('leave-room', (roomId: string) => {
    socket.leave(roomId);
    if (currentUser) {
      clearTyping(roomId, currentUser);
      broadcastTyping(roomId);
    }
  });

  socket.on('typing-start', ({ roomId }: { roomId: string }) => {
    if (!currentUser || !roomId) return;
    if (!typingTimers.has(roomId)) typingTimers.set(roomId, new Map());
    clearTyping(roomId, currentUser);
    const user = currentUser;
    const timer = setTimeout(() => {
      clearTyping(roomId, user);
      broadcastTyping(roomId);
    }, 3000);
    typingTimers.get(roomId)!.set(currentUser, timer);
    broadcastTyping(roomId);
  });

  socket.on('typing-stop', ({ roomId }: { roomId: string }) => {
    if (!currentUser || !roomId) return;
    clearTyping(roomId, currentUser);
    broadcastTyping(roomId);
  });

  socket.on('draft-update', async ({ roomId, text }: { roomId: string; text: string }) => {
    if (!currentUser || !roomId) return;
    const trimmed = typeof text === 'string' ? text.slice(0, 2000) : '';
    if (trimmed) {
      await Draft.findOneAndUpdate(
        { userName: currentUser, roomId },
        { text: trimmed, updatedAt: new Date() },
        { upsert: true, new: true }
      );
    } else {
      await Draft.deleteOne({ userName: currentUser, roomId });
    }
    // Broadcast to other sockets of the same user (multi-device sync)
    const userSocketSet = userSockets.get(currentUser);
    if (userSocketSet) {
      for (const sid of userSocketSet) {
        if (sid !== socket.id) {
          io.to(sid).emit('draft-updated', { roomId, text: trimmed });
        }
      }
    }
  });

  socket.on('disconnect', async () => {
    if (!currentUser) return;
    const user = currentUser;
    const sockets = userSockets.get(user);
    if (sockets) {
      sockets.delete(socket.id);
      if (sockets.size === 0) userSockets.delete(user);
    }
    const stillOnline = (userSockets.get(user)?.size ?? 0) > 0;
    if (!stillOnline) {
      await User.findOneAndUpdate({ name: user }, { online: false, lastSeen: new Date() });
    }
    const roomsToUpdate: string[] = [];
    for (const [roomId, roomMap] of typingTimers.entries()) {
      if (roomMap.has(user)) {
        clearTimeout(roomMap.get(user)!);
        roomMap.delete(user);
        roomsToUpdate.push(roomId);
      }
    }
    for (const roomId of roomsToUpdate) broadcastTyping(roomId);
    const allUsers = await User.find({}).select('name status lastSeen online');
    io.emit('online-users', { users: allUsers });
  });
});

setInterval(async () => {
  try {
    const due = await ScheduledMessage.find({ sent: false, scheduledAt: { $lte: new Date() } });
    for (const scheduled of due) {
      const msg = await Message.create({
        roomId: scheduled.roomId,
        sender: scheduled.sender,
        text: scheduled.text,
        readBy: [scheduled.sender],
      });
      scheduled.sent = true;
      await scheduled.save();
      const roomId = scheduled.roomId.toString();
      io.to(roomId).emit('message', { message: msg });
      io.to(roomId).emit('scheduled-message-sent', { scheduledId: scheduled._id.toString() });
      trackMessageActivity(roomId);
    }
  } catch (err) {
    console.error('Scheduled message poll error:', err);
  }
}, 10000);

setInterval(async () => {
  try {
    // Use lean() here — we only need _id and roomId, no full documents
    const expired = await Message.find({ expiresAt: { $lte: new Date() } }).select('_id roomId').lean();
    for (const msg of expired) {
      const roomId = (msg.roomId as mongoose.Types.ObjectId).toString();
      await Message.findByIdAndDelete(msg._id);
      io.to(roomId).emit('message-deleted', { messageId: (msg._id as mongoose.Types.ObjectId).toString(), roomId });
    }
  } catch (err) {
    console.error('Ephemeral message cleanup error:', err);
  }
}, 5000);

// Periodically re-evaluate activity levels so badges decay in real time when rooms go quiet
setInterval(() => {
  const now = Date.now();
  const cutoff = now - 10 * 60 * 1000;
  for (const [roomId, timestamps] of roomActivityTimestamps.entries()) {
    // Trim stale entries in-place
    let start = 0;
    while (start < timestamps.length && timestamps[start] < cutoff) start++;
    if (start > 0) timestamps.splice(0, start);

    const level = getActivityLevel(roomId);
    const prev = lastEmittedActivityLevel.get(roomId) ?? '';
    if (level !== prev) {
      lastEmittedActivityLevel.set(roomId, level);
      io.emit('room-activity', { roomId, level });
    }
    if (timestamps.length === 0) {
      roomActivityTimestamps.delete(roomId);
      lastEmittedActivityLevel.delete(roomId);
    }
  }
}, 15000);

const PORT = Number(process.env.PORT) || 6001;
httpServer.listen(PORT, () => {
  console.log(`Server on port ${PORT}`);
});

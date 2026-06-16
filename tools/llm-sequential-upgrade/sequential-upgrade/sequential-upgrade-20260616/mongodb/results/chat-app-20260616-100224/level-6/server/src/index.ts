import 'dotenv/config';
import express, { Request, Response } from 'express';
import { createServer } from 'http';
import { Server } from 'socket.io';
import cors from 'cors';
import mongoose from 'mongoose';
import { User, Room, Message, ScheduledMessage } from './models.js';

const app = express();
const httpServer = createServer(app);

const io = new Server(httpServer, {
  cors: {
    origin: 'http://localhost:6373',
    methods: ['GET', 'POST'],
  },
});

app.use(cors({ origin: 'http://localhost:6373' }));
app.use(express.json());

const DB_URL = process.env.DATABASE_URL ?? 'mongodb://localhost:6437/chat-app';
await mongoose.connect(DB_URL);
console.log('Connected to MongoDB');

// roomId -> { userName -> timeout }
const typingTimers = new Map<string, Map<string, ReturnType<typeof setTimeout>>>();

// userName -> Set of active socketIds (tracks multi-tab presence)
const userSockets = new Map<string, Set<string>>();

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

app.get('/api/health', (_req: Request, res: Response): void => {
  res.json({ ok: true });
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
  const users = await User.find({ online: true }).select('name');
  res.json({ users });
});

app.get('/api/rooms', async (_req: Request, res: Response): Promise<void> => {
  const rooms = await Room.find().sort({ createdAt: 1 });
  res.json({ rooms });
});

app.post('/api/rooms', async (req: Request, res: Response): Promise<void> => {
  const name = typeof req.body?.name === 'string' ? req.body.name.trim().slice(0, 64) : '';
  const createdBy = typeof req.body?.createdBy === 'string' ? req.body.createdBy.trim() : '';
  if (!name || !createdBy) {
    res.status(400).json({ error: 'name and createdBy are required' });
    return;
  }
  try {
    const room = await Room.create({ name, createdBy, members: [createdBy], admins: [createdBy] });
    io.emit('room-created', { room });
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
  io.emit('room-updated', { room });
  res.json({ room });
});

app.get('/api/rooms/:roomId/messages', async (req: Request, res: Response): Promise<void> => {
  const messages = await Message.find({ roomId: req.params.roomId })
    .sort({ createdAt: 1 })
    .limit(100);
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
  io.to(req.params.roomId).emit('message', { message: msg });
  res.json({ message: msg });
});

app.post('/api/rooms/:roomId/read', async (req: Request, res: Response): Promise<void> => {
  const userName = typeof req.body?.userName === 'string' ? req.body.userName.trim() : '';
  if (!userName) { res.status(400).json({ error: 'userName required' }); return; }
  const roomId = req.params.roomId;
  await Message.updateMany(
    { roomId, readBy: { $ne: userName } },
    { $addToSet: { readBy: userName } }
  );
  const messages = await Message.find({ roomId }).sort({ createdAt: 1 }).limit(100);
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

  io.emit('room-updated', { room });
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

  io.emit('room-updated', { room });
  res.json({ room });
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
    const online = await User.find({ online: true }).select('name');
    io.emit('online-users', { users: online.map((u) => u.name) });
  });

  socket.on('join-room', (roomId: string) => {
    socket.join(roomId);
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
    const online = await User.find({ online: true }).select('name');
    io.emit('online-users', { users: online.map((u) => u.name) });
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
    }
  } catch (err) {
    console.error('Scheduled message poll error:', err);
  }
}, 10000);

setInterval(async () => {
  try {
    const expired = await Message.find({ expiresAt: { $lte: new Date() } }).select('_id roomId');
    for (const msg of expired) {
      const roomId = msg.roomId.toString();
      await Message.findByIdAndDelete(msg._id);
      io.to(roomId).emit('message-deleted', { messageId: msg._id.toString(), roomId });
    }
  } catch (err) {
    console.error('Ephemeral message cleanup error:', err);
  }
}, 5000);

const PORT = Number(process.env.PORT) || 6001;
httpServer.listen(PORT, () => {
  console.log(`Server on port ${PORT}`);
});

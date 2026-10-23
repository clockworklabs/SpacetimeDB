// MongoDB chat-app client wrapper for the perf benchmark.
//
// The Level 12 generated MongoDB app (MERN: Express + Mongoose + Socket.io)
// exposes a DIFFERENT contract than the Postgres app — identity is the
// username string (not a numeric id), and messages are sent over REST:
//
//   POST /api/users { name }                       -> { user: { id, name } }
//   POST /api/rooms { name, createdBy }            -> { room: { _id, ... } }
//   POST /api/rooms/:roomId/join { userName }
//   POST /api/rooms/:roomId/messages { sender, text } -> { message: {...} }   // SEND (REST)
//   socket.emit('authenticate', { userName })       // register presence
//   socket.emit('join-room', roomId)                // BARE string arg (not {roomId})
//   socket.on('message', ({ message }) => ...)       // broadcast wrapped in { message }
//
// Notes:
// - The Mongo app's send_message path is REST-only (there is no socket
//   'send_message' handler), so ack latency is the POST HTTP round-trip
//   (server inserts + responds). Fan-out latency is measured by a separate
//   listener socket joined to the room (true server→client broadcast).
// - IMPORTANT: unlike the PG app, the Mongo app enforces NO per-user send
//   rate limit. Throughput numbers are therefore not directly comparable to
//   the PG `stress` scenario (PG caps each writer at ~2 msg/s). The
//   `realistic` scenario (human cadence, well under any throttle) is the
//   apples-to-apples comparison.

import { io, type Socket } from 'socket.io-client';

export interface MongoConfig {
  baseUrl: string; // e.g. http://localhost:6001
}

export interface MongoUser {
  name: string; // username IS the identity in this app
}

export async function createMongoUser(cfg: MongoConfig, name: string): Promise<MongoUser> {
  const res = await fetch(`${cfg.baseUrl}/api/users`, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ name }),
  });
  if (!res.ok) throw new Error(`createMongoUser ${name} failed: ${res.status} ${await res.text()}`);
  return { name };
}

export async function createMongoRoom(cfg: MongoConfig, name: string, createdBy: string): Promise<{ id: string }> {
  const res = await fetch(`${cfg.baseUrl}/api/rooms`, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ name, createdBy, isPrivate: false }),
  });
  if (!res.ok) throw new Error(`createMongoRoom ${name} failed: ${res.status} ${await res.text()}`);
  const body = (await res.json()) as { room: { _id: string } };
  return { id: body.room._id };
}

export async function joinMongoRoom(cfg: MongoConfig, roomId: string, userName: string): Promise<void> {
  const res = await fetch(`${cfg.baseUrl}/api/rooms/${roomId}/join`, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ userName }),
  });
  // Membership is not required to send (the messages endpoint has no member
  // check), so a failed join is non-fatal — swallow it.
  if (!res.ok) { /* ignore */ }
}

export interface MongoMessage {
  sender: string;
  text: string;
  roomId: string;
}

export interface MongoClientHandle {
  socket: Socket;
  userName: string;
  close(): void;
}

export async function connectMongoClient(
  cfg: MongoConfig,
  userName: string,
  roomId: string,
  onMessage: (msg: MongoMessage) => void,
): Promise<MongoClientHandle> {
  const socket = io(cfg.baseUrl, {
    transports: ['websocket'],
    reconnection: false,
    forceNew: true,
  });
  await new Promise<void>((resolve, reject) => {
    socket.once('connect', () => resolve());
    socket.once('connect_error', (err) => reject(err));
    setTimeout(() => reject(new Error('socket connect timeout')), 10_000);
  });
  socket.emit('authenticate', { userName });
  socket.emit('join-room', roomId); // bare string arg, per the app's handler
  socket.on('message', (payload: { message: MongoMessage }) => {
    if (payload && payload.message) onMessage(payload.message);
  });
  return {
    socket,
    userName,
    close: () => {
      try { socket.disconnect(); } catch { /* ignore */ }
    },
  };
}

// REST send: POST /api/rooms/:roomId/messages { sender, text }. Returns the
// created message on success, null on failure.
export async function mongoSendRest(cfg: MongoConfig, roomId: string, sender: string, text: string): Promise<unknown> {
  const res = await fetch(`${cfg.baseUrl}/api/rooms/${roomId}/messages`, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ sender, text }),
  });
  if (!res.ok) return null;
  return res.json();
}

// Postgres chat-app client wrapper for the perf benchmark.
//
// The Level 11 generated app exposes:
//   POST /api/users { name } -> { id, name, ... }
//   POST /api/rooms { name, userId, isPrivate: false } -> { id, ... }
//   POST /api/rooms/:id/join { userId }
//   socket.emit('register', { userId, userName })
//   socket.emit('join_room', { roomId })
//   socket.emit('send_message', { roomId, content })
//   socket.on('message', cb)            // top-level messages broadcast to room subscribers
//
// Notes:
// - The send_message handler enforces a 500ms per-user rate limit (server/src/index.ts).
//   This means each writer can issue at most ~2 msgs/sec. Throughput must scale via writers.
// - The handler does NOT call a socket.io ack callback. We treat the round-trip
//   "send → server inserts → server emits 'message' back to me" as ack latency.
// - All client connections in a single Node process share clocks, so fan-out latency
//   measured by a separate listener client is meaningful.

import { io, type Socket } from 'socket.io-client';

export interface PgConfig {
  baseUrl: string; // e.g. http://localhost:6001
}

export interface PgUser {
  id: number;
  name: string;
}

export async function createPgUser(cfg: PgConfig, name: string): Promise<PgUser> {
  const res = await fetch(`${cfg.baseUrl}/api/users`, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ name }),
  });
  if (!res.ok) throw new Error(`createPgUser ${name} failed: ${res.status} ${await res.text()}`);
  return (await res.json()) as PgUser;
}

export async function createPgRoom(cfg: PgConfig, name: string, userId: number): Promise<{ id: number }> {
  const res = await fetch(`${cfg.baseUrl}/api/rooms`, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ name, userId, isPrivate: false }),
  });
  if (!res.ok) throw new Error(`createPgRoom ${name} failed: ${res.status} ${await res.text()}`);
  return (await res.json()) as { id: number };
}

export async function joinPgRoom(cfg: PgConfig, roomId: number, userId: number): Promise<void> {
  const res = await fetch(`${cfg.baseUrl}/api/rooms/${roomId}/join`, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ userId }),
  });
  if (!res.ok) throw new Error(`joinPgRoom ${roomId} failed: ${res.status} ${await res.text()}`);
}

export interface PgClientHandle {
  socket: Socket;
  user: PgUser;
  close(): void;
}

export async function connectPgClient(
  cfg: PgConfig,
  user: PgUser,
  roomId: number,
  onMessage: (msg: { id: number; roomId: number; userId: number; content: string }) => void,
): Promise<PgClientHandle> {
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
  socket.emit('register', { userId: user.id, userName: user.name });
  socket.emit('join_room', { roomId });
  socket.on('message', onMessage);
  return {
    socket,
    user,
    close: () => {
      try { socket.disconnect(); } catch { /* ignore */ }
    },
  };
}

export function pgSend(handle: PgClientHandle, roomId: number, content: string): void {
  handle.socket.emit('send_message', { roomId, content });
}

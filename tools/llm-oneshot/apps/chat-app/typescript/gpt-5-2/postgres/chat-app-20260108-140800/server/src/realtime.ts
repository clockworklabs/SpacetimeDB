import type { Server, Socket } from 'socket.io';
import { eq } from 'drizzle-orm';
import type { PostgresJsDatabase } from 'drizzle-orm/postgres-js';
import { users } from './db/schema';

type TypingEntry = { userId: string; displayName: string; expiresAtMs: number };

export function createRealtime(
  io: Server,
  db: PostgresJsDatabase<Record<string, never>>
) {
  const userSocketCounts = new Map<string, number>();
  const typingByRoom = new Map<number, Map<string, TypingEntry>>();

  function setOnline(userId: string, isOnline: boolean) {
    void db
      .update(users)
      .set({ isOnline, lastActiveAt: new Date() })
      .where(eq(users.id, userId));
    io.emit('presence:changed', { userId, isOnline });
  }

  function bumpPresence(userId: string, delta: number) {
    const next = (userSocketCounts.get(userId) || 0) + delta;
    if (next <= 0) {
      userSocketCounts.delete(userId);
      setOnline(userId, false);
    } else {
      const prev = userSocketCounts.get(userId) || 0;
      userSocketCounts.set(userId, next);
      if (prev === 0) setOnline(userId, true);
      else
        void db
          .update(users)
          .set({ lastActiveAt: new Date() })
          .where(eq(users.id, userId));
    }
  }

  function emitTyping(roomId: number) {
    const entries = [...(typingByRoom.get(roomId)?.values() || [])]
      .filter(e => e.expiresAtMs > Date.now())
      .map(e => ({ userId: e.userId, displayName: e.displayName }));
    io.emit('typing:state', { roomId, users: entries });
  }

  function setTyping(
    roomId: number,
    userId: string,
    displayName: string,
    isTyping: boolean
  ) {
    let room = typingByRoom.get(roomId);
    if (!room) {
      room = new Map();
      typingByRoom.set(roomId, room);
    }

    if (!isTyping) room.delete(userId);
    else {
      room.set(userId, { userId, displayName, expiresAtMs: Date.now() + 4000 });
    }
    emitTyping(roomId);
  }

  // Cleanup timer (typing indicators)
  const typingCleanup = setInterval(() => {
    const now = Date.now();
    let changed = false;
    for (const [roomId, room] of typingByRoom) {
      for (const [userId, entry] of room) {
        if (entry.expiresAtMs <= now) {
          room.delete(userId);
          changed = true;
        }
      }
      if (room.size === 0) typingByRoom.delete(roomId);
      if (changed) emitTyping(roomId);
      changed = false;
    }
  }, 1000);

  function close() {
    clearInterval(typingCleanup);
  }

  function onSocketAuthed(socket: Socket, userId: string, displayName: string) {
    bumpPresence(userId, +1);

    socket.on('typing:start', (payload: any) => {
      const roomId = Number(payload?.roomId);
      if (!Number.isFinite(roomId)) return;
      setTyping(roomId, userId, displayName, true);
    });

    socket.on('typing:stop', (payload: any) => {
      const roomId = Number(payload?.roomId);
      if (!Number.isFinite(roomId)) return;
      setTyping(roomId, userId, displayName, false);
    });

    socket.on('disconnect', () => {
      bumpPresence(userId, -1);
      // Remove typing entries for this user (all rooms)
      for (const [roomId, room] of typingByRoom) {
        if (room.delete(userId)) emitTyping(roomId);
      }
    });
  }

  function broadcastRoomsChanged() {
    io.emit('rooms:changed', {});
  }

  function broadcastRoomMembersChanged(roomId: number) {
    io.emit('roomMembers:changed', { roomId });
  }

  function broadcastMessageCreated(roomId: number, messageId: number) {
    io.emit('message:created', { roomId, messageId });
  }

  function broadcastMessageUpdated(roomId: number, messageId: number) {
    io.emit('message:updated', { roomId, messageId });
  }

  function broadcastMessageDeleted(roomId: number, messageId: number) {
    io.emit('message:deleted', { roomId, messageId });
  }

  function broadcastReactionsChanged(messageId: number) {
    io.emit('reactions:changed', { messageId });
  }

  function broadcastReadPositionChanged(roomId: number, userId: string) {
    io.emit('reads:changed', { roomId, userId });
  }

  return {
    close,
    onSocketAuthed,
    broadcastRoomsChanged,
    broadcastRoomMembersChanged,
    broadcastMessageCreated,
    broadcastMessageUpdated,
    broadcastMessageDeleted,
    broadcastReactionsChanged,
    broadcastReadPositionChanged,
  };
}

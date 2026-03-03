import { db, messages } from '../db';
import { lte, and, eq } from 'drizzle-orm';
import { Server } from 'socket.io';

let io: Server;

export function setCleanupSocketServer(socketServer: Server) {
  io = socketServer;
}

export async function cleanupExpiredMessages() {
  try {
    const now = new Date();

    // Find expired messages that haven't been deleted yet
    const expiredMessages = await db
      .select({ id: messages.id, roomId: messages.roomId })
      .from(messages)
      .where(and(lte(messages.expiresAt, now), eq(messages.isDeleted, false)));

    if (expiredMessages.length > 0) {
      // Mark as deleted (soft delete)
      await db
        .update(messages)
        .set({ isDeleted: true })
        .where(
          and(lte(messages.expiresAt, now), eq(messages.isDeleted, false))
        );

      console.log(`Cleaned up ${expiredMessages.length} expired messages`);

      // Broadcast to clients that these messages have been deleted
      if (io) {
        for (const msg of expiredMessages) {
          io.to(msg.roomId).emit('message_deleted', {
            messageId: msg.id,
            roomId: msg.roomId,
          });
        }
      }
    }
  } catch (error) {
    console.error('Error cleaning up expired messages:', error);
  }
}

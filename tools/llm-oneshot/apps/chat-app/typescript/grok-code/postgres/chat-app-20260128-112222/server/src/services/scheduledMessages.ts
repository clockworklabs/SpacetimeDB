import { db, scheduledMessages, messages, users } from '../db';
import { eq, lte, and } from 'drizzle-orm';
import { Server } from 'socket.io';

let io: Server;

export function setSocketServer(socketServer: Server) {
  io = socketServer;
}

export async function processScheduledMessages() {
  try {
    const now = new Date();

    // Find messages that should be sent now
    const messagesToSend = await db.select({
      id: messages.id,
      roomId: messages.roomId,
      userId: messages.userId,
      content: messages.content,
      createdAt: messages.createdAt,
      expiresAt: messages.expiresAt,
      scheduledId: scheduledMessages.id,
      displayName: users.displayName,
    })
    .from(scheduledMessages)
    .innerJoin(messages, eq(scheduledMessages.messageId, messages.id))
    .leftJoin(users, eq(messages.userId, users.id))
    .where(and(
      lte(scheduledMessages.scheduledFor, now),
      eq(scheduledMessages.status, 'pending')
    ));

    for (const message of messagesToSend) {
      // Send the message to the room
      const messageData = {
        id: message.id,
        roomId: message.roomId,
        userId: message.userId,
        displayName: message.displayName,
        content: message.content,
        createdAt: message.createdAt,
        updatedAt: new Date(),
        expiresAt: message.expiresAt,
        isScheduled: true,
      };

      if (io) {
        io.to(message.roomId).emit('new_message', messageData);
      }

      // Mark as sent
      await db.update(scheduledMessages)
        .set({ status: 'sent' })
        .where(eq(scheduledMessages.id, message.scheduledId));

      console.log(`Sent scheduled message: ${message.id}`);
    }

  } catch (error) {
    console.error('Error processing scheduled messages:', error);
  }
}
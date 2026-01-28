import { Server, Socket } from 'socket.io';
import { db, users, rooms, roomMembers, messages, messageReactions, readReceipts, typingIndicators, unreadCounts, onlineUsers, scheduledMessages, messageEdits } from '../db';
import { eq, and, gt, sql } from 'drizzle-orm';
import { v4 as uuidv4 } from 'uuid';

interface AuthenticatedSocket extends Socket {
  userId?: string;
  displayName?: string;
}

export function setupSocketHandlers(io: Server) {
  io.on('connection', (socket: AuthenticatedSocket) => {
    console.log('User connected:', socket.id);

    // Authentication
    socket.on('authenticate', async (data: { displayName: string }) => {
      try {
        const { displayName } = data;

        if (!displayName || displayName.trim().length === 0 || displayName.length > 50) {
          socket.emit('error', { message: 'Invalid display name' });
          return;
        }

        // Create or get user
        let user = await db.select().from(users).where(eq(users.displayName, displayName)).limit(1);
        let userId: string;

        if (user.length === 0) {
          userId = uuidv4();
          await db.insert(users).values({
            id: userId,
            displayName: displayName.trim(),
          });
        } else {
          userId = user[0].id;
          // Update last seen
          await db.update(users).set({ lastSeen: new Date() }).where(eq(users.id, userId));
        }

        socket.userId = userId;
        socket.displayName = displayName.trim();

        // Add to online users
        await db.insert(onlineUsers).values({
          userId,
          socketId: socket.id,
        });

        socket.emit('authenticated', { userId, displayName: socket.displayName });

        // Broadcast online status
        io.emit('user_online', { userId, displayName: socket.displayName });

        // Send initial data
        await sendInitialData(socket);

      } catch (error) {
        console.error('Authentication error:', error);
        socket.emit('error', { message: 'Authentication failed' });
      }
    });

    // Room operations
    socket.on('create_room', async (data: { name: string }) => {
      if (!socket.userId) return;

      try {
        const { name } = data;
        if (!name || name.trim().length === 0 || name.length > 100) {
          socket.emit('error', { message: 'Invalid room name' });
          return;
        }

        const roomId = uuidv4();
        await db.insert(rooms).values({
          id: roomId,
          name: name.trim(),
          createdBy: socket.userId,
        });

        // Add creator to room
        await db.insert(roomMembers).values({
          roomId,
          userId: socket.userId,
        });

        const newRoom = {
          id: roomId,
          name: name.trim(),
          createdAt: new Date(),
          memberCount: 1,
        };

        // Broadcast new room to ALL connected users so they can see it
        io.emit('room_created', newRoom);

        // Join socket room
        socket.join(roomId);

        // Update unread counts
        await updateUnreadCounts(socket.userId);

      } catch (error) {
        console.error('Create room error:', error);
        socket.emit('error', { message: 'Failed to create room' });
      }
    });

    // Get all available rooms
    socket.on('get_all_rooms', async () => {
      if (!socket.userId) return;

      try {
        const allRooms = await db.select({
          id: rooms.id,
          name: rooms.name,
          createdAt: rooms.createdAt,
          memberCount: sql<number>`count(${roomMembers.userId})`,
        })
        .from(rooms)
        .leftJoin(roomMembers, eq(rooms.id, roomMembers.roomId))
        .groupBy(rooms.id, rooms.name, rooms.createdAt);

        socket.emit('all_rooms', { rooms: allRooms });
      } catch (error) {
        console.error('Get all rooms error:', error);
        socket.emit('error', { message: 'Failed to get rooms' });
      }
    });

    socket.on('join_room', async (data: { roomId: string }) => {
      if (!socket.userId) return;

      try {
        const { roomId } = data;

        // Check if room exists
        const room = await db.select().from(rooms).where(eq(rooms.id, roomId)).limit(1);
        if (room.length === 0) {
          socket.emit('error', { message: 'Room not found' });
          return;
        }

        // Check if already a member
        const membership = await db.select().from(roomMembers)
          .where(and(eq(roomMembers.roomId, roomId), eq(roomMembers.userId, socket.userId)))
          .limit(1);

        if (membership.length === 0) {
          await db.insert(roomMembers).values({
            roomId,
            userId: socket.userId,
          });
        }

        socket.join(roomId);

        // Send room messages
        const roomMessages = await db.select({
          id: messages.id,
          content: messages.content,
          createdAt: messages.createdAt,
          updatedAt: messages.updatedAt,
          userId: messages.userId,
          displayName: users.displayName,
          isDeleted: messages.isDeleted,
          expiresAt: messages.expiresAt,
        })
        .from(messages)
        .leftJoin(users, eq(messages.userId, users.id))
        .where(and(eq(messages.roomId, roomId), eq(messages.isDeleted, false)))
        .orderBy(messages.createdAt)
        .limit(100);

        socket.emit('room_joined', {
          roomId,
          messages: roomMessages,
        });

        // Update unread counts
        await updateUnreadCounts(socket.userId);

      } catch (error) {
        console.error('Join room error:', error);
        socket.emit('error', { message: 'Failed to join room' });
      }
    });

    // Message operations
    socket.on('send_message', async (data: { roomId: string; content: string; scheduledFor?: string; expiresAt?: string }) => {
      if (!socket.userId) return;

      try {
        const { roomId, content, scheduledFor, expiresAt } = data;

        if (!content || content.trim().length === 0 || content.length > 2000) {
          socket.emit('error', { message: 'Invalid message content' });
          return;
        }

        // Check if user is member of room
        const membership = await db.select().from(roomMembers)
          .where(and(eq(roomMembers.roomId, roomId), eq(roomMembers.userId, socket.userId)))
          .limit(1);

        if (membership.length === 0) {
          socket.emit('error', { message: 'Not a member of this room' });
          return;
        }

        const messageId = uuidv4();

        // Parse dates if provided
        let scheduledDate: Date | undefined;
        let expiresDate: Date | undefined;

        if (scheduledFor) {
          scheduledDate = new Date(scheduledFor);
          if (isNaN(scheduledDate.getTime())) {
            socket.emit('error', { message: 'Invalid scheduled time' });
            return;
          }
        }

        if (expiresAt) {
          expiresDate = new Date(expiresAt);
          if (isNaN(expiresDate.getTime())) {
            socket.emit('error', { message: 'Invalid expiration time' });
            return;
          }
        }

        await db.insert(messages).values({
          id: messageId,
          roomId,
          userId: socket.userId,
          content: content.trim(),
          scheduledFor: scheduledDate,
          expiresAt: expiresDate,
        });

        // If not scheduled, send immediately
        if (!scheduledDate) {
          const messageData = {
            id: messageId,
            roomId,
            userId: socket.userId,
            displayName: socket.displayName!,
            content: content.trim(),
            createdAt: new Date(),
            updatedAt: new Date(),
            expiresAt: expiresDate,
          };

          io.to(roomId).emit('new_message', messageData);

          // Update unread counts for other users
          await updateUnreadCountsForRoom(roomId, socket.userId);
        } else {
          // Add to scheduled messages queue
          await db.insert(scheduledMessages).values({
            messageId,
            scheduledFor: scheduledDate,
          });

          socket.emit('message_scheduled', { messageId, scheduledFor: scheduledDate });
        }

      } catch (error) {
        console.error('Send message error:', error);
        socket.emit('error', { message: 'Failed to send message' });
      }
    });

    socket.on('edit_message', async (data: { messageId: string; content: string }) => {
      if (!socket.userId) return;

      try {
        const { messageId, content } = data;

        if (!content || content.trim().length === 0 || content.length > 2000) {
          socket.emit('error', { message: 'Invalid message content' });
          return;
        }

        // Get original message
        const message = await db.select().from(messages).where(eq(messages.id, messageId)).limit(1);
        if (message.length === 0) {
          socket.emit('error', { message: 'Message not found' });
          return;
        }

        if (message[0].userId !== socket.userId) {
          socket.emit('error', { message: 'Can only edit your own messages' });
          return;
        }

        // Save edit history
        await db.insert(messageEdits).values({
          messageId,
          previousContent: message[0].content,
          editedBy: socket.userId,
        });

        // Update message
        await db.update(messages)
          .set({
            content: content.trim(),
            updatedAt: new Date(),
          })
          .where(eq(messages.id, messageId));

        // Broadcast edit
        const roomId = message[0].roomId;
        io.to(roomId).emit('message_edited', {
          messageId,
          content: content.trim(),
          updatedAt: new Date(),
        });

      } catch (error) {
        console.error('Edit message error:', error);
        socket.emit('error', { message: 'Failed to edit message' });
      }
    });

    // Typing indicators
    socket.on('start_typing', async (data: { roomId: string }) => {
      if (!socket.userId) return;

      try {
        const { roomId } = data;
        const expiresAt = new Date(Date.now() + 3000); // 3 seconds

        await db.insert(typingIndicators).values({
          roomId,
          userId: socket.userId,
          expiresAt,
        }).onConflictDoUpdate({
          target: [typingIndicators.roomId, typingIndicators.userId],
          set: { expiresAt },
        });

        socket.to(roomId).emit('user_typing', {
          userId: socket.userId,
          displayName: socket.displayName,
          roomId,
        });

      } catch (error) {
        console.error('Start typing error:', error);
      }
    });

    socket.on('stop_typing', async (data: { roomId: string }) => {
      if (!socket.userId) return;

      try {
        const { roomId } = data;

        await db.delete(typingIndicators)
          .where(and(eq(typingIndicators.roomId, roomId), eq(typingIndicators.userId, socket.userId)));

        socket.to(roomId).emit('user_stopped_typing', {
          userId: socket.userId,
          roomId,
        });

      } catch (error) {
        console.error('Stop typing error:', error);
      }
    });

    // Read receipts
    socket.on('mark_as_read', async (data: { roomId: string; messageId: string }) => {
      if (!socket.userId) return;

      try {
        const { roomId, messageId } = data;

        // Insert read receipt
        await db.insert(readReceipts).values({
          messageId,
          userId: socket.userId,
        }).onConflictDoNothing();

        // Update last read message for user in room
        await db.update(roomMembers)
          .set({ lastReadMessageId: messageId })
          .where(and(eq(roomMembers.roomId, roomId), eq(roomMembers.userId, socket.userId)));

        // Update unread counts
        await updateUnreadCounts(socket.userId);

        // Broadcast read receipt
        socket.to(roomId).emit('message_read', {
          messageId,
          userId: socket.userId,
          displayName: socket.displayName,
        });

      } catch (error) {
        console.error('Mark as read error:', error);
      }
    });

    // Message reactions
    socket.on('add_reaction', async (data: { messageId: string; emoji: string }) => {
      if (!socket.userId) return;

      try {
        const { messageId, emoji } = data;

        if (!['👍', '❤️', '😂', '😮', '😢'].includes(emoji)) {
          socket.emit('error', { message: 'Invalid emoji' });
          return;
        }

        await db.insert(messageReactions).values({
          messageId,
          userId: socket.userId,
          emoji,
        }).onConflictDoNothing();

        // Get message room
        const message = await db.select({ roomId: messages.roomId }).from(messages).where(eq(messages.id, messageId)).limit(1);
        if (message.length > 0) {
          const roomId = message[0].roomId;

          // Get reaction counts
          const reactions = await db.select({
            emoji: messageReactions.emoji,
            count: sql<number>`count(*)`,
            users: sql<string[]>`array_agg(${users.displayName})`,
          })
          .from(messageReactions)
          .leftJoin(users, eq(messageReactions.userId, users.id))
          .where(eq(messageReactions.messageId, messageId))
          .groupBy(messageReactions.emoji);

          io.to(roomId).emit('reaction_updated', {
            messageId,
            reactions: reactions.map(r => ({ emoji: r.emoji, count: r.count, users: r.users })),
          });
        }

      } catch (error) {
        console.error('Add reaction error:', error);
        socket.emit('error', { message: 'Failed to add reaction' });
      }
    });

    socket.on('remove_reaction', async (data: { messageId: string; emoji: string }) => {
      if (!socket.userId) return;

      try {
        const { messageId, emoji } = data;

        await db.delete(messageReactions)
          .where(and(
            eq(messageReactions.messageId, messageId),
            eq(messageReactions.userId, socket.userId),
            eq(messageReactions.emoji, emoji)
          ));

        // Get message room and broadcast update
        const message = await db.select({ roomId: messages.roomId }).from(messages).where(eq(messages.id, messageId)).limit(1);
        if (message.length > 0) {
          const roomId = message[0].roomId;

          const reactions = await db.select({
            emoji: messageReactions.emoji,
            count: sql<number>`count(*)`,
            users: sql<string[]>`array_agg(${users.displayName})`,
          })
          .from(messageReactions)
          .leftJoin(users, eq(messageReactions.userId, users.id))
          .where(eq(messageReactions.messageId, messageId))
          .groupBy(messageReactions.emoji);

          io.to(roomId).emit('reaction_updated', {
            messageId,
            reactions: reactions.map(r => ({ emoji: r.emoji, count: r.count, users: r.users })),
          });
        }

      } catch (error) {
        console.error('Remove reaction error:', error);
        socket.emit('error', { message: 'Failed to remove reaction' });
      }
    });

    // Disconnect
    socket.on('disconnect', async () => {
      if (socket.userId) {
        // Remove from online users
        await db.delete(onlineUsers).where(eq(onlineUsers.socketId, socket.id));

        // Remove typing indicators
        await db.delete(typingIndicators).where(eq(typingIndicators.userId, socket.userId));

        // Broadcast offline status
        io.emit('user_offline', { userId: socket.userId });
      }

      console.log('User disconnected:', socket.id);
    });
  });
}

async function sendInitialData(socket: AuthenticatedSocket) {
  if (!socket.userId) return;

  try {
    // Get user's rooms
    const userRooms = await db.select({
      id: rooms.id,
      name: rooms.name,
      createdAt: rooms.createdAt,
      memberCount: sql<number>`count(${roomMembers.userId})`,
    })
    .from(rooms)
    .leftJoin(roomMembers, eq(rooms.id, roomMembers.roomId))
    .where(eq(roomMembers.userId, socket.userId))
    .groupBy(rooms.id, rooms.name, rooms.createdAt);

    // Get online users
    const online = await db.select({
      userId: onlineUsers.userId,
      displayName: users.displayName,
    })
    .from(onlineUsers)
    .leftJoin(users, eq(onlineUsers.userId, users.id));

    // Get unread counts
    const unread = await db.select({
      roomId: unreadCounts.roomId,
      count: unreadCounts.count,
    })
    .from(unreadCounts)
    .where(eq(unreadCounts.userId, socket.userId));

    socket.emit('initial_data', {
      rooms: userRooms,
      onlineUsers: online,
      unreadCounts: unread,
    });

  } catch (error) {
    console.error('Send initial data error:', error);
  }
}

async function updateUnreadCounts(userId: string) {
  try {
    // This is a simplified implementation - in production you'd want more sophisticated logic
    const userRooms = await db.select({ roomId: roomMembers.roomId })
      .from(roomMembers)
      .where(eq(roomMembers.userId, userId));

    for (const { roomId } of userRooms) {
      const lastRead = await db.select({ lastReadMessageId: roomMembers.lastReadMessageId })
        .from(roomMembers)
        .where(and(eq(roomMembers.roomId, roomId), eq(roomMembers.userId, userId)))
        .limit(1);

      let count = 0;
      if (lastRead.length > 0 && lastRead[0].lastReadMessageId) {
        const unreadResult = await db.select({ count: sql<number>`count(*)` })
          .from(messages)
          .where(and(
            eq(messages.roomId, roomId),
            gt(messages.createdAt, sql`(SELECT created_at FROM messages WHERE id = ${lastRead[0].lastReadMessageId})`),
            eq(messages.isDeleted, false)
          ));

        count = unreadResult[0]?.count || 0;
      } else {
        // Count all messages if no last read
        const totalResult = await db.select({ count: sql<number>`count(*)` })
          .from(messages)
          .where(and(eq(messages.roomId, roomId), eq(messages.isDeleted, false)));

        count = totalResult[0]?.count || 0;
      }

      await db.insert(unreadCounts).values({
        roomId,
        userId,
        count,
        updatedAt: new Date(),
      }).onConflictDoUpdate({
        target: [unreadCounts.roomId, unreadCounts.userId],
        set: { count, updatedAt: new Date() },
      });
    }
  } catch (error) {
    console.error('Update unread counts error:', error);
  }
}

async function updateUnreadCountsForRoom(roomId: string, excludeUserId: string) {
  try {
    const roomMembersList = await db.select({ userId: roomMembers.userId })
      .from(roomMembers)
      .where(and(eq(roomMembers.roomId, roomId), sql`${roomMembers.userId} != ${excludeUserId}`));

    for (const { userId } of roomMembersList) {
      await updateUnreadCounts(userId);
    }

    // Broadcast updated counts
    const counts = await db.select({
      roomId: unreadCounts.roomId,
      userId: unreadCounts.userId,
      count: unreadCounts.count,
    })
    .from(unreadCounts)
    .where(eq(unreadCounts.roomId, roomId));

    for (const count of counts) {
      io.to(roomId).emit('unread_count_updated', {
        roomId: count.roomId,
        count: count.count,
      });
    }
  } catch (error) {
    console.error('Update unread counts for room error:', error);
  }
}
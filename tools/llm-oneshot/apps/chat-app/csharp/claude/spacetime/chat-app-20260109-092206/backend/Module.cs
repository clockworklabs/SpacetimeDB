using SpacetimeDB;

public static partial class Module
{
    // ============================================================================
    // LIFECYCLE HOOKS
    // ============================================================================

    [SpacetimeDB.Reducer(ReducerKind.ClientConnected)]
    public static void ClientConnected(ReducerContext ctx)
    {
        var existingUser = ctx.Db.user.Identity.Find(ctx.Sender);
        if (existingUser != null)
        {
            // Update online status
            ctx.Db.user.Identity.Update(new User
            {
                Identity = existingUser.Identity,
                DisplayName = existingUser.DisplayName,
                IsOnline = true,
                LastSeen = ctx.Timestamp
            });
        }
        else
        {
            // Create new user with default display name
            ctx.Db.user.Insert(new User
            {
                Identity = ctx.Sender,
                DisplayName = $"User_{ctx.Sender.ToString().Substring(0, 8)}",
                IsOnline = true,
                LastSeen = ctx.Timestamp
            });
        }
        Log.Info($"Client connected: {ctx.Sender}");
    }

    [SpacetimeDB.Reducer(ReducerKind.ClientDisconnected)]
    public static void ClientDisconnected(ReducerContext ctx)
    {
        var user = ctx.Db.user.Identity.Find(ctx.Sender);
        if (user != null)
        {
            ctx.Db.user.Identity.Update(new User
            {
                Identity = user.Identity,
                DisplayName = user.DisplayName,
                IsOnline = false,
                LastSeen = ctx.Timestamp
            });
        }

        // Clean up typing indicators for this user
        foreach (var typing in ctx.Db.typing_indicator.Iter())
        {
            if (typing.UserIdentity == ctx.Sender)
            {
                ctx.Db.typing_indicator.Id.Delete(typing.Id);
            }
        }
        Log.Info($"Client disconnected: {ctx.Sender}");
    }

    // ============================================================================
    // USER REDUCERS
    // ============================================================================

    [SpacetimeDB.Reducer]
    public static void SetDisplayName(ReducerContext ctx, string displayName)
    {
        if (string.IsNullOrWhiteSpace(displayName))
        {
            throw new ArgumentException("Display name cannot be empty");
        }
        if (displayName.Length > 32)
        {
            throw new ArgumentException("Display name cannot exceed 32 characters");
        }

        var user = ctx.Db.user.Identity.Find(ctx.Sender);
        if (user == null)
        {
            throw new Exception("User not found. Please reconnect.");
        }

        ctx.Db.user.Identity.Update(new User
        {
            Identity = user.Identity,
            DisplayName = displayName.Trim(),
            IsOnline = user.IsOnline,
            LastSeen = user.LastSeen
        });
        Log.Info($"User {ctx.Sender} set display name to: {displayName}");
    }

    // ============================================================================
    // ROOM REDUCERS
    // ============================================================================

    [SpacetimeDB.Reducer]
    public static void CreateRoom(ReducerContext ctx, string name)
    {
        if (string.IsNullOrWhiteSpace(name))
        {
            throw new ArgumentException("Room name cannot be empty");
        }
        if (name.Length > 50)
        {
            throw new ArgumentException("Room name cannot exceed 50 characters");
        }

        // Check if room with same name exists
        foreach (var room in ctx.Db.room.Iter())
        {
            if (room.Name.Equals(name, StringComparison.OrdinalIgnoreCase))
            {
                throw new ArgumentException($"Room '{name}' already exists");
            }
        }

        var newRoom = ctx.Db.room.Insert(new Room
        {
            Id = 0,  // Auto-increment placeholder
            Name = name.Trim(),
            CreatedBy = ctx.Sender,
            CreatedAt = ctx.Timestamp
        });

        // Auto-join the creator to the room
        ctx.Db.room_member.Insert(new RoomMember
        {
            Id = 0,
            RoomId = newRoom.Id,
            UserIdentity = ctx.Sender,
            JoinedAt = ctx.Timestamp
        });

        Log.Info($"Room created: {name} by {ctx.Sender}");
    }

    [SpacetimeDB.Reducer]
    public static void JoinRoom(ReducerContext ctx, ulong roomId)
    {
        var room = ctx.Db.room.Id.Find(roomId);
        if (room == null)
        {
            throw new Exception("Room not found");
        }

        // Check if already a member
        foreach (var member in ctx.Db.room_member.room_member_room_id.Filter(roomId))
        {
            if (member.UserIdentity == ctx.Sender)
            {
                throw new Exception("Already a member of this room");
            }
        }

        ctx.Db.room_member.Insert(new RoomMember
        {
            Id = 0,
            RoomId = roomId,
            UserIdentity = ctx.Sender,
            JoinedAt = ctx.Timestamp
        });
        Log.Info($"User {ctx.Sender} joined room {roomId}");
    }

    [SpacetimeDB.Reducer]
    public static void LeaveRoom(ReducerContext ctx, ulong roomId)
    {
        RoomMember? memberToRemove = null;
        foreach (var member in ctx.Db.room_member.room_member_room_id.Filter(roomId))
        {
            if (member.UserIdentity == ctx.Sender)
            {
                memberToRemove = member;
                break;
            }
        }

        if (memberToRemove == null)
        {
            throw new Exception("Not a member of this room");
        }

        ctx.Db.room_member.Id.Delete(memberToRemove.Id);

        // Clean up typing indicator if any
        foreach (var typing in ctx.Db.typing_indicator.typing_indicator_room_id.Filter(roomId))
        {
            if (typing.UserIdentity == ctx.Sender)
            {
                ctx.Db.typing_indicator.Id.Delete(typing.Id);
            }
        }

        // Clean up last read
        foreach (var lastRead in ctx.Db.last_read.last_read_user_identity.Filter(ctx.Sender))
        {
            if (lastRead.RoomId == roomId)
            {
                ctx.Db.last_read.Id.Delete(lastRead.Id);
            }
        }

        Log.Info($"User {ctx.Sender} left room {roomId}");
    }

    // ============================================================================
    // MESSAGE REDUCERS
    // ============================================================================

    private static bool IsRoomMember(ReducerContext ctx, ulong roomId)
    {
        foreach (var member in ctx.Db.room_member.room_member_room_id.Filter(roomId))
        {
            if (member.UserIdentity == ctx.Sender)
            {
                return true;
            }
        }
        return false;
    }

    [SpacetimeDB.Reducer]
    public static void SendMessage(ReducerContext ctx, ulong roomId, string content)
    {
        if (string.IsNullOrWhiteSpace(content))
        {
            throw new ArgumentException("Message cannot be empty");
        }
        if (content.Length > 2000)
        {
            throw new ArgumentException("Message cannot exceed 2000 characters");
        }

        var room = ctx.Db.room.Id.Find(roomId);
        if (room == null)
        {
            throw new Exception("Room not found");
        }

        if (!IsRoomMember(ctx, roomId))
        {
            throw new Exception("Must be a member of the room to send messages");
        }

        // Rate limiting: check if user sent a message in the last 500ms
        long rateLimit = 500000; // 500ms in microseconds
        foreach (var msg in ctx.Db.message.message_room_id.Filter(roomId))
        {
            if (msg.SenderIdentity == ctx.Sender)
            {
                var timeSince = ctx.Timestamp.MicrosecondsSinceUnixEpoch - msg.CreatedAt.MicrosecondsSinceUnixEpoch;
                if (timeSince < rateLimit)
                {
                    throw new Exception("Please wait before sending another message");
                }
            }
        }

        ctx.Db.message.Insert(new Message
        {
            Id = 0,
            RoomId = roomId,
            SenderIdentity = ctx.Sender,
            Content = content.Trim(),
            CreatedAt = ctx.Timestamp,
            IsEdited = false,
            IsEphemeral = false,
            ExpiresAtMicros = 0
        });

        // Clear typing indicator
        foreach (var typing in ctx.Db.typing_indicator.typing_indicator_room_id.Filter(roomId))
        {
            if (typing.UserIdentity == ctx.Sender)
            {
                ctx.Db.typing_indicator.Id.Delete(typing.Id);
            }
        }

        Log.Info($"Message sent in room {roomId} by {ctx.Sender}: {content.Substring(0, Math.Min(50, content.Length))}");
    }

    // ============================================================================
    // MESSAGE EDITING WITH HISTORY
    // ============================================================================

    [SpacetimeDB.Reducer]
    public static void EditMessage(ReducerContext ctx, ulong messageId, string newContent)
    {
        if (string.IsNullOrWhiteSpace(newContent))
        {
            throw new ArgumentException("Message cannot be empty");
        }
        if (newContent.Length > 2000)
        {
            throw new ArgumentException("Message cannot exceed 2000 characters");
        }

        var message = ctx.Db.message.Id.Find(messageId);
        if (message == null)
        {
            throw new Exception("Message not found");
        }

        if (message.SenderIdentity != ctx.Sender)
        {
            throw new Exception("Can only edit your own messages");
        }

        if (message.IsEphemeral)
        {
            throw new Exception("Cannot edit ephemeral messages");
        }

        // Store the previous content in edit history
        ctx.Db.message_edit.Insert(new MessageEdit
        {
            Id = 0,
            MessageId = messageId,
            PreviousContent = message.Content,
            EditedAt = ctx.Timestamp
        });

        // Update the message
        ctx.Db.message.Id.Update(new Message
        {
            Id = message.Id,
            RoomId = message.RoomId,
            SenderIdentity = message.SenderIdentity,
            Content = newContent.Trim(),
            CreatedAt = message.CreatedAt,
            IsEdited = true,
            IsEphemeral = message.IsEphemeral,
            ExpiresAtMicros = message.ExpiresAtMicros
        });

        Log.Info($"Message {messageId} edited by {ctx.Sender}");
    }

    // ============================================================================
    // TYPING INDICATORS
    // ============================================================================

    [SpacetimeDB.Reducer]
    public static void StartTyping(ReducerContext ctx, ulong roomId)
    {
        if (!IsRoomMember(ctx, roomId))
        {
            throw new Exception("Must be a member of the room");
        }

        // Check if already typing
        foreach (var typing in ctx.Db.typing_indicator.typing_indicator_room_id.Filter(roomId))
        {
            if (typing.UserIdentity == ctx.Sender)
            {
                // Update the timestamp
                ctx.Db.typing_indicator.Id.Update(new TypingIndicator
                {
                    Id = typing.Id,
                    RoomId = typing.RoomId,
                    UserIdentity = typing.UserIdentity,
                    StartedAt = ctx.Timestamp
                });
                return;
            }
        }

        // Create new typing indicator
        var indicator = ctx.Db.typing_indicator.Insert(new TypingIndicator
        {
            Id = 0,
            RoomId = roomId,
            UserIdentity = ctx.Sender,
            StartedAt = ctx.Timestamp
        });

        // Schedule cleanup after 5 seconds
        var expiresAt = ctx.Timestamp.MicrosecondsSinceUnixEpoch + (5 * 1000000);  // 5 seconds
        ctx.Db.typing_cleanup.Insert(new TypingCleanup
        {
            ScheduledId = 0,
            ScheduledAt = new ScheduleAt.Time(new Timestamp(expiresAt)),
            TypingIndicatorId = indicator.Id
        });
    }

    [SpacetimeDB.Reducer]
    public static void StopTyping(ReducerContext ctx, ulong roomId)
    {
        foreach (var typing in ctx.Db.typing_indicator.typing_indicator_room_id.Filter(roomId))
        {
            if (typing.UserIdentity == ctx.Sender)
            {
                ctx.Db.typing_indicator.Id.Delete(typing.Id);
                return;
            }
        }
    }

    [SpacetimeDB.Reducer]
    public static void CleanupTypingIndicator(ReducerContext ctx, TypingCleanup job)
    {
        // The typing indicator might have been updated or deleted
        var indicator = ctx.Db.typing_indicator.Id.Find(job.TypingIndicatorId);
        if (indicator != null)
        {
            // Only delete if it hasn't been updated recently
            var age = ctx.Timestamp.MicrosecondsSinceUnixEpoch - indicator.StartedAt.MicrosecondsSinceUnixEpoch;
            if (age >= 5 * 1000000)  // 5 seconds
            {
                ctx.Db.typing_indicator.Id.Delete(indicator.Id);
            }
        }
    }

    // ============================================================================
    // READ RECEIPTS
    // ============================================================================

    [SpacetimeDB.Reducer]
    public static void MarkMessageRead(ReducerContext ctx, ulong messageId)
    {
        var message = ctx.Db.message.Id.Find(messageId);
        if (message == null)
        {
            throw new Exception("Message not found");
        }

        if (!IsRoomMember(ctx, message.RoomId))
        {
            throw new Exception("Must be a member of the room");
        }

        // Check if already read
        foreach (var receipt in ctx.Db.read_receipt.read_receipt_message_id.Filter(messageId))
        {
            if (receipt.UserIdentity == ctx.Sender)
            {
                return;  // Already marked as read
            }
        }

        ctx.Db.read_receipt.Insert(new ReadReceipt
        {
            Id = 0,
            MessageId = messageId,
            UserIdentity = ctx.Sender,
            ReadAt = ctx.Timestamp
        });
    }

    // ============================================================================
    // UNREAD MESSAGE COUNTS (Last Read Position)
    // ============================================================================

    [SpacetimeDB.Reducer]
    public static void UpdateLastRead(ReducerContext ctx, ulong roomId, ulong lastMessageId)
    {
        if (!IsRoomMember(ctx, roomId))
        {
            throw new Exception("Must be a member of the room");
        }

        var message = ctx.Db.message.Id.Find(lastMessageId);
        if (message == null || message.RoomId != roomId)
        {
            throw new Exception("Invalid message for this room");
        }

        // Find existing last read record
        LastRead? existing = null;
        foreach (var lr in ctx.Db.last_read.last_read_user_identity.Filter(ctx.Sender))
        {
            if (lr.RoomId == roomId)
            {
                existing = lr;
                break;
            }
        }

        if (existing != null)
        {
            ctx.Db.last_read.Id.Update(new LastRead
            {
                Id = existing.Id,
                RoomId = roomId,
                UserIdentity = ctx.Sender,
                LastMessageId = lastMessageId,
                LastReadAt = ctx.Timestamp
            });
        }
        else
        {
            ctx.Db.last_read.Insert(new LastRead
            {
                Id = 0,
                RoomId = roomId,
                UserIdentity = ctx.Sender,
                LastMessageId = lastMessageId,
                LastReadAt = ctx.Timestamp
            });
        }
    }

    // ============================================================================
    // SCHEDULED MESSAGES
    // ============================================================================

    [SpacetimeDB.Reducer]
    public static void ScheduleMessage(ReducerContext ctx, ulong roomId, string content, ulong delayMs)
    {
        if (string.IsNullOrWhiteSpace(content))
        {
            throw new ArgumentException("Message cannot be empty");
        }
        if (content.Length > 2000)
        {
            throw new ArgumentException("Message cannot exceed 2000 characters");
        }
        if (delayMs < 1000)
        {
            throw new ArgumentException("Delay must be at least 1 second");
        }
        if (delayMs > 7 * 24 * 60 * 60 * 1000)  // 7 days
        {
            throw new ArgumentException("Delay cannot exceed 7 days");
        }

        var room = ctx.Db.room.Id.Find(roomId);
        if (room == null)
        {
            throw new Exception("Room not found");
        }

        if (!IsRoomMember(ctx, roomId))
        {
            throw new Exception("Must be a member of the room");
        }

        var scheduledTime = ctx.Timestamp.MicrosecondsSinceUnixEpoch + (long)(delayMs * 1000);
        ctx.Db.scheduled_message.Insert(new ScheduledMessage
        {
            ScheduledId = 0,
            ScheduledAt = new ScheduleAt.Time(new Timestamp(scheduledTime)),
            RoomId = roomId,
            SenderIdentity = ctx.Sender,
            Content = content.Trim()
        });

        Log.Info($"Message scheduled for {delayMs}ms by {ctx.Sender} in room {roomId}");
    }

    [SpacetimeDB.Reducer]
    public static void CancelScheduledMessage(ReducerContext ctx, ulong scheduledId)
    {
        var scheduled = ctx.Db.scheduled_message.ScheduledId.Find(scheduledId);
        if (scheduled == null)
        {
            throw new Exception("Scheduled message not found");
        }

        if (scheduled.SenderIdentity != ctx.Sender)
        {
            throw new Exception("Can only cancel your own scheduled messages");
        }

        ctx.Db.scheduled_message.ScheduledId.Delete(scheduledId);
        Log.Info($"Scheduled message {scheduledId} cancelled by {ctx.Sender}");
    }

    [SpacetimeDB.Reducer]
    public static void SendScheduledMessage(ReducerContext ctx, ScheduledMessage job)
    {
        // Verify room still exists and user is still a member
        var room = ctx.Db.room.Id.Find(job.RoomId);
        if (room == null)
        {
            Log.Warn($"Scheduled message room {job.RoomId} no longer exists");
            return;
        }

        bool isMember = false;
        foreach (var member in ctx.Db.room_member.room_member_room_id.Filter(job.RoomId))
        {
            if (member.UserIdentity == job.SenderIdentity)
            {
                isMember = true;
                break;
            }
        }

        if (!isMember)
        {
            Log.Warn($"Scheduled message sender {job.SenderIdentity} is no longer a member of room {job.RoomId}");
            return;
        }

        // Send the message
        ctx.Db.message.Insert(new Message
        {
            Id = 0,
            RoomId = job.RoomId,
            SenderIdentity = job.SenderIdentity,
            Content = job.Content,
            CreatedAt = ctx.Timestamp,
            IsEdited = false,
            IsEphemeral = false,
            ExpiresAtMicros = 0
        });

        Log.Info($"Scheduled message delivered in room {job.RoomId}");
    }

    // ============================================================================
    // EPHEMERAL/DISAPPEARING MESSAGES
    // ============================================================================

    [SpacetimeDB.Reducer]
    public static void SendEphemeralMessage(ReducerContext ctx, ulong roomId, string content, ulong lifetimeMs)
    {
        if (string.IsNullOrWhiteSpace(content))
        {
            throw new ArgumentException("Message cannot be empty");
        }
        if (content.Length > 2000)
        {
            throw new ArgumentException("Message cannot exceed 2000 characters");
        }
        if (lifetimeMs < 10000)  // 10 seconds minimum
        {
            throw new ArgumentException("Lifetime must be at least 10 seconds");
        }
        if (lifetimeMs > 60 * 60 * 1000)  // 1 hour max
        {
            throw new ArgumentException("Lifetime cannot exceed 1 hour");
        }

        var room = ctx.Db.room.Id.Find(roomId);
        if (room == null)
        {
            throw new Exception("Room not found");
        }

        if (!IsRoomMember(ctx, roomId))
        {
            throw new Exception("Must be a member of the room");
        }

        var expiresAt = ctx.Timestamp.MicrosecondsSinceUnixEpoch + (long)(lifetimeMs * 1000);
        var newMessage = ctx.Db.message.Insert(new Message
        {
            Id = 0,
            RoomId = roomId,
            SenderIdentity = ctx.Sender,
            Content = content.Trim(),
            CreatedAt = ctx.Timestamp,
            IsEdited = false,
            IsEphemeral = true,
            ExpiresAtMicros = (ulong)expiresAt
        });

        // Schedule cleanup
        ctx.Db.ephemeral_cleanup.Insert(new EphemeralCleanup
        {
            ScheduledId = 0,
            ScheduledAt = new ScheduleAt.Time(new Timestamp(expiresAt)),
            MessageId = newMessage.Id
        });

        // Clear typing indicator
        foreach (var typing in ctx.Db.typing_indicator.typing_indicator_room_id.Filter(roomId))
        {
            if (typing.UserIdentity == ctx.Sender)
            {
                ctx.Db.typing_indicator.Id.Delete(typing.Id);
            }
        }

        Log.Info($"Ephemeral message sent in room {roomId}, expires in {lifetimeMs}ms");
    }

    [SpacetimeDB.Reducer]
    public static void CleanupEphemeralMessage(ReducerContext ctx, EphemeralCleanup job)
    {
        var message = ctx.Db.message.Id.Find(job.MessageId);
        if (message != null && message.IsEphemeral)
        {
            // Delete associated read receipts
            foreach (var receipt in ctx.Db.read_receipt.read_receipt_message_id.Filter(job.MessageId))
            {
                ctx.Db.read_receipt.Id.Delete(receipt.Id);
            }

            // Delete associated reactions
            foreach (var reaction in ctx.Db.reaction.reaction_message_id.Filter(job.MessageId))
            {
                ctx.Db.reaction.Id.Delete(reaction.Id);
            }

            // Delete the message
            ctx.Db.message.Id.Delete(job.MessageId);
            Log.Info($"Ephemeral message {job.MessageId} deleted");
        }
    }

    // ============================================================================
    // MESSAGE REACTIONS
    // ============================================================================

    private static readonly string[] AllowedEmojis = { "👍", "❤️", "😂", "😮", "😢", "🎉", "🔥", "👀" };

    [SpacetimeDB.Reducer]
    public static void ToggleReaction(ReducerContext ctx, ulong messageId, string emoji)
    {
        if (string.IsNullOrEmpty(emoji))
        {
            throw new ArgumentException("Emoji cannot be empty");
        }

        bool isAllowed = false;
        foreach (var allowed in AllowedEmojis)
        {
            if (allowed == emoji)
            {
                isAllowed = true;
                break;
            }
        }
        if (!isAllowed)
        {
            throw new ArgumentException($"Emoji '{emoji}' is not allowed");
        }

        var message = ctx.Db.message.Id.Find(messageId);
        if (message == null)
        {
            throw new Exception("Message not found");
        }

        if (!IsRoomMember(ctx, message.RoomId))
        {
            throw new Exception("Must be a member of the room");
        }

        // Check if reaction already exists
        Reaction? existingReaction = null;
        foreach (var reaction in ctx.Db.reaction.reaction_message_id.Filter(messageId))
        {
            if (reaction.UserIdentity == ctx.Sender && reaction.Emoji == emoji)
            {
                existingReaction = reaction;
                break;
            }
        }

        if (existingReaction != null)
        {
            // Remove reaction (toggle off)
            ctx.Db.reaction.Id.Delete(existingReaction.Id);
            Log.Info($"Reaction {emoji} removed from message {messageId} by {ctx.Sender}");
        }
        else
        {
            // Add reaction (toggle on)
            ctx.Db.reaction.Insert(new Reaction
            {
                Id = 0,
                MessageId = messageId,
                UserIdentity = ctx.Sender,
                Emoji = emoji,
                CreatedAt = ctx.Timestamp
            });
            Log.Info($"Reaction {emoji} added to message {messageId} by {ctx.Sender}");
        }
    }
}

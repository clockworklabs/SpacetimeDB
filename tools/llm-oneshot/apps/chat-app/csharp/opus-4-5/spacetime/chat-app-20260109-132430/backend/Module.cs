using SpacetimeDB;

public static partial class Module
{
    // ========================================================================
    // CONSTANTS
    // ========================================================================
    
    private const int MaxDisplayNameLength = 50;
    private const int MaxRoomNameLength = 100;
    private const int MaxMessageLength = 2000;
    private const int TypingExpiryMs = 3000; // 3 seconds

    // ========================================================================
    // LIFECYCLE HOOKS
    // ========================================================================

    [SpacetimeDB.Reducer(ReducerKind.ClientConnected)]
    public static void ClientConnected(ReducerContext ctx)
    {
        var existing = ctx.Db.User.Identity.Find(ctx.Sender);
        if (existing != null)
        {
            var user = existing.Value;
            ctx.Db.User.Identity.Update(new User
            {
                Identity = user.Identity,
                DisplayName = user.DisplayName,
                Online = true,
                LastSeen = ctx.Timestamp
            });
        }
        else
        {
            ctx.Db.User.Insert(new User
            {
                Identity = ctx.Sender,
                DisplayName = $"User_{ctx.Sender.ToString().Substring(0, 8)}",
                Online = true,
                LastSeen = ctx.Timestamp
            });
        }
        Log.Info($"Client connected: {ctx.Sender}");
    }

    [SpacetimeDB.Reducer(ReducerKind.ClientDisconnected)]
    public static void ClientDisconnected(ReducerContext ctx)
    {
        var existing = ctx.Db.User.Identity.Find(ctx.Sender);
        if (existing != null)
        {
            var user = existing.Value;
            ctx.Db.User.Identity.Update(new User
            {
                Identity = user.Identity,
                DisplayName = user.DisplayName,
                Online = false,
                LastSeen = ctx.Timestamp
            });
        }

        // Clean up typing indicators for this user
        foreach (var typing in ctx.Db.TypingIndicator.Iter().Where(t => t.UserId == ctx.Sender).ToList())
        {
            ctx.Db.TypingIndicator.Id.Delete(typing.Id);
        }

        Log.Info($"Client disconnected: {ctx.Sender}");
    }

    // ========================================================================
    // USER REDUCERS
    // ========================================================================

    [SpacetimeDB.Reducer]
    public static void SetDisplayName(ReducerContext ctx, string displayName)
    {
        displayName = displayName.Trim();
        
        if (string.IsNullOrEmpty(displayName))
        {
            throw new ArgumentException("Display name cannot be empty");
        }
        
        if (displayName.Length > MaxDisplayNameLength)
        {
            throw new ArgumentException($"Display name cannot exceed {MaxDisplayNameLength} characters");
        }

        var existing = ctx.Db.User.Identity.Find(ctx.Sender);
        if (existing == null)
        {
            throw new Exception("User not found");
        }

        var user = existing.Value;
        ctx.Db.User.Identity.Update(new User
        {
            Identity = user.Identity,
            DisplayName = displayName,
            Online = user.Online,
            LastSeen = ctx.Timestamp
        });

        Log.Info($"User {ctx.Sender} set display name to: {displayName}");
    }

    // ========================================================================
    // ROOM REDUCERS
    // ========================================================================

    [SpacetimeDB.Reducer]
    public static void CreateRoom(ReducerContext ctx, string roomName)
    {
        roomName = roomName.Trim();
        
        if (string.IsNullOrEmpty(roomName))
        {
            throw new ArgumentException("Room name cannot be empty");
        }
        
        if (roomName.Length > MaxRoomNameLength)
        {
            throw new ArgumentException($"Room name cannot exceed {MaxRoomNameLength} characters");
        }

        var room = ctx.Db.Room.Insert(new Room
        {
            Id = 0,
            Name = roomName,
            CreatedBy = ctx.Sender,
            CreatedAt = ctx.Timestamp
        });

        ctx.Db.RoomMember.Insert(new RoomMember
        {
            Id = 0,
            RoomId = room.Id,
            UserId = ctx.Sender,
            JoinedAt = ctx.Timestamp,
            LastReadAt = ctx.Timestamp
        });

        Log.Info($"Room created: {roomName} (ID: {room.Id}) by {ctx.Sender}");
    }

    [SpacetimeDB.Reducer]
    public static void JoinRoom(ReducerContext ctx, ulong roomId)
    {
        var roomResult = ctx.Db.Room.Id.Find(roomId);
        if (roomResult == null)
        {
            throw new Exception("Room not found");
        }

        var existingMember = ctx.Db.RoomMember.Iter()
            .FirstOrDefault(m => m.RoomId == roomId && m.UserId == ctx.Sender);
        
        if (existingMember.Id != 0)
        {
            throw new Exception("Already a member of this room");
        }

        ctx.Db.RoomMember.Insert(new RoomMember
        {
            Id = 0,
            RoomId = roomId,
            UserId = ctx.Sender,
            JoinedAt = ctx.Timestamp,
            LastReadAt = ctx.Timestamp
        });

        Log.Info($"User {ctx.Sender} joined room {roomId}");
    }

    [SpacetimeDB.Reducer]
    public static void LeaveRoom(ReducerContext ctx, ulong roomId)
    {
        var membership = ctx.Db.RoomMember.Iter()
            .FirstOrDefault(m => m.RoomId == roomId && m.UserId == ctx.Sender);
        
        if (membership.Id == 0)
        {
            throw new Exception("Not a member of this room");
        }

        ctx.Db.RoomMember.Id.Delete(membership.Id);

        foreach (var typing in ctx.Db.TypingIndicator.Iter()
            .Where(t => t.RoomId == roomId && t.UserId == ctx.Sender).ToList())
        {
            ctx.Db.TypingIndicator.Id.Delete(typing.Id);
        }

        Log.Info($"User {ctx.Sender} left room {roomId}");
    }

    // ========================================================================
    // MESSAGE REDUCERS
    // ========================================================================

    [SpacetimeDB.Reducer]
    public static void SendMessage(ReducerContext ctx, ulong roomId, string content)
    {
        content = content.Trim();
        
        if (string.IsNullOrEmpty(content))
        {
            throw new ArgumentException("Message cannot be empty");
        }
        
        if (content.Length > MaxMessageLength)
        {
            throw new ArgumentException($"Message cannot exceed {MaxMessageLength} characters");
        }

        var membership = ctx.Db.RoomMember.Iter()
            .FirstOrDefault(m => m.RoomId == roomId && m.UserId == ctx.Sender);
        
        if (membership.Id == 0)
        {
            throw new Exception("Must be a member of the room to send messages");
        }

        var msg = ctx.Db.Message.Insert(new Message
        {
            Id = 0,
            RoomId = roomId,
            SenderId = ctx.Sender,
            Content = content,
            CreatedAt = ctx.Timestamp,
            IsEdited = false,
            IsEphemeral = false,
            ExpiresAt = null
        });

        foreach (var typing in ctx.Db.TypingIndicator.Iter()
            .Where(t => t.RoomId == roomId && t.UserId == ctx.Sender).ToList())
        {
            ctx.Db.TypingIndicator.Id.Delete(typing.Id);
        }

        ctx.Db.MessageRead.Insert(new MessageRead
        {
            Id = 0,
            MessageId = msg.Id,
            UserId = ctx.Sender,
            ReadAt = ctx.Timestamp
        });

        ctx.Db.RoomMember.Id.Update(new RoomMember
        {
            Id = membership.Id,
            RoomId = membership.RoomId,
            UserId = membership.UserId,
            JoinedAt = membership.JoinedAt,
            LastReadAt = ctx.Timestamp
        });

        Log.Info($"Message sent in room {roomId} by {ctx.Sender}");
    }

    [SpacetimeDB.Reducer]
    public static void SendEphemeralMessage(ReducerContext ctx, ulong roomId, string content, ulong durationMs)
    {
        content = content.Trim();
        
        if (string.IsNullOrEmpty(content))
        {
            throw new ArgumentException("Message cannot be empty");
        }
        
        if (content.Length > MaxMessageLength)
        {
            throw new ArgumentException($"Message cannot exceed {MaxMessageLength} characters");
        }

        if (durationMs < 10000 || durationMs > 600000)
        {
            throw new ArgumentException("Ephemeral duration must be between 10 seconds and 10 minutes");
        }

        var membership = ctx.Db.RoomMember.Iter()
            .FirstOrDefault(m => m.RoomId == roomId && m.UserId == ctx.Sender);
        
        if (membership.Id == 0)
        {
            throw new Exception("Must be a member of the room to send messages");
        }

        var expiresAt = new Timestamp(ctx.Timestamp.MicrosecondsSinceUnixEpoch + (long)(durationMs * 1000));

        var msg = ctx.Db.Message.Insert(new Message
        {
            Id = 0,
            RoomId = roomId,
            SenderId = ctx.Sender,
            Content = content,
            CreatedAt = ctx.Timestamp,
            IsEdited = false,
            IsEphemeral = true,
            ExpiresAt = expiresAt
        });

        ctx.Db.ScheduledDeletion.Insert(new ScheduledDeletion
        {
            ScheduledId = 0,
            ScheduledAt = new ScheduleAt.Time(expiresAt),
            MessageId = msg.Id
        });

        foreach (var typing in ctx.Db.TypingIndicator.Iter()
            .Where(t => t.RoomId == roomId && t.UserId == ctx.Sender).ToList())
        {
            ctx.Db.TypingIndicator.Id.Delete(typing.Id);
        }

        Log.Info($"Ephemeral message sent in room {roomId}, expires in {durationMs}ms");
    }

    [SpacetimeDB.Reducer]
    public static void DeleteEphemeralMessage(ReducerContext ctx, ScheduledDeletion job)
    {
        var msgResult = ctx.Db.Message.Id.Find(job.MessageId);
        if (msgResult != null)
        {
            var msg = msgResult.Value;
            if (msg.IsEphemeral)
            {
                foreach (var reaction in ctx.Db.Reaction.MessageId.Filter(job.MessageId).ToList())
                {
                    ctx.Db.Reaction.Id.Delete(reaction.Id);
                }

                foreach (var read in ctx.Db.MessageRead.MessageId.Filter(job.MessageId).ToList())
                {
                    ctx.Db.MessageRead.Id.Delete(read.Id);
                }

                foreach (var edit in ctx.Db.MessageEdit.MessageId.Filter(job.MessageId).ToList())
                {
                    ctx.Db.MessageEdit.Id.Delete(edit.Id);
                }

                ctx.Db.Message.Id.Delete(job.MessageId);
                Log.Info($"Ephemeral message {job.MessageId} deleted");
            }
        }
    }

    [SpacetimeDB.Reducer]
    public static void EditMessage(ReducerContext ctx, ulong messageId, string newContent)
    {
        newContent = newContent.Trim();
        
        if (string.IsNullOrEmpty(newContent))
        {
            throw new ArgumentException("Message cannot be empty");
        }
        
        if (newContent.Length > MaxMessageLength)
        {
            throw new ArgumentException($"Message cannot exceed {MaxMessageLength} characters");
        }

        var msgResult = ctx.Db.Message.Id.Find(messageId);
        if (msgResult == null)
        {
            throw new Exception("Message not found");
        }

        var msg = msgResult.Value;

        if (msg.SenderId != ctx.Sender)
        {
            throw new Exception("Can only edit your own messages");
        }

        ctx.Db.MessageEdit.Insert(new MessageEdit
        {
            Id = 0,
            MessageId = messageId,
            OldContent = msg.Content,
            EditedAt = ctx.Timestamp
        });

        ctx.Db.Message.Id.Update(new Message
        {
            Id = msg.Id,
            RoomId = msg.RoomId,
            SenderId = msg.SenderId,
            Content = newContent,
            CreatedAt = msg.CreatedAt,
            IsEdited = true,
            IsEphemeral = msg.IsEphemeral,
            ExpiresAt = msg.ExpiresAt
        });

        Log.Info($"Message {messageId} edited by {ctx.Sender}");
    }

    // ========================================================================
    // SCHEDULED MESSAGE REDUCERS
    // ========================================================================

    [SpacetimeDB.Reducer]
    public static void ScheduleMessage(ReducerContext ctx, ulong roomId, string content, long scheduledTimeMs)
    {
        content = content.Trim();
        
        if (string.IsNullOrEmpty(content))
        {
            throw new ArgumentException("Message cannot be empty");
        }
        
        if (content.Length > MaxMessageLength)
        {
            throw new ArgumentException($"Message cannot exceed {MaxMessageLength} characters");
        }

        var membership = ctx.Db.RoomMember.Iter()
            .FirstOrDefault(m => m.RoomId == roomId && m.UserId == ctx.Sender);
        
        if (membership.Id == 0)
        {
            throw new Exception("Must be a member of the room to schedule messages");
        }

        var scheduledMicros = scheduledTimeMs * 1000;
        var currentMicros = ctx.Timestamp.MicrosecondsSinceUnixEpoch;

        if (scheduledMicros <= currentMicros)
        {
            throw new ArgumentException("Scheduled time must be in the future");
        }

        ctx.Db.ScheduledMessage.Insert(new ScheduledMessage
        {
            ScheduledId = 0,
            ScheduledAt = new ScheduleAt.Time(new Timestamp(scheduledMicros)),
            RoomId = roomId,
            SenderId = ctx.Sender,
            Content = content,
            CreatedAt = ctx.Timestamp
        });

        Log.Info($"Message scheduled for room {roomId} by {ctx.Sender}");
    }

    [SpacetimeDB.Reducer]
    public static void CancelScheduledMessage(ReducerContext ctx, ulong scheduledId)
    {
        var schedResult = ctx.Db.ScheduledMessage.ScheduledId.Find(scheduledId);
        if (schedResult == null)
        {
            throw new Exception("Scheduled message not found");
        }

        var sched = schedResult.Value;

        if (sched.SenderId != ctx.Sender)
        {
            throw new Exception("Can only cancel your own scheduled messages");
        }

        ctx.Db.ScheduledMessage.ScheduledId.Delete(scheduledId);
        Log.Info($"Scheduled message {scheduledId} cancelled by {ctx.Sender}");
    }

    [SpacetimeDB.Reducer]
    public static void SendScheduledMessage(ReducerContext ctx, ScheduledMessage job)
    {
        var msg = ctx.Db.Message.Insert(new Message
        {
            Id = 0,
            RoomId = job.RoomId,
            SenderId = job.SenderId,
            Content = job.Content,
            CreatedAt = ctx.Timestamp,
            IsEdited = false,
            IsEphemeral = false,
            ExpiresAt = null
        });

        ctx.Db.MessageRead.Insert(new MessageRead
        {
            Id = 0,
            MessageId = msg.Id,
            UserId = job.SenderId,
            ReadAt = ctx.Timestamp
        });

        Log.Info($"Scheduled message delivered to room {job.RoomId}");
    }

    // ========================================================================
    // TYPING INDICATOR REDUCERS
    // ========================================================================

    [SpacetimeDB.Reducer]
    public static void SetTyping(ReducerContext ctx, ulong roomId)
    {
        var membership = ctx.Db.RoomMember.Iter()
            .FirstOrDefault(m => m.RoomId == roomId && m.UserId == ctx.Sender);
        
        if (membership.Id == 0)
        {
            throw new Exception("Must be a member of the room");
        }

        var expiresAt = new Timestamp(ctx.Timestamp.MicrosecondsSinceUnixEpoch + (TypingExpiryMs * 1000));

        var existing = ctx.Db.TypingIndicator.Iter()
            .FirstOrDefault(t => t.RoomId == roomId && t.UserId == ctx.Sender);

        if (existing.Id != 0)
        {
            ctx.Db.TypingIndicator.Id.Update(new TypingIndicator
            {
                Id = existing.Id,
                RoomId = existing.RoomId,
                UserId = existing.UserId,
                ExpiresAt = expiresAt
            });
        }
        else
        {
            var typing = ctx.Db.TypingIndicator.Insert(new TypingIndicator
            {
                Id = 0,
                RoomId = roomId,
                UserId = ctx.Sender,
                ExpiresAt = expiresAt
            });

            ctx.Db.ScheduledTypingCleanup.Insert(new ScheduledTypingCleanup
            {
                ScheduledId = 0,
                ScheduledAt = new ScheduleAt.Time(expiresAt),
                TypingId = typing.Id
            });
        }
    }

    [SpacetimeDB.Reducer]
    public static void ClearTyping(ReducerContext ctx, ulong roomId)
    {
        foreach (var typing in ctx.Db.TypingIndicator.Iter()
            .Where(t => t.RoomId == roomId && t.UserId == ctx.Sender).ToList())
        {
            ctx.Db.TypingIndicator.Id.Delete(typing.Id);
        }
    }

    [SpacetimeDB.Reducer]
    public static void CleanupTypingIndicator(ReducerContext ctx, ScheduledTypingCleanup job)
    {
        var typingResult = ctx.Db.TypingIndicator.Id.Find(job.TypingId);
        if (typingResult != null)
        {
            var typing = typingResult.Value;
            if (typing.ExpiresAt.MicrosecondsSinceUnixEpoch <= ctx.Timestamp.MicrosecondsSinceUnixEpoch)
            {
                ctx.Db.TypingIndicator.Id.Delete(job.TypingId);
            }
        }
    }

    // ========================================================================
    // READ RECEIPT REDUCERS
    // ========================================================================

    [SpacetimeDB.Reducer]
    public static void MarkMessageRead(ReducerContext ctx, ulong messageId)
    {
        var msgResult = ctx.Db.Message.Id.Find(messageId);
        if (msgResult == null)
        {
            throw new Exception("Message not found");
        }

        var msg = msgResult.Value;

        var membership = ctx.Db.RoomMember.Iter()
            .FirstOrDefault(m => m.RoomId == msg.RoomId && m.UserId == ctx.Sender);
        
        if (membership.Id == 0)
        {
            throw new Exception("Must be a member of the room");
        }

        var existingRead = ctx.Db.MessageRead.Iter()
            .FirstOrDefault(r => r.MessageId == messageId && r.UserId == ctx.Sender);

        if (existingRead.Id == 0)
        {
            ctx.Db.MessageRead.Insert(new MessageRead
            {
                Id = 0,
                MessageId = messageId,
                UserId = ctx.Sender,
                ReadAt = ctx.Timestamp
            });
        }
    }

    [SpacetimeDB.Reducer]
    public static void MarkRoomRead(ReducerContext ctx, ulong roomId)
    {
        var membership = ctx.Db.RoomMember.Iter()
            .FirstOrDefault(m => m.RoomId == roomId && m.UserId == ctx.Sender);
        
        if (membership.Id == 0)
        {
            throw new Exception("Must be a member of the room");
        }

        ctx.Db.RoomMember.Id.Update(new RoomMember
        {
            Id = membership.Id,
            RoomId = membership.RoomId,
            UserId = membership.UserId,
            JoinedAt = membership.JoinedAt,
            LastReadAt = ctx.Timestamp
        });

        foreach (var msg in ctx.Db.Message.RoomId.Filter(roomId))
        {
            var existingRead = ctx.Db.MessageRead.Iter()
                .FirstOrDefault(r => r.MessageId == msg.Id && r.UserId == ctx.Sender);

            if (existingRead.Id == 0)
            {
                ctx.Db.MessageRead.Insert(new MessageRead
                {
                    Id = 0,
                    MessageId = msg.Id,
                    UserId = ctx.Sender,
                    ReadAt = ctx.Timestamp
                });
            }
        }
    }

    // ========================================================================
    // REACTION REDUCERS
    // ========================================================================

    [SpacetimeDB.Reducer]
    public static void AddReaction(ReducerContext ctx, ulong messageId, string emoji)
    {
        if (string.IsNullOrEmpty(emoji))
        {
            throw new ArgumentException("Emoji cannot be empty");
        }

        var msgResult = ctx.Db.Message.Id.Find(messageId);
        if (msgResult == null)
        {
            throw new Exception("Message not found");
        }

        var msg = msgResult.Value;

        var membership = ctx.Db.RoomMember.Iter()
            .FirstOrDefault(m => m.RoomId == msg.RoomId && m.UserId == ctx.Sender);
        
        if (membership.Id == 0)
        {
            throw new Exception("Must be a member of the room to react");
        }

        var existingReaction = ctx.Db.Reaction.Iter()
            .FirstOrDefault(r => r.MessageId == messageId && r.UserId == ctx.Sender && r.Emoji == emoji);

        if (existingReaction.Id != 0)
        {
            throw new Exception("Already reacted with this emoji");
        }

        ctx.Db.Reaction.Insert(new Reaction
        {
            Id = 0,
            MessageId = messageId,
            UserId = ctx.Sender,
            Emoji = emoji,
            CreatedAt = ctx.Timestamp
        });

        Log.Info($"Reaction {emoji} added to message {messageId} by {ctx.Sender}");
    }

    [SpacetimeDB.Reducer]
    public static void RemoveReaction(ReducerContext ctx, ulong messageId, string emoji)
    {
        var reaction = ctx.Db.Reaction.Iter()
            .FirstOrDefault(r => r.MessageId == messageId && r.UserId == ctx.Sender && r.Emoji == emoji);

        if (reaction.Id == 0)
        {
            throw new Exception("Reaction not found");
        }

        ctx.Db.Reaction.Id.Delete(reaction.Id);
        Log.Info($"Reaction {emoji} removed from message {messageId} by {ctx.Sender}");
    }

    [SpacetimeDB.Reducer]
    public static void ToggleReaction(ReducerContext ctx, ulong messageId, string emoji)
    {
        if (string.IsNullOrEmpty(emoji))
        {
            throw new ArgumentException("Emoji cannot be empty");
        }

        var msgResult = ctx.Db.Message.Id.Find(messageId);
        if (msgResult == null)
        {
            throw new Exception("Message not found");
        }

        var msg = msgResult.Value;

        var membership = ctx.Db.RoomMember.Iter()
            .FirstOrDefault(m => m.RoomId == msg.RoomId && m.UserId == ctx.Sender);
        
        if (membership.Id == 0)
        {
            throw new Exception("Must be a member of the room to react");
        }

        var existingReaction = ctx.Db.Reaction.Iter()
            .FirstOrDefault(r => r.MessageId == messageId && r.UserId == ctx.Sender && r.Emoji == emoji);

        if (existingReaction.Id != 0)
        {
            ctx.Db.Reaction.Id.Delete(existingReaction.Id);
            Log.Info($"Reaction {emoji} removed from message {messageId} by {ctx.Sender}");
        }
        else
        {
            ctx.Db.Reaction.Insert(new Reaction
            {
                Id = 0,
                MessageId = messageId,
                UserId = ctx.Sender,
                Emoji = emoji,
                CreatedAt = ctx.Timestamp
            });
            Log.Info($"Reaction {emoji} added to message {messageId} by {ctx.Sender}");
        }
    }
}

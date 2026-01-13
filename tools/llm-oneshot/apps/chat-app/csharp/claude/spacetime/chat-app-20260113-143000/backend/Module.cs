using SpacetimeDB;

public static partial class Module
{
    // Rate limiting: 500ms between messages per user
    private const long MESSAGE_COOLDOWN_MICROS = 500_000;
    private const long TYPING_EXPIRY_MICROS = 5_000_000; // 5 seconds
    private const int MAX_MESSAGE_LENGTH = 2000;
    private const int MAX_USERNAME_LENGTH = 32;
    private const int MAX_ROOM_NAME_LENGTH = 50;

    // ========================================================================
    // LIFECYCLE HOOKS (NO "On" prefix!)
    // ========================================================================

    [SpacetimeDB.Reducer(ReducerKind.ClientConnected)]
    public static void ClientConnected(ReducerContext ctx)
    {
        var existing = ctx.Db.user.Identity.Find(ctx.Sender);
        if (existing != null)
        {
            var user = existing.Value;
            ctx.Db.user.Identity.Update(new User
            {
                Identity = user.Identity,
                Username = user.Username,
                Status = user.Status == UserStatus.Invisible ? UserStatus.Invisible : UserStatus.Online,
                LastActive = ctx.Timestamp,
                IsAnonymous = user.IsAnonymous
            });
        }
        else
        {
            ctx.Db.user.Insert(new User
            {
                Identity = ctx.Sender,
                Username = "",
                Status = UserStatus.Online,
                LastActive = ctx.Timestamp,
                IsAnonymous = true
            });
        }
    }

    [SpacetimeDB.Reducer(ReducerKind.ClientDisconnected)]
    public static void ClientDisconnected(ReducerContext ctx)
    {
        var existing = ctx.Db.user.Identity.Find(ctx.Sender);
        if (existing != null)
        {
            var user = existing.Value;
            var newStatus = user.Status == UserStatus.Invisible ? UserStatus.Invisible : UserStatus.Online;
            ctx.Db.user.Identity.Update(new User
            {
                Identity = user.Identity,
                Username = user.Username,
                Status = newStatus,
                LastActive = ctx.Timestamp,
                IsAnonymous = user.IsAnonymous
            });
        }

        // Clear typing indicators for this user
        foreach (var typing in ctx.Db.typing_indicator.Iter())
        {
            if (typing.UserId == ctx.Sender)
            {
                ctx.Db.typing_indicator.ScheduledId.Delete(typing.ScheduledId);
            }
        }
    }

    // ========================================================================
    // USER MANAGEMENT
    // ========================================================================

    private static User? FindUserByUsername(ReducerContext ctx, string username)
    {
        foreach (var user in ctx.Db.user.idx_user_username.Filter(username))
        {
            if (user.Username == username)
                return user;
        }
        return null;
    }

    [SpacetimeDB.Reducer]
    public static void SetUsername(ReducerContext ctx, string username)
    {
        if (string.IsNullOrWhiteSpace(username))
            throw new ArgumentException("Username cannot be empty");
        if (username.Length > MAX_USERNAME_LENGTH)
            throw new ArgumentException($"Username cannot exceed {MAX_USERNAME_LENGTH} characters");

        var existingWithName = FindUserByUsername(ctx, username);
        if (existingWithName != null && existingWithName.Value.Identity != ctx.Sender)
            throw new ArgumentException("Username already taken");

        var user = ctx.Db.user.Identity.Find(ctx.Sender);
        if (user == null)
            throw new Exception("User not found");

        var u = user.Value;
        ctx.Db.user.Identity.Update(new User
        {
            Identity = u.Identity,
            Username = username,
            Status = u.Status,
            LastActive = ctx.Timestamp,
            IsAnonymous = false // Registering
        });
    }

    [SpacetimeDB.Reducer]
    public static void SetStatus(ReducerContext ctx, UserStatus status)
    {
        var user = ctx.Db.user.Identity.Find(ctx.Sender);
        if (user == null)
            throw new Exception("User not found");

        var u = user.Value;
        ctx.Db.user.Identity.Update(new User
        {
            Identity = u.Identity,
            Username = u.Username,
            Status = status,
            LastActive = ctx.Timestamp,
            IsAnonymous = u.IsAnonymous
        });
    }

    [SpacetimeDB.Reducer]
    public static void UpdateActivity(ReducerContext ctx)
    {
        var user = ctx.Db.user.Identity.Find(ctx.Sender);
        if (user == null) return;

        var u = user.Value;
        ctx.Db.user.Identity.Update(new User
        {
            Identity = u.Identity,
            Username = u.Username,
            Status = u.Status == UserStatus.Invisible ? UserStatus.Invisible : UserStatus.Online,
            LastActive = ctx.Timestamp,
            IsAnonymous = u.IsAnonymous
        });
    }

    // ========================================================================
    // ROOM MANAGEMENT
    // ========================================================================

    [SpacetimeDB.Reducer]
    public static void CreateRoom(ReducerContext ctx, string name, bool isPrivate)
    {
        if (string.IsNullOrWhiteSpace(name))
            throw new ArgumentException("Room name cannot be empty");
        if (name.Length > MAX_ROOM_NAME_LENGTH)
            throw new ArgumentException($"Room name cannot exceed {MAX_ROOM_NAME_LENGTH} characters");

        var room = ctx.Db.room.Insert(new Room
        {
            Id = 0,
            Name = name,
            OwnerId = ctx.Sender,
            IsPrivate = isPrivate,
            IsDm = false,
            CreatedAt = ctx.Timestamp
        });

        // Creator joins as admin
        ctx.Db.room_member.Insert(new RoomMember
        {
            Id = 0,
            RoomId = room.Id,
            UserId = ctx.Sender,
            Role = MemberRole.Admin,
            IsBanned = false,
            IsKicked = false,
            JoinedAt = ctx.Timestamp,
            LastReadMessageId = 0
        });

        // Initialize activity
        ctx.Db.room_activity.Insert(new RoomActivity
        {
            RoomId = room.Id,
            MessageCountLastHour = 0,
            LastMessageAt = ctx.Timestamp,
            LastUpdated = ctx.Timestamp
        });
    }

    [SpacetimeDB.Reducer]
    public static void JoinRoom(ReducerContext ctx, ulong roomId)
    {
        var room = ctx.Db.room.Id.Find(roomId);
        if (room == null)
            throw new Exception("Room not found");

        if (room.Value.IsPrivate)
            throw new Exception("Cannot join private room without invite");

        // Check if already member or banned
        foreach (var member in ctx.Db.room_member.idx_room_member_room.Filter(roomId))
        {
            if (member.UserId == ctx.Sender)
            {
                if (member.IsBanned)
                    throw new Exception("You are banned from this room");
                if (!member.IsKicked)
                    throw new Exception("Already a member of this room");
                
                // Rejoin if kicked
                ctx.Db.room_member.Id.Update(new RoomMember
                {
                    Id = member.Id,
                    RoomId = member.RoomId,
                    UserId = member.UserId,
                    Role = MemberRole.Member,
                    IsBanned = false,
                    IsKicked = false,
                    JoinedAt = ctx.Timestamp,
                    LastReadMessageId = member.LastReadMessageId
                });
                return;
            }
        }

        ctx.Db.room_member.Insert(new RoomMember
        {
            Id = 0,
            RoomId = roomId,
            UserId = ctx.Sender,
            Role = MemberRole.Member,
            IsBanned = false,
            IsKicked = false,
            JoinedAt = ctx.Timestamp,
            LastReadMessageId = 0
        });
    }

    [SpacetimeDB.Reducer]
    public static void LeaveRoom(ReducerContext ctx, ulong roomId)
    {
        foreach (var member in ctx.Db.room_member.idx_room_member_room.Filter(roomId))
        {
            if (member.UserId == ctx.Sender)
            {
                ctx.Db.room_member.Id.Delete(member.Id);
                return;
            }
        }
        throw new Exception("Not a member of this room");
    }

    // ========================================================================
    // PRIVATE ROOMS & INVITES
    // ========================================================================

    [SpacetimeDB.Reducer]
    public static void InviteToRoom(ReducerContext ctx, ulong roomId, string inviteeUsername)
    {
        var room = ctx.Db.room.Id.Find(roomId);
        if (room == null)
            throw new Exception("Room not found");

        // Check sender is admin
        bool isAdmin = false;
        foreach (var member in ctx.Db.room_member.idx_room_member_room.Filter(roomId))
        {
            if (member.UserId == ctx.Sender && member.Role == MemberRole.Admin && !member.IsKicked && !member.IsBanned)
            {
                isAdmin = true;
                break;
            }
        }
        if (!isAdmin)
            throw new Exception("Only admins can invite users");

        var invitee = FindUserByUsername(ctx, inviteeUsername);
        if (invitee == null)
            throw new Exception("User not found");

        ctx.Db.room_invite.Insert(new RoomInvite
        {
            Id = 0,
            RoomId = roomId,
            InviterId = ctx.Sender,
            InviteeId = invitee.Value.Identity,
            CreatedAt = ctx.Timestamp,
            Status = "pending"
        });
    }

    [SpacetimeDB.Reducer]
    public static void AcceptInvite(ReducerContext ctx, ulong inviteId)
    {
        var invite = ctx.Db.room_invite.Id.Find(inviteId);
        if (invite == null)
            throw new Exception("Invite not found");

        var inv = invite.Value;
        if (inv.InviteeId != ctx.Sender)
            throw new Exception("This invite is not for you");
        if (inv.Status != "pending")
            throw new Exception("Invite already processed");

        ctx.Db.room_invite.Id.Update(new RoomInvite
        {
            Id = inv.Id,
            RoomId = inv.RoomId,
            InviterId = inv.InviterId,
            InviteeId = inv.InviteeId,
            CreatedAt = inv.CreatedAt,
            Status = "accepted"
        });

        // Add to room
        ctx.Db.room_member.Insert(new RoomMember
        {
            Id = 0,
            RoomId = inv.RoomId,
            UserId = ctx.Sender,
            Role = MemberRole.Member,
            IsBanned = false,
            IsKicked = false,
            JoinedAt = ctx.Timestamp,
            LastReadMessageId = 0
        });
    }

    [SpacetimeDB.Reducer]
    public static void DeclineInvite(ReducerContext ctx, ulong inviteId)
    {
        var invite = ctx.Db.room_invite.Id.Find(inviteId);
        if (invite == null)
            throw new Exception("Invite not found");

        var inv = invite.Value;
        if (inv.InviteeId != ctx.Sender)
            throw new Exception("This invite is not for you");

        ctx.Db.room_invite.Id.Update(new RoomInvite
        {
            Id = inv.Id,
            RoomId = inv.RoomId,
            InviterId = inv.InviterId,
            InviteeId = inv.InviteeId,
            CreatedAt = inv.CreatedAt,
            Status = "declined"
        });
    }

    [SpacetimeDB.Reducer]
    public static void StartDm(ReducerContext ctx, string targetUsername)
    {
        var target = FindUserByUsername(ctx, targetUsername);
        if (target == null)
            throw new Exception("User not found");

        var targetId = target.Value.Identity;
        if (targetId == ctx.Sender)
            throw new Exception("Cannot DM yourself");

        // Check if DM already exists
        foreach (var room in ctx.Db.room.Iter())
        {
            if (!room.IsDm) continue;
            
            bool hasSender = false, hasTarget = false;
            foreach (var member in ctx.Db.room_member.idx_room_member_room.Filter(room.Id))
            {
                if (member.UserId == ctx.Sender) hasSender = true;
                if (member.UserId == targetId) hasTarget = true;
            }
            if (hasSender && hasTarget)
                throw new Exception("DM already exists");
        }

        // Create DM room
        var dmRoom = ctx.Db.room.Insert(new Room
        {
            Id = 0,
            Name = "DM",
            OwnerId = ctx.Sender,
            IsPrivate = true,
            IsDm = true,
            CreatedAt = ctx.Timestamp
        });

        // Add both users
        ctx.Db.room_member.Insert(new RoomMember
        {
            Id = 0,
            RoomId = dmRoom.Id,
            UserId = ctx.Sender,
            Role = MemberRole.Admin,
            IsBanned = false,
            IsKicked = false,
            JoinedAt = ctx.Timestamp,
            LastReadMessageId = 0
        });

        ctx.Db.room_member.Insert(new RoomMember
        {
            Id = 0,
            RoomId = dmRoom.Id,
            UserId = targetId,
            Role = MemberRole.Admin,
            IsBanned = false,
            IsKicked = false,
            JoinedAt = ctx.Timestamp,
            LastReadMessageId = 0
        });

        ctx.Db.room_activity.Insert(new RoomActivity
        {
            RoomId = dmRoom.Id,
            MessageCountLastHour = 0,
            LastMessageAt = ctx.Timestamp,
            LastUpdated = ctx.Timestamp
        });
    }

    // ========================================================================
    // PERMISSIONS (Kick/Ban/Promote)
    // ========================================================================

    [SpacetimeDB.Reducer]
    public static void KickUser(ReducerContext ctx, ulong roomId, string targetUsername)
    {
        var room = ctx.Db.room.Id.Find(roomId);
        if (room == null)
            throw new Exception("Room not found");

        var target = FindUserByUsername(ctx, targetUsername);
        if (target == null)
            throw new Exception("User not found");

        var targetId = target.Value.Identity;

        // Check sender is admin
        RoomMember? senderMember = null;
        RoomMember? targetMember = null;
        foreach (var member in ctx.Db.room_member.idx_room_member_room.Filter(roomId))
        {
            if (member.UserId == ctx.Sender) senderMember = member;
            if (member.UserId == targetId) targetMember = member;
        }

        if (senderMember == null || senderMember.Value.Role != MemberRole.Admin)
            throw new Exception("Only admins can kick users");
        if (targetMember == null)
            throw new Exception("User is not in this room");
        if (targetId == room.Value.OwnerId)
            throw new Exception("Cannot kick room owner");

        var tm = targetMember.Value;
        ctx.Db.room_member.Id.Update(new RoomMember
        {
            Id = tm.Id,
            RoomId = tm.RoomId,
            UserId = tm.UserId,
            Role = tm.Role,
            IsBanned = tm.IsBanned,
            IsKicked = true,
            JoinedAt = tm.JoinedAt,
            LastReadMessageId = tm.LastReadMessageId
        });
    }

    [SpacetimeDB.Reducer]
    public static void BanUser(ReducerContext ctx, ulong roomId, string targetUsername)
    {
        var room = ctx.Db.room.Id.Find(roomId);
        if (room == null)
            throw new Exception("Room not found");

        var target = FindUserByUsername(ctx, targetUsername);
        if (target == null)
            throw new Exception("User not found");

        var targetId = target.Value.Identity;

        RoomMember? senderMember = null;
        RoomMember? targetMember = null;
        foreach (var member in ctx.Db.room_member.idx_room_member_room.Filter(roomId))
        {
            if (member.UserId == ctx.Sender) senderMember = member;
            if (member.UserId == targetId) targetMember = member;
        }

        if (senderMember == null || senderMember.Value.Role != MemberRole.Admin)
            throw new Exception("Only admins can ban users");
        if (targetId == room.Value.OwnerId)
            throw new Exception("Cannot ban room owner");

        if (targetMember != null)
        {
            var tm = targetMember.Value;
            ctx.Db.room_member.Id.Update(new RoomMember
            {
                Id = tm.Id,
                RoomId = tm.RoomId,
                UserId = tm.UserId,
                Role = tm.Role,
                IsBanned = true,
                IsKicked = true,
                JoinedAt = tm.JoinedAt,
                LastReadMessageId = tm.LastReadMessageId
            });
        }
        else
        {
            // Create banned member entry
            ctx.Db.room_member.Insert(new RoomMember
            {
                Id = 0,
                RoomId = roomId,
                UserId = targetId,
                Role = MemberRole.Member,
                IsBanned = true,
                IsKicked = true,
                JoinedAt = ctx.Timestamp,
                LastReadMessageId = 0
            });
        }
    }

    [SpacetimeDB.Reducer]
    public static void PromoteToAdmin(ReducerContext ctx, ulong roomId, string targetUsername)
    {
        var room = ctx.Db.room.Id.Find(roomId);
        if (room == null)
            throw new Exception("Room not found");

        var target = FindUserByUsername(ctx, targetUsername);
        if (target == null)
            throw new Exception("User not found");

        var targetId = target.Value.Identity;

        RoomMember? senderMember = null;
        RoomMember? targetMember = null;
        foreach (var member in ctx.Db.room_member.idx_room_member_room.Filter(roomId))
        {
            if (member.UserId == ctx.Sender) senderMember = member;
            if (member.UserId == targetId) targetMember = member;
        }

        if (senderMember == null || senderMember.Value.Role != MemberRole.Admin)
            throw new Exception("Only admins can promote users");
        if (targetMember == null)
            throw new Exception("User is not in this room");

        var tm = targetMember.Value;
        ctx.Db.room_member.Id.Update(new RoomMember
        {
            Id = tm.Id,
            RoomId = tm.RoomId,
            UserId = tm.UserId,
            Role = MemberRole.Admin,
            IsBanned = tm.IsBanned,
            IsKicked = tm.IsKicked,
            JoinedAt = tm.JoinedAt,
            LastReadMessageId = tm.LastReadMessageId
        });
    }

    // ========================================================================
    // MESSAGES
    // ========================================================================

    private static RoomMember? GetMembership(ReducerContext ctx, ulong roomId)
    {
        foreach (var member in ctx.Db.room_member.idx_room_member_room.Filter(roomId))
        {
            if (member.UserId == ctx.Sender && !member.IsKicked && !member.IsBanned)
                return member;
        }
        return null;
    }

    [SpacetimeDB.Reducer]
    public static void SendMessage(ReducerContext ctx, ulong roomId, string text, ulong parentMessageId)
    {
        if (string.IsNullOrWhiteSpace(text))
            throw new ArgumentException("Message cannot be empty");
        if (text.Length > MAX_MESSAGE_LENGTH)
            throw new ArgumentException($"Message cannot exceed {MAX_MESSAGE_LENGTH} characters");

        var membership = GetMembership(ctx, roomId);
        if (membership == null)
            throw new Exception("Not a member of this room");

        // Rate limiting: check last message time
        foreach (var msg in ctx.Db.message.idx_message_room.Filter(roomId))
        {
            if (msg.SenderId == ctx.Sender)
            {
                var elapsed = ctx.Timestamp.MicrosecondsSinceUnixEpoch - msg.CreatedAt.MicrosecondsSinceUnixEpoch;
                if (elapsed < MESSAGE_COOLDOWN_MICROS)
                    throw new Exception("Please wait before sending another message");
            }
        }

        ctx.Db.message.Insert(new Message
        {
            Id = 0,
            RoomId = roomId,
            SenderId = ctx.Sender,
            Text = text,
            CreatedAt = ctx.Timestamp,
            IsEdited = false,
            ParentMessageId = parentMessageId,
            IsEphemeral = false,
            ExpiresAtMicros = 0
        });

        // Update room activity
        UpdateRoomActivity(ctx, roomId);

        // Clear typing indicator
        ClearTypingIndicator(ctx, roomId);
    }

    [SpacetimeDB.Reducer]
    public static void SendEphemeralMessage(ReducerContext ctx, ulong roomId, string text, ulong expiryMinutes)
    {
        if (string.IsNullOrWhiteSpace(text))
            throw new ArgumentException("Message cannot be empty");
        if (expiryMinutes < 1 || expiryMinutes > 60)
            throw new ArgumentException("Expiry must be between 1 and 60 minutes");

        var membership = GetMembership(ctx, roomId);
        if (membership == null)
            throw new Exception("Not a member of this room");

        var expiresAtMicros = ctx.Timestamp.MicrosecondsSinceUnixEpoch + (long)(expiryMinutes * 60 * 1_000_000);
        var expiresAt = new Timestamp(expiresAtMicros);

        var message = ctx.Db.message.Insert(new Message
        {
            Id = 0,
            RoomId = roomId,
            SenderId = ctx.Sender,
            Text = text,
            CreatedAt = ctx.Timestamp,
            IsEdited = false,
            ParentMessageId = 0,
            IsEphemeral = true,
            ExpiresAtMicros = expiresAtMicros
        });

        // Schedule deletion
        ctx.Db.message_expiry.Insert(new MessageExpiry
        {
            ScheduledId = 0,
            ScheduledAt = new ScheduleAt.Time(expiresAt),
            MessageId = message.Id
        });

        UpdateRoomActivity(ctx, roomId);
        ClearTypingIndicator(ctx, roomId);
    }

    [SpacetimeDB.Reducer]
    public static void EditMessage(ReducerContext ctx, ulong messageId, string newText)
    {
        if (string.IsNullOrWhiteSpace(newText))
            throw new ArgumentException("Message cannot be empty");
        if (newText.Length > MAX_MESSAGE_LENGTH)
            throw new ArgumentException($"Message cannot exceed {MAX_MESSAGE_LENGTH} characters");

        var message = ctx.Db.message.Id.Find(messageId);
        if (message == null)
            throw new Exception("Message not found");

        var msg = message.Value;
        if (msg.SenderId != ctx.Sender)
            throw new Exception("Can only edit your own messages");

        // Save edit history
        ctx.Db.message_edit.Insert(new MessageEdit
        {
            Id = 0,
            MessageId = messageId,
            OldText = msg.Text,
            EditedAt = ctx.Timestamp
        });

        ctx.Db.message.Id.Update(new Message
        {
            Id = msg.Id,
            RoomId = msg.RoomId,
            SenderId = msg.SenderId,
            Text = newText,
            CreatedAt = msg.CreatedAt,
            IsEdited = true,
            ParentMessageId = msg.ParentMessageId,
            IsEphemeral = msg.IsEphemeral,
            ExpiresAtMicros = msg.ExpiresAtMicros
        });
    }

    [SpacetimeDB.Reducer]
    public static void DeleteMessage(ReducerContext ctx, ulong messageId)
    {
        var message = ctx.Db.message.Id.Find(messageId);
        if (message == null)
            throw new Exception("Message not found");

        var msg = message.Value;
        
        // Check ownership or admin
        if (msg.SenderId != ctx.Sender)
        {
            var membership = GetMembership(ctx, msg.RoomId);
            if (membership == null || membership.Value.Role != MemberRole.Admin)
                throw new Exception("Can only delete your own messages or be admin");
        }

        // Delete associated data
        foreach (var read in ctx.Db.message_read.idx_message_read_message.Filter(messageId))
            ctx.Db.message_read.Id.Delete(read.Id);
        foreach (var reaction in ctx.Db.reaction.idx_reaction_message.Filter(messageId))
            ctx.Db.reaction.Id.Delete(reaction.Id);
        foreach (var edit in ctx.Db.message_edit.idx_message_edit_message.Filter(messageId))
            ctx.Db.message_edit.Id.Delete(edit.Id);

        ctx.Db.message.Id.Delete(messageId);
    }

    // ========================================================================
    // TYPING INDICATORS
    // ========================================================================

    [SpacetimeDB.Reducer]
    public static void SetTyping(ReducerContext ctx, ulong roomId)
    {
        var membership = GetMembership(ctx, roomId);
        if (membership == null)
            throw new Exception("Not a member of this room");

        // Clear existing typing indicator for this user in this room
        ClearTypingIndicator(ctx, roomId);

        // Create new one with expiry
        var expiresAt = new Timestamp(ctx.Timestamp.MicrosecondsSinceUnixEpoch + TYPING_EXPIRY_MICROS);
        ctx.Db.typing_indicator.Insert(new TypingIndicator
        {
            ScheduledId = 0,
            ScheduledAt = new ScheduleAt.Time(expiresAt),
            RoomId = roomId,
            UserId = ctx.Sender
        });
    }

    private static void ClearTypingIndicator(ReducerContext ctx, ulong roomId)
    {
        foreach (var typing in ctx.Db.typing_indicator.Iter())
        {
            if (typing.RoomId == roomId && typing.UserId == ctx.Sender)
            {
                ctx.Db.typing_indicator.ScheduledId.Delete(typing.ScheduledId);
            }
        }
    }

    [SpacetimeDB.Reducer]
    public static void ExpireTyping(ReducerContext ctx, TypingIndicator typing)
    {
        // Row auto-deleted
        Log.Info($"Typing indicator expired for user in room {typing.RoomId}");
    }

    // ========================================================================
    // READ RECEIPTS
    // ========================================================================

    [SpacetimeDB.Reducer]
    public static void MarkMessageRead(ReducerContext ctx, ulong messageId)
    {
        var message = ctx.Db.message.Id.Find(messageId);
        if (message == null)
            throw new Exception("Message not found");

        var membership = GetMembership(ctx, message.Value.RoomId);
        if (membership == null)
            throw new Exception("Not a member of this room");

        // Check if already read
        foreach (var read in ctx.Db.message_read.idx_message_read_message.Filter(messageId))
        {
            if (read.UserId == ctx.Sender)
                return; // Already marked read
        }

        ctx.Db.message_read.Insert(new MessageRead
        {
            Id = 0,
            MessageId = messageId,
            UserId = ctx.Sender,
            ReadAt = ctx.Timestamp
        });

        // Update last read message ID for member
        var m = membership.Value;
        if (messageId > m.LastReadMessageId)
        {
            ctx.Db.room_member.Id.Update(new RoomMember
            {
                Id = m.Id,
                RoomId = m.RoomId,
                UserId = m.UserId,
                Role = m.Role,
                IsBanned = m.IsBanned,
                IsKicked = m.IsKicked,
                JoinedAt = m.JoinedAt,
                LastReadMessageId = messageId
            });
        }
    }

    [SpacetimeDB.Reducer]
    public static void MarkRoomRead(ReducerContext ctx, ulong roomId)
    {
        var membership = GetMembership(ctx, roomId);
        if (membership == null)
            throw new Exception("Not a member of this room");

        // Find latest message ID in room
        ulong latestId = 0;
        foreach (var msg in ctx.Db.message.idx_message_room.Filter(roomId))
        {
            if (msg.Id > latestId)
                latestId = msg.Id;
        }

        if (latestId > 0)
        {
            var m = membership.Value;
            ctx.Db.room_member.Id.Update(new RoomMember
            {
                Id = m.Id,
                RoomId = m.RoomId,
                UserId = m.UserId,
                Role = m.Role,
                IsBanned = m.IsBanned,
                IsKicked = m.IsKicked,
                JoinedAt = m.JoinedAt,
                LastReadMessageId = latestId
            });
        }
    }

    // ========================================================================
    // REACTIONS
    // ========================================================================

    [SpacetimeDB.Reducer]
    public static void AddReaction(ReducerContext ctx, ulong messageId, string emoji)
    {
        var message = ctx.Db.message.Id.Find(messageId);
        if (message == null)
            throw new Exception("Message not found");

        var membership = GetMembership(ctx, message.Value.RoomId);
        if (membership == null)
            throw new Exception("Not a member of this room");

        // Check if already reacted with same emoji
        foreach (var reaction in ctx.Db.reaction.idx_reaction_message.Filter(messageId))
        {
            if (reaction.UserId == ctx.Sender && reaction.Emoji == emoji)
                return; // Already reacted
        }

        ctx.Db.reaction.Insert(new Reaction
        {
            Id = 0,
            MessageId = messageId,
            UserId = ctx.Sender,
            Emoji = emoji
        });
    }

    [SpacetimeDB.Reducer]
    public static void RemoveReaction(ReducerContext ctx, ulong messageId, string emoji)
    {
        foreach (var reaction in ctx.Db.reaction.idx_reaction_message.Filter(messageId))
        {
            if (reaction.UserId == ctx.Sender && reaction.Emoji == emoji)
            {
                ctx.Db.reaction.Id.Delete(reaction.Id);
                return;
            }
        }
    }

    // ========================================================================
    // SCHEDULED MESSAGES
    // ========================================================================

    [SpacetimeDB.Reducer]
    public static void ScheduleMessage(ReducerContext ctx, ulong roomId, string text, long sendAtMicros, ulong parentMessageId)
    {
        if (string.IsNullOrWhiteSpace(text))
            throw new ArgumentException("Message cannot be empty");
        if (sendAtMicros <= ctx.Timestamp.MicrosecondsSinceUnixEpoch)
            throw new ArgumentException("Scheduled time must be in the future");

        var membership = GetMembership(ctx, roomId);
        if (membership == null)
            throw new Exception("Not a member of this room");

        ctx.Db.scheduled_message.Insert(new ScheduledMessage
        {
            ScheduledId = 0,
            ScheduledAt = new ScheduleAt.Time(new Timestamp(sendAtMicros)),
            RoomId = roomId,
            SenderId = ctx.Sender,
            Text = text,
            ParentMessageId = parentMessageId
        });
    }

    [SpacetimeDB.Reducer]
    public static void CancelScheduledMessage(ReducerContext ctx, ulong scheduledId)
    {
        var scheduled = ctx.Db.scheduled_message.ScheduledId.Find(scheduledId);
        if (scheduled == null)
            throw new Exception("Scheduled message not found");

        if (scheduled.Value.SenderId != ctx.Sender)
            throw new Exception("Can only cancel your own scheduled messages");

        ctx.Db.scheduled_message.ScheduledId.Delete(scheduledId);
    }

    [SpacetimeDB.Reducer]
    public static void SendScheduledMessage(ReducerContext ctx, ScheduledMessage scheduled)
    {
        // Check if user is still a member
        bool isMember = false;
        foreach (var member in ctx.Db.room_member.idx_room_member_room.Filter(scheduled.RoomId))
        {
            if (member.UserId == scheduled.SenderId && !member.IsKicked && !member.IsBanned)
            {
                isMember = true;
                break;
            }
        }

        if (isMember)
        {
            ctx.Db.message.Insert(new Message
            {
                Id = 0,
                RoomId = scheduled.RoomId,
                SenderId = scheduled.SenderId,
                Text = scheduled.Text,
                CreatedAt = ctx.Timestamp,
                IsEdited = false,
                ParentMessageId = scheduled.ParentMessageId,
                IsEphemeral = false,
                ExpiresAtMicros = 0
            });

            UpdateRoomActivity(ctx, scheduled.RoomId);
        }
        // Row auto-deleted
    }

    // ========================================================================
    // EPHEMERAL MESSAGE EXPIRY
    // ========================================================================

    [SpacetimeDB.Reducer]
    public static void ExpireMessage(ReducerContext ctx, MessageExpiry expiry)
    {
        var message = ctx.Db.message.Id.Find(expiry.MessageId);
        if (message != null)
        {
            // Delete associated data
            foreach (var read in ctx.Db.message_read.idx_message_read_message.Filter(expiry.MessageId))
                ctx.Db.message_read.Id.Delete(read.Id);
            foreach (var reaction in ctx.Db.reaction.idx_reaction_message.Filter(expiry.MessageId))
                ctx.Db.reaction.Id.Delete(reaction.Id);
            foreach (var edit in ctx.Db.message_edit.idx_message_edit_message.Filter(expiry.MessageId))
                ctx.Db.message_edit.Id.Delete(edit.Id);

            ctx.Db.message.Id.Delete(expiry.MessageId);
        }
        // Row auto-deleted
    }

    // ========================================================================
    // DRAFTS
    // ========================================================================

    [SpacetimeDB.Reducer]
    public static void SaveDraft(ReducerContext ctx, ulong roomId, string text)
    {
        var membership = GetMembership(ctx, roomId);
        if (membership == null)
            throw new Exception("Not a member of this room");

        // Find existing draft
        foreach (var draft in ctx.Db.draft.idx_draft_user.Filter(ctx.Sender))
        {
            if (draft.RoomId == roomId)
            {
                if (string.IsNullOrEmpty(text))
                {
                    ctx.Db.draft.Id.Delete(draft.Id);
                }
                else
                {
                    ctx.Db.draft.Id.Update(new Draft
                    {
                        Id = draft.Id,
                        RoomId = roomId,
                        UserId = ctx.Sender,
                        Text = text,
                        UpdatedAt = ctx.Timestamp
                    });
                }
                return;
            }
        }

        if (!string.IsNullOrEmpty(text))
        {
            ctx.Db.draft.Insert(new Draft
            {
                Id = 0,
                RoomId = roomId,
                UserId = ctx.Sender,
                Text = text,
                UpdatedAt = ctx.Timestamp
            });
        }
    }

    [SpacetimeDB.Reducer]
    public static void ClearDraft(ReducerContext ctx, ulong roomId)
    {
        foreach (var draft in ctx.Db.draft.idx_draft_user.Filter(ctx.Sender))
        {
            if (draft.RoomId == roomId)
            {
                ctx.Db.draft.Id.Delete(draft.Id);
                return;
            }
        }
    }

    // ========================================================================
    // ROOM ACTIVITY
    // ========================================================================

    private static void UpdateRoomActivity(ReducerContext ctx, ulong roomId)
    {
        var activity = ctx.Db.room_activity.RoomId.Find(roomId);
        if (activity != null)
        {
            var a = activity.Value;
            ctx.Db.room_activity.RoomId.Update(new RoomActivity
            {
                RoomId = roomId,
                MessageCountLastHour = a.MessageCountLastHour + 1,
                LastMessageAt = ctx.Timestamp,
                LastUpdated = ctx.Timestamp
            });
        }
    }
}

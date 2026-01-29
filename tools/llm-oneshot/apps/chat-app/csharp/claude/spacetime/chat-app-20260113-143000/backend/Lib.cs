using SpacetimeDB;

// ============================================================================
// USER & PRESENCE
// ============================================================================

[SpacetimeDB.Type]
public enum UserStatus
{
    Online,
    Away,
    DoNotDisturb,
    Invisible
}

[SpacetimeDB.Table(Name = "user", Public = true)]
[SpacetimeDB.Index.BTree(Name = "idx_user_username", Columns = new[] { "Username" })]
public partial struct User
{
    [SpacetimeDB.PrimaryKey]
    public Identity Identity;

    public string Username; // Not nullable, empty string = anonymous

    public UserStatus Status;
    public Timestamp LastActive;
    public bool IsAnonymous;
}

// ============================================================================
// ROOMS
// ============================================================================

[SpacetimeDB.Table(Name = "room", Public = true)]
public partial struct Room
{
    [SpacetimeDB.PrimaryKey]
    [SpacetimeDB.AutoInc]
    public ulong Id;

    public string Name;
    public Identity OwnerId;
    public bool IsPrivate;
    public bool IsDm;
    public Timestamp CreatedAt;
}

[SpacetimeDB.Type]
public enum MemberRole
{
    Member,
    Admin
}

[SpacetimeDB.Table(Name = "room_member", Public = true)]
[SpacetimeDB.Index.BTree(Name = "idx_room_member_room", Columns = new[] { "RoomId" })]
[SpacetimeDB.Index.BTree(Name = "idx_room_member_user", Columns = new[] { "UserId" })]
public partial struct RoomMember
{
    [SpacetimeDB.PrimaryKey]
    [SpacetimeDB.AutoInc]
    public ulong Id;

    public ulong RoomId;
    public Identity UserId;
    public MemberRole Role;
    public bool IsBanned;
    public bool IsKicked;
    public Timestamp JoinedAt;
    public ulong LastReadMessageId;
}

[SpacetimeDB.Table(Name = "room_invite", Public = true)]
[SpacetimeDB.Index.BTree(Name = "idx_room_invite_invitee", Columns = new[] { "InviteeId" })]
public partial struct RoomInvite
{
    [SpacetimeDB.PrimaryKey]
    [SpacetimeDB.AutoInc]
    public ulong Id;

    public ulong RoomId;
    public Identity InviterId;
    public Identity InviteeId;
    public Timestamp CreatedAt;
    public string Status; // pending, accepted, declined
}

// ============================================================================
// MESSAGES
// ============================================================================

[SpacetimeDB.Table(Name = "message", Public = true)]
[SpacetimeDB.Index.BTree(Name = "idx_message_room", Columns = new[] { "RoomId" })]
[SpacetimeDB.Index.BTree(Name = "idx_message_parent", Columns = new[] { "ParentMessageId" })]
public partial struct Message
{
    [SpacetimeDB.PrimaryKey]
    [SpacetimeDB.AutoInc]
    public ulong Id;

    public ulong RoomId;
    public Identity SenderId;
    public string Text;
    public Timestamp CreatedAt;
    public bool IsEdited;
    public ulong ParentMessageId; // 0 = no parent (for threading)
    public bool IsEphemeral;
    public long ExpiresAtMicros; // 0 = no expiry
}

[SpacetimeDB.Table(Name = "message_read", Public = true)]
[SpacetimeDB.Index.BTree(Name = "idx_message_read_message", Columns = new[] { "MessageId" })]
public partial struct MessageRead
{
    [SpacetimeDB.PrimaryKey]
    [SpacetimeDB.AutoInc]
    public ulong Id;

    public ulong MessageId;
    public Identity UserId;
    public Timestamp ReadAt;
}

[SpacetimeDB.Table(Name = "reaction", Public = true)]
[SpacetimeDB.Index.BTree(Name = "idx_reaction_message", Columns = new[] { "MessageId" })]
public partial struct Reaction
{
    [SpacetimeDB.PrimaryKey]
    [SpacetimeDB.AutoInc]
    public ulong Id;

    public ulong MessageId;
    public Identity UserId;
    public string Emoji;
}

[SpacetimeDB.Table(Name = "message_edit", Public = true)]
[SpacetimeDB.Index.BTree(Name = "idx_message_edit_message", Columns = new[] { "MessageId" })]
public partial struct MessageEdit
{
    [SpacetimeDB.PrimaryKey]
    [SpacetimeDB.AutoInc]
    public ulong Id;

    public ulong MessageId;
    public string OldText;
    public Timestamp EditedAt;
}

// ============================================================================
// TYPING INDICATORS (SCHEDULED TABLE - Auto-expires)
// ============================================================================

[SpacetimeDB.Table(Name = "typing_indicator", Public = true, Scheduled = "ExpireTyping")]
public partial struct TypingIndicator
{
    [SpacetimeDB.PrimaryKey]
    [SpacetimeDB.AutoInc]
    public ulong ScheduledId;

    public ScheduleAt ScheduledAt;
    public ulong RoomId;
    public Identity UserId;
}

// ============================================================================
// SCHEDULED MESSAGES
// ============================================================================

[SpacetimeDB.Table(Name = "scheduled_message", Public = true, Scheduled = "SendScheduledMessage")]
public partial struct ScheduledMessage
{
    [SpacetimeDB.PrimaryKey]
    [SpacetimeDB.AutoInc]
    public ulong ScheduledId;

    public ScheduleAt ScheduledAt;
    public ulong RoomId;
    public Identity SenderId;
    public string Text;
    public ulong ParentMessageId;
}

// ============================================================================
// EPHEMERAL MESSAGE EXPIRY
// ============================================================================

[SpacetimeDB.Table(Name = "message_expiry", Scheduled = "ExpireMessage")]
public partial struct MessageExpiry
{
    [SpacetimeDB.PrimaryKey]
    [SpacetimeDB.AutoInc]
    public ulong ScheduledId;

    public ScheduleAt ScheduledAt;
    public ulong MessageId;
}

// ============================================================================
// DRAFT SYNC
// ============================================================================

[SpacetimeDB.Table(Name = "draft", Public = true)]
[SpacetimeDB.Index.BTree(Name = "idx_draft_user", Columns = new[] { "UserId" })]
public partial struct Draft
{
    [SpacetimeDB.PrimaryKey]
    [SpacetimeDB.AutoInc]
    public ulong Id;

    public ulong RoomId;
    public Identity UserId;
    public string Text;
    public Timestamp UpdatedAt;
}

// ============================================================================
// ROOM ACTIVITY (For activity indicators)
// ============================================================================

[SpacetimeDB.Table(Name = "room_activity", Public = true)]
public partial struct RoomActivity
{
    [SpacetimeDB.PrimaryKey]
    public ulong RoomId;

    public ulong MessageCountLastHour;
    public Timestamp LastMessageAt;
    public Timestamp LastUpdated;
}

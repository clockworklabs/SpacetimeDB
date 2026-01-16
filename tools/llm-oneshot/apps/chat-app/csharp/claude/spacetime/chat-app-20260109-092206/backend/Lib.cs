using SpacetimeDB;

// ============================================================================
// USER TABLE - Stores user profiles and online status
// ============================================================================
[SpacetimeDB.Table(Name = "user", Public = true)]
public partial class User
{
    [SpacetimeDB.PrimaryKey]
    public Identity Identity;

    public string DisplayName = "";
    public bool IsOnline;
    public Timestamp LastSeen;
}

// ============================================================================
// ROOM TABLE - Chat rooms
// ============================================================================
[SpacetimeDB.Table(Name = "room", Public = true)]
public partial class Room
{
    [SpacetimeDB.PrimaryKey]
    [SpacetimeDB.AutoInc]
    public ulong Id;

    public string Name = "";
    public Identity CreatedBy;
    public Timestamp CreatedAt;
}

// ============================================================================
// ROOM_MEMBER TABLE - Tracks which users are in which rooms
// ============================================================================
[SpacetimeDB.Table(Name = "room_member", Public = true)]
[SpacetimeDB.Index.BTree(Name = "room_member_room_id", Columns = new[] { "RoomId" })]
[SpacetimeDB.Index.BTree(Name = "room_member_user_identity", Columns = new[] { "UserIdentity" })]
public partial class RoomMember
{
    [SpacetimeDB.PrimaryKey]
    [SpacetimeDB.AutoInc]
    public ulong Id;

    public ulong RoomId;
    public Identity UserIdentity;
    public Timestamp JoinedAt;
}

// ============================================================================
// MESSAGE TABLE - Chat messages
// ============================================================================
[SpacetimeDB.Table(Name = "message", Public = true)]
[SpacetimeDB.Index.BTree(Name = "message_room_id", Columns = new[] { "RoomId" })]
public partial class Message
{
    [SpacetimeDB.PrimaryKey]
    [SpacetimeDB.AutoInc]
    public ulong Id;

    public ulong RoomId;
    public Identity SenderIdentity;
    public string Content = "";
    public Timestamp CreatedAt;
    public bool IsEdited;
    public bool IsEphemeral;
    public ulong ExpiresAtMicros;  // 0 means no expiration
}

// ============================================================================
// MESSAGE_EDIT TABLE - Edit history for messages
// ============================================================================
[SpacetimeDB.Table(Name = "message_edit", Public = true)]
[SpacetimeDB.Index.BTree(Name = "message_edit_message_id", Columns = new[] { "MessageId" })]
public partial class MessageEdit
{
    [SpacetimeDB.PrimaryKey]
    [SpacetimeDB.AutoInc]
    public ulong Id;

    public ulong MessageId;
    public string PreviousContent = "";
    public Timestamp EditedAt;
}

// ============================================================================
// TYPING_INDICATOR TABLE - Tracks who is currently typing
// ============================================================================
[SpacetimeDB.Table(Name = "typing_indicator", Public = true)]
[SpacetimeDB.Index.BTree(Name = "typing_indicator_room_id", Columns = new[] { "RoomId" })]
public partial class TypingIndicator
{
    [SpacetimeDB.PrimaryKey]
    [SpacetimeDB.AutoInc]
    public ulong Id;

    public ulong RoomId;
    public Identity UserIdentity;
    public Timestamp StartedAt;
}

// ============================================================================
// READ_RECEIPT TABLE - Tracks which messages users have read
// ============================================================================
[SpacetimeDB.Table(Name = "read_receipt", Public = true)]
[SpacetimeDB.Index.BTree(Name = "read_receipt_message_id", Columns = new[] { "MessageId" })]
[SpacetimeDB.Index.BTree(Name = "read_receipt_user_identity", Columns = new[] { "UserIdentity" })]
public partial class ReadReceipt
{
    [SpacetimeDB.PrimaryKey]
    [SpacetimeDB.AutoInc]
    public ulong Id;

    public ulong MessageId;
    public Identity UserIdentity;
    public Timestamp ReadAt;
}

// ============================================================================
// LAST_READ TABLE - Tracks last read message per user per room (for unread counts)
// ============================================================================
[SpacetimeDB.Table(Name = "last_read", Public = true)]
[SpacetimeDB.Index.BTree(Name = "last_read_user_identity", Columns = new[] { "UserIdentity" })]
public partial class LastRead
{
    [SpacetimeDB.PrimaryKey]
    [SpacetimeDB.AutoInc]
    public ulong Id;

    public ulong RoomId;
    public Identity UserIdentity;
    public ulong LastMessageId;
    public Timestamp LastReadAt;
}

// ============================================================================
// REACTION TABLE - Message reactions
// ============================================================================
[SpacetimeDB.Table(Name = "reaction", Public = true)]
[SpacetimeDB.Index.BTree(Name = "reaction_message_id", Columns = new[] { "MessageId" })]
public partial class Reaction
{
    [SpacetimeDB.PrimaryKey]
    [SpacetimeDB.AutoInc]
    public ulong Id;

    public ulong MessageId;
    public Identity UserIdentity;
    public string Emoji = "";
    public Timestamp CreatedAt;
}

// ============================================================================
// SCHEDULED_MESSAGE TABLE - Messages scheduled for future delivery
// ============================================================================
[SpacetimeDB.Table(Name = "scheduled_message", Public = true, Scheduled = nameof(Module.SendScheduledMessage))]
[SpacetimeDB.Index.BTree(Name = "scheduled_message_room_id", Columns = new[] { "RoomId" })]
[SpacetimeDB.Index.BTree(Name = "scheduled_message_sender", Columns = new[] { "SenderIdentity" })]
public partial class ScheduledMessage
{
    [SpacetimeDB.PrimaryKey]
    [SpacetimeDB.AutoInc]
    public ulong ScheduledId;

    public SpacetimeDB.ScheduleAt ScheduledAt;
    public ulong RoomId;
    public Identity SenderIdentity;
    public string Content = "";
}

// ============================================================================
// EPHEMERAL_CLEANUP TABLE - Scheduled cleanup of ephemeral messages
// ============================================================================
[SpacetimeDB.Table(Name = "ephemeral_cleanup", Scheduled = nameof(Module.CleanupEphemeralMessage))]
public partial class EphemeralCleanup
{
    [SpacetimeDB.PrimaryKey]
    [SpacetimeDB.AutoInc]
    public ulong ScheduledId;

    public SpacetimeDB.ScheduleAt ScheduledAt;
    public ulong MessageId;
}

// ============================================================================
// TYPING_CLEANUP TABLE - Scheduled cleanup of typing indicators
// ============================================================================
[SpacetimeDB.Table(Name = "typing_cleanup", Scheduled = nameof(Module.CleanupTypingIndicator))]
public partial class TypingCleanup
{
    [SpacetimeDB.PrimaryKey]
    [SpacetimeDB.AutoInc]
    public ulong ScheduledId;

    public SpacetimeDB.ScheduleAt ScheduledAt;
    public ulong TypingIndicatorId;
}

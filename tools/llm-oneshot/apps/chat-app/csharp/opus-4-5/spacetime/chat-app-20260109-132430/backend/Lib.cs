using SpacetimeDB;

// ============================================================================
// TABLES
// ============================================================================

[SpacetimeDB.Table(Name = "User", Public = true)]
public partial struct User
{
    [SpacetimeDB.PrimaryKey]
    public Identity Identity;

    public string DisplayName;
    public bool Online;
    public Timestamp LastSeen;
}

[SpacetimeDB.Table(Name = "Room", Public = true)]
public partial struct Room
{
    [SpacetimeDB.PrimaryKey]
    [SpacetimeDB.AutoInc]
    public ulong Id;

    public string Name;
    public Identity CreatedBy;
    public Timestamp CreatedAt;
}

[SpacetimeDB.Table(Name = "RoomMember", Public = true)]
public partial struct RoomMember
{
    [SpacetimeDB.PrimaryKey]
    [SpacetimeDB.AutoInc]
    public ulong Id;

    [SpacetimeDB.Index.BTree]
    public ulong RoomId;
    
    [SpacetimeDB.Index.BTree]
    public Identity UserId;
    
    public Timestamp JoinedAt;
    public Timestamp LastReadAt;
}

[SpacetimeDB.Table(Name = "Message", Public = true)]
public partial struct Message
{
    [SpacetimeDB.PrimaryKey]
    [SpacetimeDB.AutoInc]
    public ulong Id;

    [SpacetimeDB.Index.BTree]
    public ulong RoomId;
    
    public Identity SenderId;
    public string Content;
    public Timestamp CreatedAt;
    public bool IsEdited;
    public bool IsEphemeral;
    public Timestamp? ExpiresAt;
}

[SpacetimeDB.Table(Name = "MessageEdit", Public = true)]
public partial struct MessageEdit
{
    [SpacetimeDB.PrimaryKey]
    [SpacetimeDB.AutoInc]
    public ulong Id;

    [SpacetimeDB.Index.BTree]
    public ulong MessageId;
    
    public string OldContent;
    public Timestamp EditedAt;
}

[SpacetimeDB.Table(Name = "TypingIndicator", Public = true)]
public partial struct TypingIndicator
{
    [SpacetimeDB.PrimaryKey]
    [SpacetimeDB.AutoInc]
    public ulong Id;

    [SpacetimeDB.Index.BTree]
    public ulong RoomId;
    
    public Identity UserId;
    public Timestamp ExpiresAt;
}

[SpacetimeDB.Table(Name = "MessageRead", Public = true)]
public partial struct MessageRead
{
    [SpacetimeDB.PrimaryKey]
    [SpacetimeDB.AutoInc]
    public ulong Id;

    [SpacetimeDB.Index.BTree]
    public ulong MessageId;
    
    [SpacetimeDB.Index.BTree]
    public Identity UserId;
    
    public Timestamp ReadAt;
}

[SpacetimeDB.Table(Name = "ScheduledMessage", Public = true, Scheduled = "SendScheduledMessage")]
public partial struct ScheduledMessage
{
    [SpacetimeDB.PrimaryKey]
    [SpacetimeDB.AutoInc]
    public ulong ScheduledId;

    public SpacetimeDB.ScheduleAt ScheduledAt;
    
    [SpacetimeDB.Index.BTree]
    public ulong RoomId;
    
    [SpacetimeDB.Index.BTree]
    public Identity SenderId;
    
    public string Content;
    public Timestamp CreatedAt;
}

[SpacetimeDB.Table(Name = "ScheduledDeletion", Scheduled = "DeleteEphemeralMessage")]
public partial struct ScheduledDeletion
{
    [SpacetimeDB.PrimaryKey]
    [SpacetimeDB.AutoInc]
    public ulong ScheduledId;

    public SpacetimeDB.ScheduleAt ScheduledAt;
    public ulong MessageId;
}

[SpacetimeDB.Table(Name = "ScheduledTypingCleanup", Scheduled = "CleanupTypingIndicator")]
public partial struct ScheduledTypingCleanup
{
    [SpacetimeDB.PrimaryKey]
    [SpacetimeDB.AutoInc]
    public ulong ScheduledId;

    public SpacetimeDB.ScheduleAt ScheduledAt;
    public ulong TypingId;
}

[SpacetimeDB.Table(Name = "Reaction", Public = true)]
public partial struct Reaction
{
    [SpacetimeDB.PrimaryKey]
    [SpacetimeDB.AutoInc]
    public ulong Id;

    [SpacetimeDB.Index.BTree]
    public ulong MessageId;
    
    public Identity UserId;
    public string Emoji;
    public Timestamp CreatedAt;
}

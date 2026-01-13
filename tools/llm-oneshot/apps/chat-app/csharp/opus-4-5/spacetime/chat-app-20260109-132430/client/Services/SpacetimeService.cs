using SpacetimeDB;
using SpacetimeDB.Types;

namespace ChatClient.Services;

/// <summary>
/// Service for managing SpacetimeDB connection and state.
/// </summary>
public class SpacetimeService
{
    private const string DefaultUri = "http://localhost:3000";
    private const string ModuleName = "chat-app";
    private const string TokenKey = "spacetime_auth_token";

    private DbConnection? _conn;
    private Identity? _identity;
    private bool _isConnected;
    private bool _isSubscribed;

    public event Action? OnConnected;
    public event Action? OnDisconnected;
    public event Action<string>? OnError;
    public event Action? OnSubscriptionApplied;
    public event Action? OnDataChanged;

    public DbConnection? Connection => _conn;
    public Identity? Identity => _identity;
    public bool IsConnected => _isConnected;
    public bool IsSubscribed => _isSubscribed;

    public void Connect()
    {
        if (_conn != null) return;

        try
        {
            var savedToken = Preferences.Get(TokenKey, string.Empty);

            _conn = DbConnection.Builder()
                .WithUri(DefaultUri)
                .WithModuleName(ModuleName)
                .WithToken(string.IsNullOrEmpty(savedToken) ? null : savedToken)
                .OnConnect(HandleConnect)
                .OnDisconnect((conn, err) => HandleDisconnect(err))
                .OnConnectError(HandleConnectError)
                .Build();
        }
        catch (Exception ex)
        {
            OnError?.Invoke($"Failed to create connection: {ex.Message}");
        }
    }

    public void Disconnect()
    {
        _conn?.Disconnect();
        _conn = null;
        _isConnected = false;
        _isSubscribed = false;
    }

    public void FrameTick()
    {
        _conn?.FrameTick();
    }

    private void HandleConnect(DbConnection conn, Identity identity, string token)
    {
        _identity = identity;
        _isConnected = true;

        // Save token for future reconnects
        Preferences.Set(TokenKey, token);

        // Subscribe to all tables AFTER connection is established
        conn.SubscriptionBuilder()
            .OnApplied(HandleSubscriptionApplied)
            .OnError((ctx, err) => OnError?.Invoke($"Subscription error: {err}"))
            .SubscribeToAllTables();

        OnConnected?.Invoke();
    }

    private void HandleSubscriptionApplied(SubscriptionEventContext ctx)
    {
        _isSubscribed = true;
        SetupCallbacks();
        OnSubscriptionApplied?.Invoke();
    }

    private void HandleDisconnect(Exception? error)
    {
        _isConnected = false;
        _isSubscribed = false;
        OnDisconnected?.Invoke();

        if (error != null)
        {
            OnError?.Invoke($"Disconnected: {error.Message}");
        }
    }

    private void HandleConnectError(Exception error)
    {
        OnError?.Invoke($"Connection error: {error.Message}");
    }

    private void SetupCallbacks()
    {
        if (_conn == null) return;

        // User changes
        _conn.Db.User.OnInsert += (ctx, row) => OnDataChanged?.Invoke();
        _conn.Db.User.OnUpdate += (ctx, old, row) => OnDataChanged?.Invoke();
        _conn.Db.User.OnDelete += (ctx, row) => OnDataChanged?.Invoke();

        // Room changes
        _conn.Db.Room.OnInsert += (ctx, row) => OnDataChanged?.Invoke();
        _conn.Db.Room.OnDelete += (ctx, row) => OnDataChanged?.Invoke();

        // Room member changes
        _conn.Db.RoomMember.OnInsert += (ctx, row) => OnDataChanged?.Invoke();
        _conn.Db.RoomMember.OnDelete += (ctx, row) => OnDataChanged?.Invoke();
        _conn.Db.RoomMember.OnUpdate += (ctx, old, row) => OnDataChanged?.Invoke();

        // Message changes
        _conn.Db.Message.OnInsert += (ctx, row) => OnDataChanged?.Invoke();
        _conn.Db.Message.OnUpdate += (ctx, old, row) => OnDataChanged?.Invoke();
        _conn.Db.Message.OnDelete += (ctx, row) => OnDataChanged?.Invoke();

        // Message edit history
        _conn.Db.MessageEdit.OnInsert += (ctx, row) => OnDataChanged?.Invoke();

        // Typing indicators
        _conn.Db.TypingIndicator.OnInsert += (ctx, row) => OnDataChanged?.Invoke();
        _conn.Db.TypingIndicator.OnDelete += (ctx, row) => OnDataChanged?.Invoke();
        _conn.Db.TypingIndicator.OnUpdate += (ctx, old, row) => OnDataChanged?.Invoke();

        // Read receipts
        _conn.Db.MessageRead.OnInsert += (ctx, row) => OnDataChanged?.Invoke();

        // Reactions
        _conn.Db.Reaction.OnInsert += (ctx, row) => OnDataChanged?.Invoke();
        _conn.Db.Reaction.OnDelete += (ctx, row) => OnDataChanged?.Invoke();

        // Scheduled messages
        _conn.Db.ScheduledMessage.OnInsert += (ctx, row) => OnDataChanged?.Invoke();
        _conn.Db.ScheduledMessage.OnDelete += (ctx, row) => OnDataChanged?.Invoke();
    }

    // ========================================================================
    // REDUCER CALLS
    // ========================================================================

    public void SetDisplayName(string name)
    {
        _conn?.Reducers.SetDisplayName(name);
    }

    public void CreateRoom(string name)
    {
        _conn?.Reducers.CreateRoom(name);
    }

    public void JoinRoom(ulong roomId)
    {
        _conn?.Reducers.JoinRoom(roomId);
    }

    public void LeaveRoom(ulong roomId)
    {
        _conn?.Reducers.LeaveRoom(roomId);
    }

    public void SendMessage(ulong roomId, string content)
    {
        _conn?.Reducers.SendMessage(roomId, content);
    }

    public void SendEphemeralMessage(ulong roomId, string content, ulong durationMs)
    {
        _conn?.Reducers.SendEphemeralMessage(roomId, content, durationMs);
    }

    public void EditMessage(ulong messageId, string newContent)
    {
        _conn?.Reducers.EditMessage(messageId, newContent);
    }

    public void ScheduleMessage(ulong roomId, string content, long scheduledTimeMs)
    {
        _conn?.Reducers.ScheduleMessage(roomId, content, scheduledTimeMs);
    }

    public void CancelScheduledMessage(ulong scheduledId)
    {
        _conn?.Reducers.CancelScheduledMessage(scheduledId);
    }

    public void SetTyping(ulong roomId)
    {
        _conn?.Reducers.SetTyping(roomId);
    }

    public void ClearTyping(ulong roomId)
    {
        _conn?.Reducers.ClearTyping(roomId);
    }

    public void MarkMessageRead(ulong messageId)
    {
        _conn?.Reducers.MarkMessageRead(messageId);
    }

    public void MarkRoomRead(ulong roomId)
    {
        _conn?.Reducers.MarkRoomRead(roomId);
    }

    public void ToggleReaction(ulong messageId, string emoji)
    {
        _conn?.Reducers.ToggleReaction(messageId, emoji);
    }
}

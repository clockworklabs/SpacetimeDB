using System.Collections.ObjectModel;
using SpacetimeDB;
using SpacetimeDB.Types;

namespace ChatApp;

public partial class MainPage : ContentPage
{
    private DbConnection? _conn;
    private Identity? _myIdentity;
    private ulong? _currentRoomId;
    private IDispatcherTimer? _tickTimer;
    private DateTime _lastTypingSent = DateTime.MinValue;

    // Data collections
    public ObservableCollection<RoomViewModel> Rooms { get; } = new();
    public ObservableCollection<MessageViewModel> Messages { get; } = new();
    public ObservableCollection<MemberViewModel> Members { get; } = new();

    // Token persistence
    private readonly string _tokenFile = Path.Combine(
        Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData),
        "spacetimedb-chat", "auth_token.txt");

    public MainPage()
    {
        InitializeComponent();
        
        RoomsCollection.ItemsSource = Rooms;
        MessagesCollection.ItemsSource = Messages;
        MembersCollection.ItemsSource = Members;

        // Setup tick timer
        _tickTimer = Dispatcher.CreateTimer();
        _tickTimer.Interval = TimeSpan.FromMilliseconds(16);
        _tickTimer.Tick += (s, e) => _conn?.FrameTick();
        
        Loaded += OnLoaded;
        Unloaded += OnUnloaded;
    }

    private void OnLoaded(object? sender, EventArgs e)
    {
        try
        {
            LoadTokenAndConnect();
            _tickTimer?.Start();
        }
        catch (Exception ex)
        {
            ConnectionStatusLabel.Text = $"Error: {ex.Message}";
        }
    }

    private void OnUnloaded(object? sender, EventArgs e)
    {
        _tickTimer?.Stop();
        _conn?.Disconnect();
    }

    private void LoadTokenAndConnect()
    {
        string? token = null;
        try
        {
            var dir = Path.GetDirectoryName(_tokenFile);
            if (dir != null && !Directory.Exists(dir)) Directory.CreateDirectory(dir);
            if (File.Exists(_tokenFile)) token = File.ReadAllText(_tokenFile).Trim();
        }
        catch { }

        ConnectionStatusLabel.Text = "Connecting...";

        var builder = DbConnection.Builder()
            .WithUri("http://localhost:3000")
            .WithModuleName("chat-app")
            .OnConnect(OnConnected)
            .OnDisconnect((conn, err) => OnDisconnected())
            .OnConnectError(err => OnConnectError(err));

        if (!string.IsNullOrEmpty(token)) builder = builder.WithToken(token);

        _conn = builder.Build();
        RegisterCallbacks();
    }

    private void OnConnected(DbConnection conn, Identity identity, string token)
    {
        Console.WriteLine($"[DEBUG] OnConnected called! Identity: {identity}");
        _myIdentity = identity;
        try
        {
            var dir = Path.GetDirectoryName(_tokenFile);
            if (dir != null && !Directory.Exists(dir)) Directory.CreateDirectory(dir);
            File.WriteAllText(_tokenFile, token);
        }
        catch { }

        MainThread.BeginInvokeOnMainThread(() =>
        {
            ConnectionStatusLabel.Text = "Connected";
            SyncDot.Color = Color.FromArgb("#2DD4BF");
            SyncLabel.Text = "LIVE";
            SyncLabel.TextColor = Color.FromArgb("#2DD4BF");
        });

        // Subscribe AFTER connected
        Console.WriteLine("[DEBUG] Subscribing to all tables...");
        conn.SubscriptionBuilder()
            .OnApplied(OnSubscriptionApplied)
            .OnError((ctx, err) => MainThread.BeginInvokeOnMainThread(() =>
                ConnectionStatusLabel.Text = $"Sub Error: {err}"))
            .SubscribeToAllTables();
    }

    private void OnDisconnected()
    {
        MainThread.BeginInvokeOnMainThread(() =>
        {
            ConnectionStatusLabel.Text = "Disconnected";
            SyncDot.Color = Color.FromArgb("#EF4444");
            SyncLabel.Text = "OFFLINE";
            SyncLabel.TextColor = Color.FromArgb("#EF4444");
        });
    }

    private void OnConnectError(Exception error)
    {
        MainThread.BeginInvokeOnMainThread(() =>
        {
            ConnectionStatusLabel.Text = "Error";
            DisplayAlert("Connection Error", error.Message, "OK");
        });
    }

    private void OnSubscriptionApplied(SubscriptionEventContext ctx)
    {
        Console.WriteLine("[DEBUG] OnSubscriptionApplied called!");
        MainThread.BeginInvokeOnMainThread(() =>
        {
            Console.WriteLine("[DEBUG] Setting status to Online, calling RefreshRooms");
            ConnectionStatusLabel.Text = "Online";
            RefreshRooms();
            
            if (_myIdentity.HasValue)
            {
                var user = _conn?.Db.User.Identity.Find(_myIdentity.Value);
                if (user != null) DisplayNameEntry.Text = user.DisplayName;
            }
        });
    }

    private void RegisterCallbacks()
    {
        if (_conn == null) return;

        // Incremental room updates (not full refresh)
        _conn.Db.Room.OnInsert += (ctx, room) => MainThread.BeginInvokeOnMainThread(() =>
        {
            Rooms.Add(new RoomViewModel
            {
                Id = room.Id,
                Name = room.Name,
                DisplayName = $"# {room.Name}",
                UnreadCount = 0,
                HasUnread = false
            });
            RoomCountLabel.Text = $"{Rooms.Count} channel{(Rooms.Count == 1 ? "" : "s")}";
        });

        _conn.Db.Room.OnDelete += (ctx, room) => MainThread.BeginInvokeOnMainThread(() =>
        {
            var vm = Rooms.FirstOrDefault(r => r.Id == room.Id);
            if (vm != null) Rooms.Remove(vm);
            RoomCountLabel.Text = $"{Rooms.Count} channel{(Rooms.Count == 1 ? "" : "s")}";
        });

        _conn.Db.Message.OnInsert += (ctx, msg) => MainThread.BeginInvokeOnMainThread(() =>
        {
            // Add message to current room view
            if (msg.RoomId == _currentRoomId)
            {
                AddMessage(msg);
                ScrollToBottom();
            }
            // Update unread count for that room only (if not current room and not from me)
            else if (msg.SenderIdentity != _myIdentity)
            {
                var roomVm = Rooms.FirstOrDefault(r => r.Id == msg.RoomId);
                if (roomVm != null)
                {
                    roomVm.UnreadCount++;
                    roomVm.HasUnread = true;
                    // Force UI update by replacing item
                    var idx = Rooms.IndexOf(roomVm);
                    Rooms[idx] = roomVm;
                }
            }
        });

        _conn.Db.Message.OnUpdate += (ctx, oldMsg, newMsg) =>
        {
            if (newMsg.RoomId == _currentRoomId)
                MainThread.BeginInvokeOnMainThread(() => UpdateMessage(newMsg));
        };

        _conn.Db.Message.OnDelete += (ctx, msg) =>
        {
            if (msg.RoomId == _currentRoomId)
                MainThread.BeginInvokeOnMainThread(() =>
                {
                    var vm = Messages.FirstOrDefault(m => m.Id == msg.Id);
                    if (vm != null) Messages.Remove(vm);
                });
        };

        _conn.Db.User.OnInsert += (ctx, u) => MainThread.BeginInvokeOnMainThread(RefreshMembers);
        _conn.Db.User.OnUpdate += (ctx, o, n) => MainThread.BeginInvokeOnMainThread(RefreshMembers);

        _conn.Db.RoomMember.OnInsert += (ctx, m) =>
        {
            if (m.RoomId == _currentRoomId) MainThread.BeginInvokeOnMainThread(RefreshMembers);
        };
        _conn.Db.RoomMember.OnDelete += (ctx, m) =>
        {
            if (m.RoomId == _currentRoomId) MainThread.BeginInvokeOnMainThread(RefreshMembers);
        };

        _conn.Db.TypingIndicator.OnInsert += (ctx, t) =>
        {
            if (t.RoomId == _currentRoomId && t.UserIdentity != _myIdentity)
                MainThread.BeginInvokeOnMainThread(UpdateTypingIndicator);
        };
        _conn.Db.TypingIndicator.OnDelete += (ctx, t) =>
        {
            if (t.RoomId == _currentRoomId)
                MainThread.BeginInvokeOnMainThread(UpdateTypingIndicator);
        };

        _conn.Db.Reaction.OnInsert += (ctx, r) =>
        {
            var msg = _conn?.Db.Message.Id.Find(r.MessageId);
            if (msg?.RoomId == _currentRoomId)
                MainThread.BeginInvokeOnMainThread(() => RefreshMessageReactions(r.MessageId));
        };
    }

    // ============================================================================
    // UI REFRESH
    // ============================================================================

    private void RefreshRooms()
    {
        Console.WriteLine("[DEBUG] RefreshRooms called");
        if (_conn == null) { Console.WriteLine("[DEBUG] _conn is null!"); return; }
        Rooms.Clear();
        var roomCount = 0;
        foreach (var room in _conn.Db.Room.Iter())
        {
            roomCount++;
            Console.WriteLine($"[DEBUG] Found room: {room.Name} (ID: {room.Id})");
            var unread = GetUnreadCount(room.Id);
            Rooms.Add(new RoomViewModel
            {
                Id = room.Id,
                Name = room.Name,
                DisplayName = $"# {room.Name}",
                UnreadCount = unread,
                HasUnread = unread > 0
            });
        }
        Console.WriteLine($"[DEBUG] Total rooms found: {roomCount}, Rooms.Count: {Rooms.Count}");
        RoomCountLabel.Text = $"{Rooms.Count} channel{(Rooms.Count == 1 ? "" : "s")}";
    }

    private void RefreshMessages()
    {
        if (_conn == null || !_currentRoomId.HasValue) return;
        Messages.Clear();
        var msgs = _conn.Db.Message.Iter()
            .Where(m => m.RoomId == _currentRoomId.Value)
            .OrderBy(m => m.CreatedAt.MicrosecondsSinceUnixEpoch);
        foreach (var msg in msgs) AddMessage(msg);
        ScrollToBottom();
    }

    private void AddMessage(Message msg)
    {
        var vm = CreateMessageViewModel(msg);
        var insertIdx = Messages.Count;
        for (int i = 0; i < Messages.Count; i++)
        {
            if (Messages[i].CreatedAtMicros > msg.CreatedAt.MicrosecondsSinceUnixEpoch)
            {
                insertIdx = i;
                break;
            }
        }
        Messages.Insert(insertIdx, vm);
    }

    private void UpdateMessage(Message msg)
    {
        var existing = Messages.FirstOrDefault(m => m.Id == msg.Id);
        if (existing != null)
        {
            var idx = Messages.IndexOf(existing);
            Messages[idx] = CreateMessageViewModel(msg);
        }
    }

    private MessageViewModel CreateMessageViewModel(Message msg)
    {
        var sender = _conn?.Db.User.Identity.Find(msg.SenderIdentity);
        var senderName = sender?.DisplayName ?? msg.SenderIdentity.ToString().Substring(0, 8) + "...";
        var isMe = msg.SenderIdentity == _myIdentity;

        var reactions = _conn?.Db.Reaction.Iter()
            .Where(r => r.MessageId == msg.Id)
            .GroupBy(r => r.Emoji)
            .Select(g => new ReactionViewModel { Emoji = g.Key, Count = g.Count() })
            .ToList() ?? new();

        string readReceipt = "";
        if (isMe)
        {
            var readers = _conn?.Db.ReadReceipt.Iter()
                .Where(r => r.MessageId == msg.Id && r.UserIdentity != _myIdentity)
                .Select(r => _conn?.Db.User.Identity.Find(r.UserIdentity)?.DisplayName ?? "Unknown")
                .ToList();
            if (readers?.Count > 0) readReceipt = $"✓ Seen by {string.Join(", ", readers)}";
        }

        var hasHistory = _conn?.Db.MessageEdit.Iter().Any(e => e.MessageId == msg.Id) ?? false;
        var ts = DateTimeOffset.FromUnixTimeMilliseconds(msg.CreatedAt.MicrosecondsSinceUnixEpoch / 1000).LocalDateTime;

        return new MessageViewModel
        {
            Id = msg.Id,
            IdText = $"ID: {msg.Id}",
            SenderName = senderName,
            Content = msg.Content,
            Timestamp = ts.ToString("HH:mm"),
            EditedIndicator = msg.IsEdited ? "(edited)" : "",
            EphemeralIndicator = msg.IsEphemeral ? "⏱ disappearing" : "",
            CanEdit = isMe && !msg.IsEphemeral,
            HasHistory = hasHistory,
            Reactions = new ObservableCollection<ReactionViewModel>(reactions),
            ReadReceipt = readReceipt,
            HasReadReceipt = !string.IsNullOrEmpty(readReceipt),
            CreatedAtMicros = msg.CreatedAt.MicrosecondsSinceUnixEpoch
        };
    }

    private void RefreshMembers()
    {
        if (_conn == null || !_currentRoomId.HasValue) return;
        Members.Clear();
        var memberIds = _conn.Db.RoomMember.Iter()
            .Where(m => m.RoomId == _currentRoomId.Value)
            .Select(m => m.UserIdentity);
        
        foreach (var id in memberIds)
        {
            var user = _conn.Db.User.Identity.Find(id);
            if (user != null)
            {
                Members.Add(new MemberViewModel
                {
                    Identity = id,
                    DisplayName = user.DisplayName,
                    IsOnline = user.IsOnline
                });
            }
        }
    }

    private void RefreshMessageReactions(ulong messageId)
    {
        var vm = Messages.FirstOrDefault(m => m.Id == messageId);
        if (vm == null) return;
        var reactions = _conn?.Db.Reaction.Iter()
            .Where(r => r.MessageId == messageId)
            .GroupBy(r => r.Emoji)
            .Select(g => new ReactionViewModel { Emoji = g.Key, Count = g.Count() })
            .ToList() ?? new();
        vm.Reactions = new ObservableCollection<ReactionViewModel>(reactions);
    }

    private void UpdateTypingIndicator()
    {
        if (_conn == null || !_currentRoomId.HasValue) { TypingBorder.IsVisible = false; return; }
        var typingUsers = _conn.Db.TypingIndicator.Iter()
            .Where(t => t.RoomId == _currentRoomId.Value && t.UserIdentity != _myIdentity)
            .Select(t => _conn.Db.User.Identity.Find(t.UserIdentity)?.DisplayName ?? "Someone")
            .ToList();

        if (typingUsers.Count == 0) { TypingBorder.IsVisible = false; }
        else
        {
            TypingLabel.Text = typingUsers.Count == 1 ? $"{typingUsers[0]} is typing..." : "Several people are typing...";
            TypingBorder.IsVisible = true;
        }
    }

    private int GetUnreadCount(ulong roomId)
    {
        if (_conn == null || _myIdentity == null) return 0;
        ulong lastReadId = 0;
        foreach (var lr in _conn.Db.LastRead.Iter())
            if (lr.UserIdentity == _myIdentity && lr.RoomId == roomId) { lastReadId = lr.LastMessageId; break; }
        return _conn.Db.Message.Iter().Count(m => m.RoomId == roomId && m.Id > lastReadId && m.SenderIdentity != _myIdentity);
    }

    private async void ScrollToBottom()
    {
        await Task.Delay(50);
        if (Messages.Count > 0)
            await MessagesScrollView.ScrollToAsync(0, double.MaxValue, false);
    }

    // ============================================================================
    // EVENT HANDLERS
    // ============================================================================

    private void CreateRoom_Clicked(object? sender, EventArgs e)
    {
        var name = NewRoomEntry.Text?.Trim();
        if (string.IsNullOrEmpty(name)) return;
        _conn?.Reducers.CreateRoom(name);
        NewRoomEntry.Text = "";
    }

    private void RoomsCollection_SelectionChanged(object? sender, SelectionChangedEventArgs e)
    {
        if (e.CurrentSelection.FirstOrDefault() is RoomViewModel room)
        {
            var isMember = _conn?.Db.RoomMember.Iter()
                .Any(m => m.RoomId == room.Id && m.UserIdentity == _myIdentity) ?? false;
            if (!isMember) _conn?.Reducers.JoinRoom(room.Id);

            _currentRoomId = room.Id;
            CurrentRoomLabel.Text = room.Name;
            RefreshMessages();
            RefreshMembers();
            UpdateTypingIndicator();

            var latestMsg = _conn?.Db.Message.Iter()
                .Where(m => m.RoomId == room.Id)
                .OrderByDescending(m => m.CreatedAt.MicrosecondsSinceUnixEpoch)
                .FirstOrDefault();
            if (latestMsg != null) _conn?.Reducers.UpdateLastRead(room.Id, latestMsg.Id);
        }
    }

    private void DisplayName_Completed(object? sender, EventArgs e)
    {
        var name = DisplayNameEntry.Text?.Trim();
        if (!string.IsNullOrEmpty(name)) _conn?.Reducers.SetDisplayName(name);
    }

    private void MessageEntry_TextChanged(object? sender, TextChangedEventArgs e)
    {
        if (_currentRoomId.HasValue && !string.IsNullOrEmpty(MessageEntry.Text))
        {
            if ((DateTime.Now - _lastTypingSent).TotalSeconds > 3)
            {
                _conn?.Reducers.StartTyping(_currentRoomId.Value);
                _lastTypingSent = DateTime.Now;
            }
        }
    }

    private void MessageEntry_Completed(object? sender, EventArgs e) => SendMessage();
    private void Send_Clicked(object? sender, EventArgs e) => SendMessage();

    private void SendMessage()
    {
        if (!_currentRoomId.HasValue) return;
        var content = MessageEntry.Text?.Trim();
        if (string.IsNullOrEmpty(content)) return;
        _conn?.Reducers.SendMessage(_currentRoomId.Value, content);
        MessageEntry.Text = "";
    }

    private async void Reaction_Clicked(object? sender, EventArgs e)
    {
        if (sender is Button btn && btn.CommandParameter is ulong messageId)
        {
            var emoji = await DisplayActionSheet("Add Reaction", "Cancel", null, "👍", "❤️", "😂", "😮", "😢", "🎉", "🔥", "👀");
            if (!string.IsNullOrEmpty(emoji) && emoji != "Cancel")
                _conn?.Reducers.ToggleReaction(messageId, emoji);
        }
    }

    private async void Edit_Clicked(object? sender, EventArgs e)
    {
        if (sender is Button btn && btn.CommandParameter is ulong messageId)
        {
            var msg = _conn?.Db.Message.Id.Find(messageId);
            if (msg == null) return;
            var newContent = await DisplayPromptAsync("Edit Message", "Edit your message:", initialValue: msg.Content);
            if (!string.IsNullOrEmpty(newContent))
                _conn?.Reducers.EditMessage(messageId, newContent);
        }
    }

    private async void History_Clicked(object? sender, EventArgs e)
    {
        if (sender is Button btn && btn.CommandParameter is ulong messageId)
        {
            var edits = _conn?.Db.MessageEdit.Iter()
                .Where(ed => ed.MessageId == messageId)
                .OrderBy(ed => ed.EditedAt.MicrosecondsSinceUnixEpoch)
                .Select(ed => $"{DateTimeOffset.FromUnixTimeMilliseconds(ed.EditedAt.MicrosecondsSinceUnixEpoch / 1000).LocalDateTime:g}: {ed.PreviousContent}")
                .ToList() ?? new();
            
            var currentMsg = _conn?.Db.Message.Id.Find(messageId);
            if (currentMsg != null) edits.Add($"(current): {currentMsg.Content}");
            
            await DisplayAlert("Edit History", string.Join("\n\n", edits), "Close");
        }
    }

    private async void Schedule_Clicked(object? sender, EventArgs e)
    {
        if (!_currentRoomId.HasValue) { await DisplayAlert("Error", "Join a room first", "OK"); return; }
        var delay = await DisplayPromptAsync("Schedule Message", "Delay in seconds:", keyboard: Keyboard.Numeric);
        if (string.IsNullOrEmpty(delay) || !uint.TryParse(delay, out var seconds)) return;
        var content = await DisplayPromptAsync("Schedule Message", "Message:");
        if (string.IsNullOrEmpty(content)) return;
        _conn?.Reducers.ScheduleMessage(_currentRoomId.Value, content, (ulong)seconds * 1000);
    }

    private async void Ephemeral_Clicked(object? sender, EventArgs e)
    {
        if (!_currentRoomId.HasValue) { await DisplayAlert("Error", "Join a room first", "OK"); return; }
        var lifetime = await DisplayPromptAsync("Ephemeral Message", "Lifetime in seconds (min 10):", keyboard: Keyboard.Numeric);
        if (string.IsNullOrEmpty(lifetime) || !uint.TryParse(lifetime, out var seconds)) return;
        var content = await DisplayPromptAsync("Ephemeral Message", "Message:");
        if (string.IsNullOrEmpty(content)) return;
        _conn?.Reducers.SendEphemeralMessage(_currentRoomId.Value, content, (ulong)seconds * 1000);
    }
}

// ============================================================================
// VIEW MODELS
// ============================================================================

public class RoomViewModel
{
    public ulong Id { get; set; }
    public string Name { get; set; } = "";
    public string DisplayName { get; set; } = "";
    public int UnreadCount { get; set; }
    public bool HasUnread { get; set; }
}

public class MessageViewModel
{
    public ulong Id { get; set; }
    public string IdText { get; set; } = "";
    public string SenderName { get; set; } = "";
    public string Content { get; set; } = "";
    public string Timestamp { get; set; } = "";
    public string EditedIndicator { get; set; } = "";
    public string EphemeralIndicator { get; set; } = "";
    public bool CanEdit { get; set; }
    public bool HasHistory { get; set; }
    public long CreatedAtMicros { get; set; }
    public ObservableCollection<ReactionViewModel> Reactions { get; set; } = new();
    public string ReadReceipt { get; set; } = "";
    public bool HasReadReceipt { get; set; }
}

public class MemberViewModel
{
    public Identity Identity { get; set; }
    public string DisplayName { get; set; } = "";
    public bool IsOnline { get; set; }
    public Color OnlineColor => IsOnline ? Color.FromArgb("#14B8A6") : Color.FromArgb("#64748B");
    public Color OnlineTextColor => IsOnline ? Color.FromArgb("#CBD5E1") : Color.FromArgb("#64748B");
}

public class ReactionViewModel
{
    public string Emoji { get; set; } = "";
    public int Count { get; set; }
}

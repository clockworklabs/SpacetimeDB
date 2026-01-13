using SpacetimeDB;
using SpacetimeDB.Types;
using System.Collections.ObjectModel;
using Microsoft.Maui.Layouts;

namespace ChatApp;

public partial class MainPage : ContentPage
{
    private const string SPACETIMEDB_URI = "http://localhost:3000";
    private const string MODULE_NAME = "chat-app";
    private const string TOKEN_KEY = "spacetimedb_token";

    private DbConnection? _conn;
    private Identity? _myIdentity;
    private IDispatcherTimer? _timer;
    private IDispatcherTimer? _activityTimer;

    private ulong? _selectedRoomId;
    private ulong? _replyToMessageId;
    private ulong? _threadParentId;
    private long? _scheduleAtMicros;

    // View models
    public ObservableCollection<RoomViewModel> Rooms { get; } = new();
    public ObservableCollection<InviteViewModel> Invites { get; } = new();
    public ObservableCollection<MemberViewModel> Members { get; } = new();
    public ObservableCollection<ScheduledMessageViewModel> ScheduledMessages { get; } = new();
    public ObservableCollection<EditHistoryViewModel> EditHistory { get; } = new();

    public MainPage()
    {
        InitializeComponent();
        RoomsList.ItemsSource = Rooms;
        InvitesList.ItemsSource = Invites;
        MembersList.ItemsSource = Members;
        ScheduledMessagesList.ItemsSource = ScheduledMessages;
        EditHistoryList.ItemsSource = EditHistory;

        StatusPicker.SelectedIndex = 0;
        EphemeralPicker.SelectedIndex = 1;

        Connect();
    }

    private void Connect()
    {
        var savedToken = Preferences.Get(TOKEN_KEY, string.Empty);

        _conn = DbConnection.Builder()
            .WithUri(SPACETIMEDB_URI)
            .WithModuleName(MODULE_NAME)
            .WithToken(string.IsNullOrEmpty(savedToken) ? null : savedToken)
            .OnConnect(OnConnected)
            .OnDisconnect((conn, err) => MainThread.BeginInvokeOnMainThread(() => OnDisconnected(err)))
            .OnConnectError(err => MainThread.BeginInvokeOnMainThread(() => OnConnectError(err)))
            .Build();

        // Start frame tick timer
        _timer = Dispatcher.CreateTimer();
        _timer.Interval = TimeSpan.FromMilliseconds(16);
        _timer.Tick += (s, e) => _conn?.FrameTick();
        _timer.Start();

        // Activity update timer (every 30 seconds)
        _activityTimer = Dispatcher.CreateTimer();
        _activityTimer.Interval = TimeSpan.FromSeconds(30);
        _activityTimer.Tick += (s, e) => _conn?.Reducers.UpdateActivity();
        _activityTimer.Start();
    }

    private void OnConnected(DbConnection conn, Identity identity, string token)
    {
        _myIdentity = identity;
        Preferences.Set(TOKEN_KEY, token);

        MainThread.BeginInvokeOnMainThread(() =>
        {
            ConnectionStatus.Text = "Connected";
            ConnectionStatus.TextColor = (Color)Application.Current!.Resources["StdbGreen"];
        });

        // Register callbacks
        RegisterCallbacks();

        // Subscribe
        conn.SubscriptionBuilder()
            .OnApplied(OnSubscriptionApplied)
            .OnError((ctx, err) => MainThread.BeginInvokeOnMainThread(() =>
                ConnectionStatus.Text = $"Subscription error: {err}"))
            .SubscribeToAllTables();
    }

    private void OnDisconnected(Exception? err)
    {
        ConnectionStatus.Text = err != null ? $"Disconnected: {err.Message}" : "Disconnected";
        ConnectionStatus.TextColor = (Color)Application.Current!.Resources["StdbRed"];
    }

    private void OnConnectError(Exception err)
    {
        ConnectionStatus.Text = $"Connection failed: {err.Message}";
        ConnectionStatus.TextColor = (Color)Application.Current!.Resources["StdbRed"];

        if (err.Message?.Contains("Unauthorized") == true || err.Message?.Contains("401") == true)
        {
            Preferences.Remove(TOKEN_KEY);
        }
    }

    private void RegisterCallbacks()
    {
        if (_conn == null) return;

        _conn.Db.User.OnInsert += (ctx, user) => MainThread.BeginInvokeOnMainThread(() => OnUserChanged());
        _conn.Db.User.OnUpdate += (ctx, old, user) => MainThread.BeginInvokeOnMainThread(() => OnUserChanged());
        _conn.Db.User.OnDelete += (ctx, user) => MainThread.BeginInvokeOnMainThread(() => OnUserChanged());

        _conn.Db.Room.OnInsert += (ctx, room) => MainThread.BeginInvokeOnMainThread(RefreshRooms);
        _conn.Db.Room.OnUpdate += (ctx, old, room) => MainThread.BeginInvokeOnMainThread(RefreshRooms);
        _conn.Db.Room.OnDelete += (ctx, room) => MainThread.BeginInvokeOnMainThread(RefreshRooms);

        _conn.Db.RoomMember.OnInsert += (ctx, m) => MainThread.BeginInvokeOnMainThread(() => OnMembershipChanged(m.RoomId));
        _conn.Db.RoomMember.OnUpdate += (ctx, old, m) => MainThread.BeginInvokeOnMainThread(() => OnMembershipChanged(m.RoomId));
        _conn.Db.RoomMember.OnDelete += (ctx, m) => MainThread.BeginInvokeOnMainThread(() => OnMembershipChanged(m.RoomId));

        _conn.Db.Message.OnInsert += (ctx, msg) => MainThread.BeginInvokeOnMainThread(() => OnMessageInserted(msg));
        _conn.Db.Message.OnUpdate += (ctx, old, msg) => MainThread.BeginInvokeOnMainThread(() => RefreshMessages());
        _conn.Db.Message.OnDelete += (ctx, msg) => MainThread.BeginInvokeOnMainThread(() => RefreshMessages());

        _conn.Db.TypingIndicator.OnInsert += (ctx, t) => MainThread.BeginInvokeOnMainThread(RefreshTypingIndicators);
        _conn.Db.TypingIndicator.OnDelete += (ctx, t) => MainThread.BeginInvokeOnMainThread(RefreshTypingIndicators);

        _conn.Db.MessageRead.OnInsert += (ctx, r) => MainThread.BeginInvokeOnMainThread(RefreshMessages);

        _conn.Db.Reaction.OnInsert += (ctx, r) => MainThread.BeginInvokeOnMainThread(RefreshMessages);
        _conn.Db.Reaction.OnDelete += (ctx, r) => MainThread.BeginInvokeOnMainThread(RefreshMessages);

        _conn.Db.RoomInvite.OnInsert += (ctx, i) => MainThread.BeginInvokeOnMainThread(RefreshInvites);
        _conn.Db.RoomInvite.OnUpdate += (ctx, old, i) => MainThread.BeginInvokeOnMainThread(RefreshInvites);

        _conn.Db.Draft.OnInsert += (ctx, d) => MainThread.BeginInvokeOnMainThread(() => OnDraftChanged(d));
        _conn.Db.Draft.OnUpdate += (ctx, old, d) => MainThread.BeginInvokeOnMainThread(() => OnDraftChanged(d));

        _conn.Db.ScheduledMessage.OnInsert += (ctx, s) => MainThread.BeginInvokeOnMainThread(RefreshScheduledMessages);
        _conn.Db.ScheduledMessage.OnDelete += (ctx, s) => MainThread.BeginInvokeOnMainThread(RefreshScheduledMessages);
    }

    private void OnSubscriptionApplied(SubscriptionEventContext ctx)
    {
        MainThread.BeginInvokeOnMainThread(() =>
        {
            OnUserChanged();
            RefreshRooms();
            RefreshInvites();
        });
    }

    // ========================================================================
    // USER MANAGEMENT
    // ========================================================================

    private void OnUserChanged()
    {
        if (_conn == null || _myIdentity == null) return;

        var me = _conn.Db.User.Identity.Find(_myIdentity.Value);
        if (me != null)
        {
            if (!string.IsNullOrEmpty(me.Username))
            {
                UserSetupPanel.IsVisible = false;
                UserInfoPanel.IsVisible = true;
                CurrentUsername.Text = me.Username;
                StatusText.Text = GetStatusText(me.Status);
                StatusIndicator.Color = GetStatusColor(me.Status);
            }
            else
            {
                UserSetupPanel.IsVisible = true;
                UserInfoPanel.IsVisible = false;
            }
        }

        if (_selectedRoomId.HasValue)
        {
            RefreshRoomHeader();
            RefreshMembers();
        }
    }

    private void OnSetUsername(object sender, EventArgs e)
    {
        var username = UsernameEntry.Text?.Trim();
        if (string.IsNullOrEmpty(username)) return;

        try
        {
            _conn?.Reducers.SetUsername(username);
        }
        catch (Exception ex)
        {
            DisplayAlert("Error", ex.Message, "OK");
        }
    }

    private void OnShowSettings(object sender, EventArgs e)
    {
        SettingsDialog.IsVisible = true;
    }

    private void OnCloseSettings(object sender, EventArgs e)
    {
        SettingsDialog.IsVisible = false;
    }

    private void OnSaveSettings(object sender, EventArgs e)
    {
        var status = StatusPicker.SelectedIndex switch
        {
            0 => UserStatus.Online,
            1 => UserStatus.Away,
            2 => UserStatus.DoNotDisturb,
            3 => UserStatus.Invisible,
            _ => UserStatus.Online
        };

        try
        {
            _conn?.Reducers.SetStatus(status);
            SettingsDialog.IsVisible = false;
        }
        catch (Exception ex)
        {
            DisplayAlert("Error", ex.Message, "OK");
        }
    }

    // ========================================================================
    // ROOMS
    // ========================================================================

    private void RefreshRooms()
    {
        if (_conn == null || _myIdentity == null) return;

        Rooms.Clear();

        var myMemberships = _conn.Db.RoomMember.Iter()
            .Where(m => m.UserId == _myIdentity && !m.IsKicked && !m.IsBanned)
            .ToList();

        foreach (var membership in myMemberships)
        {
            var room = _conn.Db.Room.Id.Find(membership.RoomId);
            if (room == null) continue;

            var messages = _conn.Db.Message.Iter()
                .Where(m => m.RoomId == room.Id && m.Id > membership.LastReadMessageId)
                .Count();

            var activity = _conn.Db.RoomActivity.RoomId.Find(room.Id);
            var activityText = "";
            if (activity != null)
            {
                var lastMsg = activity.LastMessageAt;
                var elapsed = DateTimeOffset.UtcNow - DateTimeOffset.FromUnixTimeMilliseconds(
                    lastMsg.MicrosecondsSinceUnixEpoch / 1000);
                activityText = elapsed.TotalMinutes < 5 ? "Active now" :
                    elapsed.TotalHours < 1 ? $"{(int)elapsed.TotalMinutes}m ago" :
                    elapsed.TotalDays < 1 ? $"{(int)elapsed.TotalHours}h ago" : "";
            }

            var displayName = room.Name;
            if (room.IsDm)
            {
                var otherMember = _conn.Db.RoomMember.Iter()
                    .FirstOrDefault(m => m.RoomId == room.Id && m.UserId != _myIdentity);
                if (otherMember != null)
                {
                    var otherUser = _conn.Db.User.Identity.Find(otherMember.UserId);
                    displayName = otherUser?.Username ?? "Unknown";
                }
            }

            Rooms.Add(new RoomViewModel
            {
                Id = room.Id,
                Name = displayName,
                Icon = room.IsDm ? "💬" : room.IsPrivate ? "🔒" : "#️⃣",
                UnreadCount = messages,
                HasUnread = messages > 0,
                ActivityText = activityText,
                IsPrivate = room.IsPrivate,
                IsDm = room.IsDm
            });
        }

        foreach (var room in _conn.Db.Room.Iter())
        {
            if (room.IsPrivate || room.IsDm) continue;
            if (Rooms.Any(r => r.Id == room.Id)) continue;

            Rooms.Add(new RoomViewModel
            {
                Id = room.Id,
                Name = room.Name,
                Icon = "#️⃣",
                UnreadCount = 0,
                HasUnread = false,
                ActivityText = "Not joined",
                IsPrivate = false,
                IsDm = false
            });
        }
    }

    private void OnRoomSelected(object sender, SelectionChangedEventArgs e)
    {
        if (e.CurrentSelection.FirstOrDefault() is not RoomViewModel room) return;

        _selectedRoomId = room.Id;
        _replyToMessageId = null;
        ReplyPreview.IsVisible = false;

        NoRoomPanel.IsVisible = false;
        ChatPanel.IsVisible = true;

        RefreshRoomHeader();
        RefreshMessages();
        RefreshTypingIndicators();
        LoadDraft();

        _conn?.Reducers.MarkRoomRead(room.Id);
    }

    private void RefreshRoomHeader()
    {
        if (_conn == null || !_selectedRoomId.HasValue) return;

        var room = _conn.Db.Room.Id.Find(_selectedRoomId.Value);
        if (room == null) return;

        RoomTitle.Text = room.IsDm ? GetDmPartnerName(room.Id) : room.Name;

        var memberCount = _conn.Db.RoomMember.Iter()
            .Count(m => m.RoomId == room.Id && !m.IsKicked && !m.IsBanned);
        var onlineCount = _conn.Db.RoomMember.Iter()
            .Where(m => m.RoomId == room.Id && !m.IsKicked && !m.IsBanned)
            .Count(m => {
                var user = _conn.Db.User.Identity.Find(m.UserId);
                return user?.Status == UserStatus.Online;
            });

        RoomMembers.Text = $"{memberCount} members, {onlineCount} online";
    }

    private string GetDmPartnerName(ulong roomId)
    {
        if (_conn == null || _myIdentity == null) return "DM";

        var otherMember = _conn.Db.RoomMember.Iter()
            .FirstOrDefault(m => m.RoomId == roomId && m.UserId != _myIdentity);
        if (otherMember == null) return "DM";

        var otherUser = _conn.Db.User.Identity.Find(otherMember.UserId);
        return otherUser?.Username ?? "Unknown";
    }

    private void OnMembershipChanged(ulong roomId)
    {
        RefreshRooms();
        if (_selectedRoomId == roomId)
        {
            RefreshRoomHeader();
            RefreshMembers();
        }
    }

    private void OnCreateRoom(object sender, EventArgs e)
    {
        NewRoomName.Text = "";
        IsPrivateRoom.IsChecked = false;
        CreateRoomDialog.IsVisible = true;
    }

    private void OnCancelCreateRoom(object sender, EventArgs e)
    {
        CreateRoomDialog.IsVisible = false;
    }

    private void OnConfirmCreateRoom(object sender, EventArgs e)
    {
        var name = NewRoomName.Text?.Trim();
        if (string.IsNullOrEmpty(name)) return;

        try
        {
            _conn?.Reducers.CreateRoom(name, IsPrivateRoom.IsChecked);
            CreateRoomDialog.IsVisible = false;
        }
        catch (Exception ex)
        {
            DisplayAlert("Error", ex.Message, "OK");
        }
    }

    private void OnLeaveRoom(object sender, EventArgs e)
    {
        if (!_selectedRoomId.HasValue) return;

        try
        {
            _conn?.Reducers.LeaveRoom(_selectedRoomId.Value);
            _selectedRoomId = null;
            ChatPanel.IsVisible = false;
            NoRoomPanel.IsVisible = true;
        }
        catch (Exception ex)
        {
            DisplayAlert("Error", ex.Message, "OK");
        }
    }

    // ========================================================================
    // MESSAGES
    // ========================================================================

    private void OnMessageInserted(Message msg)
    {
        if (msg.RoomId == _selectedRoomId)
        {
            RefreshMessages();
            MessagesScroll.ScrollToAsync(0, MessagesContainer.Height, true);
            _conn?.Reducers.MarkMessageRead(msg.Id);
        }

        RefreshRooms();
    }

    private void RefreshMessages()
    {
        if (_conn == null || !_selectedRoomId.HasValue || _myIdentity == null) return;

        MessagesContainer.Children.Clear();

        var messages = _conn.Db.Message.Iter()
            .Where(m => m.RoomId == _selectedRoomId.Value && m.ParentMessageId == 0)
            .OrderBy(m => m.CreatedAt.MicrosecondsSinceUnixEpoch)
            .ToList();

        foreach (var msg in messages)
        {
            var messageView = CreateMessageView(msg);
            MessagesContainer.Children.Add(messageView);
        }
    }

    private View CreateMessageView(Message msg)
    {
        var sender = _conn!.Db.User.Identity.Find(msg.SenderId);
        var senderName = sender?.Username ?? "Anonymous";
        var isOwnMessage = msg.SenderId == _myIdentity;

        var frame = new Frame
        {
            BackgroundColor = isOwnMessage
                ? Color.FromArgb("#1a4cf490")
                : (Color)Application.Current!.Resources["BgSurface"],
            BorderColor = (Color)Application.Current!.Resources["BorderColor"],
            CornerRadius = 8,
            Padding = new Thickness(12),
            HorizontalOptions = isOwnMessage ? LayoutOptions.End : LayoutOptions.Start,
            MaximumWidthRequest = 500
        };

        var stack = new StackLayout { Spacing = 4 };

        var headerGrid = new Grid { ColumnDefinitions = new ColumnDefinitionCollection { new(GridLength.Star), new(GridLength.Auto) } };
        var nameLabel = new Label
        {
            Text = senderName,
            TextColor = isOwnMessage
                ? (Color)Application.Current!.Resources["StdbGreen"]
                : (Color)Application.Current!.Resources["StdbPurple"],
            FontAttributes = FontAttributes.Bold
        };
        headerGrid.Add(nameLabel, 0);

        var timeLabel = new Label
        {
            Text = FormatTime(msg.CreatedAt),
            Style = (Style)Application.Current!.Resources["MutedLabel"]
        };
        headerGrid.Add(timeLabel, 1);
        stack.Children.Add(headerGrid);

        var textLabel = new Label
        {
            Text = msg.Text,
            TextColor = (Color)Application.Current!.Resources["TextPrimary"]
        };
        stack.Children.Add(textLabel);

        if (msg.IsEphemeral && msg.ExpiresAtMicros > 0)
        {
            var remaining = msg.ExpiresAtMicros / 1_000_000 -
                DateTimeOffset.UtcNow.ToUnixTimeSeconds();
            var expiryLabel = new Label
            {
                Text = $"💨 Disappears in {Math.Max(0, remaining / 60)}m {Math.Max(0, remaining % 60)}s",
                TextColor = (Color)Application.Current!.Resources["StdbYellow"],
                FontSize = 11
            };
            stack.Children.Add(expiryLabel);
        }

        if (msg.IsEdited)
        {
            var editedLabel = new Label
            {
                Text = "(edited)",
                Style = (Style)Application.Current!.Resources["MutedLabel"],
                FontSize = 11
            };
            var tapGesture = new TapGestureRecognizer();
            tapGesture.Tapped += (s, e) => ShowEditHistory(msg.Id);
            editedLabel.GestureRecognizers.Add(tapGesture);
            stack.Children.Add(editedLabel);
        }

        var reactions = _conn.Db.Reaction.Iter().Where(r => r.MessageId == msg.Id).ToList();
        if (reactions.Any())
        {
            var reactionsStack = new FlexLayout { Wrap = Microsoft.Maui.Layouts.FlexWrap.Wrap };
            var grouped = reactions.GroupBy(r => r.Emoji);
            foreach (var group in grouped)
            {
                var reactorNames = group.Select(r =>
                    _conn.Db.User.Identity.Find(r.UserId)?.Username ?? "?").ToList();

                var btn = new Button
                {
                    Text = $"{group.Key} {group.Count()}",
                    BackgroundColor = (Color)Application.Current!.Resources["BgDark"],
                    TextColor = (Color)Application.Current!.Resources["TextBody"],
                    Padding = new Thickness(8, 4),
                    CornerRadius = 12,
                    FontSize = 12,
                    Margin = new Thickness(2)
                };
                btn.Clicked += (s, e) => ToggleReaction(msg.Id, group.Key);
                ToolTipProperties.SetText(btn, string.Join(", ", reactorNames));
                reactionsStack.Children.Add(btn);
            }
            stack.Children.Add(reactionsStack);
        }

        var reads = _conn.Db.MessageRead.Iter().Where(r => r.MessageId == msg.Id).ToList();
        if (reads.Any())
        {
            var readNames = reads
                .Where(r => r.UserId != msg.SenderId)
                .Select(r => _conn.Db.User.Identity.Find(r.UserId)?.Username ?? "?")
                .Take(3)
                .ToList();
            if (readNames.Any())
            {
                var seenText = readNames.Count > 3
                    ? $"Seen by {string.Join(", ", readNames)} and {reads.Count - 3} others"
                    : $"Seen by {string.Join(", ", readNames)}";
                var seenLabel = new Label
                {
                    Text = seenText,
                    Style = (Style)Application.Current!.Resources["MutedLabel"],
                    FontSize = 10
                };
                stack.Children.Add(seenLabel);
            }
        }

        var replyCount = _conn.Db.Message.Iter().Count(m => m.ParentMessageId == msg.Id);
        if (replyCount > 0)
        {
            var threadBtn = new Button
            {
                Text = $"💬 {replyCount} replies",
                BackgroundColor = Color.FromArgb("Transparent"),
                TextColor = (Color)Application.Current!.Resources["StdbBlue"],
                Padding = new Thickness(0),
                FontSize = 12
            };
            threadBtn.Clicked += (s, e) => ShowThread(msg.Id);
            stack.Children.Add(threadBtn);
        }

        var actionsStack = new HorizontalStackLayout { Spacing = 4, Margin = new Thickness(0, 8, 0, 0) };

        var emojis = new[] { "👍", "❤️", "😂", "😮", "😢" };
        foreach (var emoji in emojis)
        {
            var emojiBtn = new Button
            {
                Text = emoji,
                Style = (Style)Application.Current!.Resources["EmojiButton"]
            };
            emojiBtn.Clicked += (s, e) => ToggleReaction(msg.Id, emoji);
            actionsStack.Children.Add(emojiBtn);
        }

        var replyBtn = new Button
        {
            Text = "↩️",
            Style = (Style)Application.Current!.Resources["EmojiButton"]
        };
        replyBtn.Clicked += (s, e) => StartReply(msg);
        actionsStack.Children.Add(replyBtn);

        if (isOwnMessage)
        {
            var editBtn = new Button
            {
                Text = "✏️",
                Style = (Style)Application.Current!.Resources["EmojiButton"]
            };
            editBtn.Clicked += async (s, e) => await EditMessage(msg);
            actionsStack.Children.Add(editBtn);

            var deleteBtn = new Button
            {
                Text = "🗑️",
                Style = (Style)Application.Current!.Resources["EmojiButton"]
            };
            deleteBtn.Clicked += async (s, e) => await DeleteMessage(msg.Id);
            actionsStack.Children.Add(deleteBtn);
        }

        stack.Children.Add(actionsStack);
        frame.Content = stack;
        return frame;
    }

    private void ToggleReaction(ulong messageId, string emoji)
    {
        if (_conn == null || _myIdentity == null) return;

        var existing = _conn.Db.Reaction.Iter()
            .FirstOrDefault(r => r.MessageId == messageId && r.UserId == _myIdentity && r.Emoji == emoji);

        if (existing != null)
        {
            _conn.Reducers.RemoveReaction(messageId, emoji);
        }
        else
        {
            _conn.Reducers.AddReaction(messageId, emoji);
        }
    }

    private void StartReply(Message msg)
    {
        _replyToMessageId = msg.Id;
        ReplyPreview.IsVisible = true;
        var sender = _conn!.Db.User.Identity.Find(msg.SenderId);
        ReplyToText.Text = $"{sender?.Username ?? "?"}: {Truncate(msg.Text, 50)}";
        MessageInput.Focus();
    }

    private void OnCancelReply(object sender, EventArgs e)
    {
        _replyToMessageId = null;
        ReplyPreview.IsVisible = false;
    }

    private async Task EditMessage(Message msg)
    {
        var result = await DisplayPromptAsync("Edit Message", "Edit your message:", initialValue: msg.Text);
        if (!string.IsNullOrEmpty(result) && result != msg.Text)
        {
            try
            {
                _conn?.Reducers.EditMessage(msg.Id, result);
            }
            catch (Exception ex)
            {
                await DisplayAlert("Error", ex.Message, "OK");
            }
        }
    }

    private async Task DeleteMessage(ulong messageId)
    {
        var confirm = await DisplayAlert("Delete Message", "Are you sure?", "Delete", "Cancel");
        if (confirm)
        {
            try
            {
                _conn?.Reducers.DeleteMessage(messageId);
            }
            catch (Exception ex)
            {
                await DisplayAlert("Error", ex.Message, "OK");
            }
        }
    }

    private void OnSendMessage(object sender, EventArgs e)
    {
        if (_conn == null || !_selectedRoomId.HasValue) return;

        var text = MessageInput.Text?.Trim();
        if (string.IsNullOrEmpty(text)) return;

        try
        {
            if (_scheduleAtMicros.HasValue)
            {
                _conn.Reducers.ScheduleMessage(_selectedRoomId.Value, text, _scheduleAtMicros.Value,
                    _replyToMessageId ?? 0);
                _scheduleAtMicros = null;
                SchedulePreview.IsVisible = false;
            }
            else
            {
                _conn.Reducers.SendMessage(_selectedRoomId.Value, text, _replyToMessageId ?? 0);
            }

            MessageInput.Text = "";
            _replyToMessageId = null;
            ReplyPreview.IsVisible = false;

            _conn.Reducers.ClearDraft(_selectedRoomId.Value);
        }
        catch (Exception ex)
        {
            DisplayAlert("Error", ex.Message, "OK");
        }
    }

    private void OnMessageTextChanged(object sender, TextChangedEventArgs e)
    {
        if (_conn == null || !_selectedRoomId.HasValue) return;

        if (!string.IsNullOrEmpty(e.NewTextValue))
        {
            _conn.Reducers.SetTyping(_selectedRoomId.Value);
        }

        _conn.Reducers.SaveDraft(_selectedRoomId.Value, e.NewTextValue ?? "");
    }

    // ========================================================================
    // TYPING INDICATORS
    // ========================================================================

    private void RefreshTypingIndicators()
    {
        if (_conn == null || !_selectedRoomId.HasValue || _myIdentity == null) return;

        var typing = _conn.Db.TypingIndicator.Iter()
            .Where(t => t.RoomId == _selectedRoomId && t.UserId != _myIdentity)
            .ToList();

        if (!typing.Any())
        {
            TypingPanel.IsVisible = false;
            return;
        }

        var names = typing
            .Select(t => _conn.Db.User.Identity.Find(t.UserId)?.Username ?? "Someone")
            .ToList();

        TypingText.Text = names.Count == 1
            ? $"{names[0]} is typing..."
            : names.Count <= 3
                ? $"{string.Join(", ", names)} are typing..."
                : "Multiple users are typing...";

        TypingPanel.IsVisible = true;
    }

    // ========================================================================
    // SCHEDULED MESSAGES
    // ========================================================================

    private void OnScheduleMessage(object sender, EventArgs e)
    {
        var text = MessageInput.Text?.Trim();
        if (string.IsNullOrEmpty(text))
        {
            DisplayAlert("Error", "Enter a message first", "OK");
            return;
        }
        ScheduleMinutes.Text = "";
        ScheduleDialog.IsVisible = true;
    }

    private void OnCancelScheduleDialog(object sender, EventArgs e)
    {
        ScheduleDialog.IsVisible = false;
    }

    private void OnConfirmSchedule(object sender, EventArgs e)
    {
        if (!int.TryParse(ScheduleMinutes.Text, out var minutes) || minutes < 1)
        {
            DisplayAlert("Error", "Enter a valid number of minutes", "OK");
            return;
        }

        _scheduleAtMicros = DateTimeOffset.UtcNow.AddMinutes(minutes).ToUnixTimeMilliseconds() * 1000;
        SchedulePreview.IsVisible = true;
        ScheduleText.Text = $"📅 Scheduled for {minutes} minutes from now";
        ScheduleDialog.IsVisible = false;
    }

    private void OnCancelSchedule(object sender, EventArgs e)
    {
        _scheduleAtMicros = null;
        SchedulePreview.IsVisible = false;
    }

    private void RefreshScheduledMessages()
    {
        if (_conn == null || !_selectedRoomId.HasValue || _myIdentity == null) return;

        ScheduledMessages.Clear();

        foreach (var sm in _conn.Db.ScheduledMessage.Iter()
            .Where(s => s.RoomId == _selectedRoomId && s.SenderId == _myIdentity))
        {
            ScheduledMessages.Add(new ScheduledMessageViewModel
            {
                Id = sm.ScheduledId,
                Text = Truncate(sm.Text, 50),
                ScheduledTime = "Scheduled"
            });
        }
    }

    private void OnCancelScheduledMessage(object sender, EventArgs e)
    {
        if (sender is Button btn && btn.CommandParameter is ulong id)
        {
            try
            {
                _conn?.Reducers.CancelScheduledMessage(id);
            }
            catch (Exception ex)
            {
                DisplayAlert("Error", ex.Message, "OK");
            }
        }
    }

    // ========================================================================
    // EPHEMERAL MESSAGES
    // ========================================================================

    private void OnSendEphemeral(object sender, EventArgs e)
    {
        var text = MessageInput.Text?.Trim();
        if (string.IsNullOrEmpty(text))
        {
            DisplayAlert("Error", "Enter a message first", "OK");
            return;
        }
        EphemeralPicker.SelectedIndex = 1;
        EphemeralDialog.IsVisible = true;
    }

    private void OnCancelEphemeral(object sender, EventArgs e)
    {
        EphemeralDialog.IsVisible = false;
    }

    private void OnConfirmEphemeral(object sender, EventArgs e)
    {
        if (_conn == null || !_selectedRoomId.HasValue) return;

        var text = MessageInput.Text?.Trim();
        if (string.IsNullOrEmpty(text)) return;

        var minutes = EphemeralPicker.SelectedIndex switch
        {
            0 => 1UL,
            1 => 5UL,
            2 => 15UL,
            3 => 30UL,
            4 => 60UL,
            _ => 5UL
        };

        try
        {
            _conn.Reducers.SendEphemeralMessage(_selectedRoomId.Value, text, minutes);
            MessageInput.Text = "";
            EphemeralDialog.IsVisible = false;
        }
        catch (Exception ex)
        {
            DisplayAlert("Error", ex.Message, "OK");
        }
    }

    // ========================================================================
    // THREADS
    // ========================================================================

    private void ShowThread(ulong parentId)
    {
        if (_conn == null) return;

        _threadParentId = parentId;
        ThreadMessages.Children.Clear();

        var parent = _conn.Db.Message.Id.Find(parentId);
        if (parent != null)
        {
            ThreadMessages.Children.Add(CreateMessageView(parent));
        }

        ThreadMessages.Children.Add(new BoxView
        {
            HeightRequest = 1,
            Color = (Color)Application.Current!.Resources["BorderColor"],
            Margin = new Thickness(0, 8)
        });

        var replies = _conn.Db.Message.Iter()
            .Where(m => m.ParentMessageId == parentId)
            .OrderBy(m => m.CreatedAt.MicrosecondsSinceUnixEpoch)
            .ToList();

        foreach (var reply in replies)
        {
            ThreadMessages.Children.Add(CreateMessageView(reply));
        }

        ThreadDialog.IsVisible = true;
    }

    private void OnCloseThread(object sender, EventArgs e)
    {
        _threadParentId = null;
        ThreadDialog.IsVisible = false;
    }

    private void OnSendThreadReply(object sender, EventArgs e)
    {
        if (_conn == null || !_selectedRoomId.HasValue || !_threadParentId.HasValue) return;

        var text = ThreadReplyInput.Text?.Trim();
        if (string.IsNullOrEmpty(text)) return;

        try
        {
            _conn.Reducers.SendMessage(_selectedRoomId.Value, text, _threadParentId.Value);
            ThreadReplyInput.Text = "";
            ShowThread(_threadParentId.Value);
        }
        catch (Exception ex)
        {
            DisplayAlert("Error", ex.Message, "OK");
        }
    }

    // ========================================================================
    // EDIT HISTORY
    // ========================================================================

    private void ShowEditHistory(ulong messageId)
    {
        if (_conn == null) return;

        EditHistory.Clear();

        var edits = _conn.Db.MessageEdit.Iter()
            .Where(edit => edit.MessageId == messageId)
            .OrderByDescending(edit => edit.EditedAt.MicrosecondsSinceUnixEpoch)
            .ToList();

        foreach (var edit in edits)
        {
            EditHistory.Add(new EditHistoryViewModel
            {
                Text = edit.OldText,
                EditedAt = FormatTime(edit.EditedAt)
            });
        }

        EditHistoryDialog.IsVisible = true;
    }

    private void OnCloseEditHistory(object sender, EventArgs e)
    {
        EditHistoryDialog.IsVisible = false;
    }

    // ========================================================================
    // INVITES
    // ========================================================================

    private void RefreshInvites()
    {
        if (_conn == null || _myIdentity == null) return;

        Invites.Clear();

        var pending = _conn.Db.RoomInvite.Iter()
            .Where(i => i.InviteeId == _myIdentity && i.Status == "pending")
            .ToList();

        foreach (var invite in pending)
        {
            var room = _conn.Db.Room.Id.Find(invite.RoomId);
            Invites.Add(new InviteViewModel
            {
                Id = invite.Id,
                RoomName = room?.Name ?? "Unknown Room"
            });
        }

        InvitesSection.IsVisible = Invites.Any();
    }

    private void OnAcceptInvite(object sender, EventArgs e)
    {
        if (sender is Button btn && btn.CommandParameter is ulong id)
        {
            try
            {
                _conn?.Reducers.AcceptInvite(id);
            }
            catch (Exception ex)
            {
                DisplayAlert("Error", ex.Message, "OK");
            }
        }
    }

    private void OnDeclineInvite(object sender, EventArgs e)
    {
        if (sender is Button btn && btn.CommandParameter is ulong id)
        {
            try
            {
                _conn?.Reducers.DeclineInvite(id);
            }
            catch (Exception ex)
            {
                DisplayAlert("Error", ex.Message, "OK");
            }
        }
    }

    // ========================================================================
    // ROOM SETTINGS / MEMBERS
    // ========================================================================

    private void OnRoomSettings(object sender, EventArgs e)
    {
        RefreshMembers();
        RefreshScheduledMessages();

        var room = _conn?.Db.Room.Id.Find(_selectedRoomId ?? 0);
        InviteUserSection.IsVisible = room?.IsPrivate == true;

        RoomSettingsDialog.IsVisible = true;
    }

    private void OnCloseRoomSettings(object sender, EventArgs e)
    {
        RoomSettingsDialog.IsVisible = false;
    }

    private void RefreshMembers()
    {
        if (_conn == null || !_selectedRoomId.HasValue || _myIdentity == null) return;

        Members.Clear();

        var room = _conn.Db.Room.Id.Find(_selectedRoomId.Value);
        var isAdmin = false;

        foreach (var member in _conn.Db.RoomMember.Iter()
            .Where(m => m.RoomId == _selectedRoomId && !m.IsKicked && !m.IsBanned))
        {
            var user = _conn.Db.User.Identity.Find(member.UserId);
            if (user == null) continue;

            var isSelf = member.UserId == _myIdentity;
            if (isSelf && member.Role == MemberRole.Admin) isAdmin = true;

            Members.Add(new MemberViewModel
            {
                Username = user.Username ?? "Anonymous",
                RoleText = member.Role == MemberRole.Admin ? "Admin" : "Member",
                StatusColor = GetStatusColor(user.Status),
                CanPromote = isAdmin && !isSelf && member.Role != MemberRole.Admin,
                CanKick = isAdmin && !isSelf && member.UserId != room?.OwnerId
            });
        }
    }

    private void OnInviteUser(object sender, EventArgs e)
    {
        if (_conn == null || !_selectedRoomId.HasValue) return;

        var username = InviteUsername.Text?.Trim();
        if (string.IsNullOrEmpty(username)) return;

        try
        {
            _conn.Reducers.InviteToRoom(_selectedRoomId.Value, username);
            InviteUsername.Text = "";
            DisplayAlert("Success", "Invite sent!", "OK");
        }
        catch (Exception ex)
        {
            DisplayAlert("Error", ex.Message, "OK");
        }
    }

    private void OnPromoteMember(object sender, EventArgs e)
    {
        if (sender is Button btn && btn.CommandParameter is string username && _selectedRoomId.HasValue)
        {
            try
            {
                _conn?.Reducers.PromoteToAdmin(_selectedRoomId.Value, username);
            }
            catch (Exception ex)
            {
                DisplayAlert("Error", ex.Message, "OK");
            }
        }
    }

    private void OnKickMember(object sender, EventArgs e)
    {
        if (sender is Button btn && btn.CommandParameter is string username && _selectedRoomId.HasValue)
        {
            try
            {
                _conn?.Reducers.KickUser(_selectedRoomId.Value, username);
            }
            catch (Exception ex)
            {
                DisplayAlert("Error", ex.Message, "OK");
            }
        }
    }

    private void OnBanMember(object sender, EventArgs e)
    {
        if (sender is Button btn && btn.CommandParameter is string username && _selectedRoomId.HasValue)
        {
            try
            {
                _conn?.Reducers.BanUser(_selectedRoomId.Value, username);
            }
            catch (Exception ex)
            {
                DisplayAlert("Error", ex.Message, "OK");
            }
        }
    }

    // ========================================================================
    // DIRECT MESSAGES
    // ========================================================================

    private void OnStartDm(object sender, EventArgs e)
    {
        DmUsername.Text = "";
        DmDialog.IsVisible = true;
    }

    private void OnCancelDm(object sender, EventArgs e)
    {
        DmDialog.IsVisible = false;
    }

    private void OnConfirmDm(object sender, EventArgs e)
    {
        var username = DmUsername.Text?.Trim();
        if (string.IsNullOrEmpty(username)) return;

        try
        {
            _conn?.Reducers.StartDm(username);
            DmDialog.IsVisible = false;
        }
        catch (Exception ex)
        {
            DisplayAlert("Error", ex.Message, "OK");
        }
    }

    // ========================================================================
    // DRAFTS
    // ========================================================================

    private void LoadDraft()
    {
        if (_conn == null || !_selectedRoomId.HasValue || _myIdentity == null) return;

        var draft = _conn.Db.Draft.Iter()
            .FirstOrDefault(d => d.RoomId == _selectedRoomId && d.UserId == _myIdentity);

        if (draft != null && !string.IsNullOrEmpty(draft.Text))
        {
            MessageInput.Text = draft.Text;
        }
    }

    private void OnDraftChanged(Draft draft)
    {
        if (draft.RoomId == _selectedRoomId && draft.UserId != _myIdentity)
        {
            MessageInput.Text = draft.Text;
        }
    }

    // ========================================================================
    // HELPERS
    // ========================================================================

    private static string GetStatusText(UserStatus status) => status switch
    {
        UserStatus.Online => "Online",
        UserStatus.Away => "Away",
        UserStatus.DoNotDisturb => "Do Not Disturb",
        UserStatus.Invisible => "Invisible",
        _ => "Unknown"
    };

    private static Color GetStatusColor(UserStatus status) => status switch
    {
        UserStatus.Online => Color.FromArgb("#4cf490"),
        UserStatus.Away => Color.FromArgb("#fbdc8e"),
        UserStatus.DoNotDisturb => Color.FromArgb("#ff4c4c"),
        UserStatus.Invisible => Color.FromArgb("#6f7987"),
        _ => Color.FromArgb("#6f7987")
    };

    private static string FormatTime(Timestamp ts)
    {
        var dt = DateTimeOffset.FromUnixTimeMilliseconds(ts.MicrosecondsSinceUnixEpoch / 1000).LocalDateTime;
        var now = DateTime.Now;
        if (dt.Date == now.Date)
            return dt.ToString("h:mm tt");
        if ((now - dt).TotalDays < 7)
            return dt.ToString("ddd h:mm tt");
        return dt.ToString("MMM d, h:mm tt");
    }

    private static string Truncate(string text, int maxLength) =>
        text.Length <= maxLength ? text : text[..(maxLength - 3)] + "...";
}

// ========================================================================
// VIEW MODELS
// ========================================================================

public class RoomViewModel
{
    public ulong Id { get; set; }
    public string Name { get; set; } = "";
    public string Icon { get; set; } = "#️⃣";
    public int UnreadCount { get; set; }
    public bool HasUnread { get; set; }
    public string ActivityText { get; set; } = "";
    public bool IsPrivate { get; set; }
    public bool IsDm { get; set; }
}

public class InviteViewModel
{
    public ulong Id { get; set; }
    public string RoomName { get; set; } = "";
}

public class MemberViewModel
{
    public string Username { get; set; } = "";
    public string RoleText { get; set; } = "";
    public Color StatusColor { get; set; } = Colors.Gray;
    public bool CanPromote { get; set; }
    public bool CanKick { get; set; }
}

public class ScheduledMessageViewModel
{
    public ulong Id { get; set; }
    public string Text { get; set; } = "";
    public string ScheduledTime { get; set; } = "";
}

public class EditHistoryViewModel
{
    public string Text { get; set; } = "";
    public string EditedAt { get; set; } = "";
}

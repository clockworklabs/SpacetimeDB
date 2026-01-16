using ChatClient.Services;
using SpacetimeDB;
using SpacetimeDB.Types;

namespace ChatClient;

public partial class MainPage : ContentPage
{
    private readonly SpacetimeService _spacetime;
    private IDispatcherTimer? _timer;
    private IDispatcherTimer? _typingTimer;
    
    private ulong? _currentRoomId;
    private ulong? _editingMessageId;
    private ulong? _reactionMessageId;
    private bool _isEphemeralMode = false;
    private bool _isScheduleMode = false;
    private DateTime _lastTypingSent = DateTime.MinValue;

    // Ephemeral duration options in milliseconds
    private readonly ulong[] _ephemeralDurations = { 30000, 60000, 300000, 600000 };

    public MainPage()
    {
        InitializeComponent();
        
        _spacetime = new SpacetimeService();
        _spacetime.OnConnected += OnSpacetimeConnected;
        _spacetime.OnDisconnected += OnSpacetimeDisconnected;
        _spacetime.OnError += OnSpacetimeError;
        _spacetime.OnSubscriptionApplied += OnSubscriptionApplied;
        _spacetime.OnDataChanged += OnDataChanged;
        
        // Setup FrameTick timer
        _timer = Dispatcher.CreateTimer();
        _timer.Interval = TimeSpan.FromMilliseconds(16);
        _timer.Tick += (s, e) => _spacetime.FrameTick();
        _timer.Start();
        
        // Setup typing indicator expiry checker
        _typingTimer = Dispatcher.CreateTimer();
        _typingTimer.Interval = TimeSpan.FromSeconds(1);
        _typingTimer.Tick += (s, e) => RefreshTypingIndicators();
        _typingTimer.Start();
        
        // Initialize date/time pickers
        ScheduleDatePicker.Date = DateTime.Today;
        ScheduleTimePicker.Time = DateTime.Now.TimeOfDay.Add(TimeSpan.FromMinutes(5));
        PickerEphemeralDuration.SelectedIndex = 1; // Default to 1 minute
        
        // Connect to SpacetimeDB
        _spacetime.Connect();
    }

    // ========================================================================
    // SPACETIME CALLBACKS
    // ========================================================================

    private void OnSpacetimeConnected()
    {
        MainThread.BeginInvokeOnMainThread(() =>
        {
            LblStatus.Text = "●  Online";
            LblStatus.TextColor = Color.FromArgb("#22c55e");
        });
    }

    private void OnSpacetimeDisconnected()
    {
        MainThread.BeginInvokeOnMainThread(() =>
        {
            LblStatus.Text = "●  Offline";
            LblStatus.TextColor = Color.FromArgb("#71717a");
            LblDisplayName.Text = "Disconnected";
        });
    }

    private void OnSpacetimeError(string error)
    {
        MainThread.BeginInvokeOnMainThread(async () =>
        {
            await DisplayAlert("Error", error, "OK");
        });
    }

    private void OnSubscriptionApplied()
    {
        MainThread.BeginInvokeOnMainThread(() =>
        {
            RefreshAll();
        });
    }

    private void OnDataChanged()
    {
        MainThread.BeginInvokeOnMainThread(() =>
        {
            RefreshAll();
        });
    }

    // ========================================================================
    // REFRESH METHODS
    // ========================================================================

    private void RefreshAll()
    {
        RefreshUserProfile();
        RefreshRoomsList();
        RefreshOnlineUsers();
        RefreshMessages();
        RefreshTypingIndicators();
        RefreshScheduledMessages();
    }

    private void RefreshUserProfile()
    {
        var conn = _spacetime.Connection;
        var identity = _spacetime.Identity;
        if (conn == null || identity == null) return;

        var user = conn.Db.User.Identity.Find(identity.Value);
        if (user != null)
        {
            LblDisplayName.Text = user.DisplayName;
        }
    }

    private void RefreshRoomsList()
    {
        var conn = _spacetime.Connection;
        var identity = _spacetime.Identity;
        if (conn == null || identity == null) return;

        RoomsList.Children.Clear();

        // Get all rooms
        var allRooms = conn.Db.Room.Iter().ToList();
        var myMemberships = conn.Db.RoomMember.Iter()
            .Where(m => m.UserId == identity.Value)
            .ToDictionary(m => m.RoomId);

        foreach (var room in allRooms.OrderBy(r => r.Name))
        {
            var isMember = myMemberships.ContainsKey(room.Id);
            var membership = isMember ? myMemberships[room.Id] : default;

            // Count unread messages
            var unreadCount = 0;
            if (isMember)
            {
                var lastReadAt = membership.LastReadAt.MicrosecondsSinceUnixEpoch;
                unreadCount = conn.Db.Message.RoomId.Filter(room.Id)
                    .Count(m => m.CreatedAt.MicrosecondsSinceUnixEpoch > lastReadAt && m.SenderId != identity.Value);
            }

            var roomButton = CreateRoomButton(room, isMember, unreadCount);
            RoomsList.Children.Add(roomButton);
        }
    }

    private Border CreateRoomButton(Room room, bool isMember, int unreadCount)
    {
        var isSelected = _currentRoomId == room.Id;
        var bgColor = isSelected ? Color.FromArgb("#6366f1") : 
                      isMember ? Color.FromArgb("#1a1a25") : 
                      Color.FromArgb("#12121a");

        var grid = new Grid
        {
            ColumnDefinitions = new ColumnDefinitionCollection
            {
                new ColumnDefinition(GridLength.Star),
                new ColumnDefinition(GridLength.Auto)
            },
            Padding = new Thickness(12, 10)
        };

        var nameLabel = new Label
        {
            Text = $"# {room.Name}",
            TextColor = isMember ? Color.FromArgb("#ffffff") : Color.FromArgb("#71717a"),
            FontSize = 14,
            FontAttributes = isSelected ? FontAttributes.Bold : FontAttributes.None
        };
        grid.Add(nameLabel, 0, 0);

        // Unread badge
        if (unreadCount > 0)
        {
            var badge = new Border
            {
                BackgroundColor = Color.FromArgb("#22d3ee"),
                StrokeThickness = 0,
                WidthRequest = 24,
                HeightRequest = 24,
                Padding = 0,
                Content = new Label
                {
                    Text = unreadCount > 99 ? "99+" : unreadCount.ToString(),
                    TextColor = Color.FromArgb("#0a0a0f"),
                    FontSize = 10,
                    FontAttributes = FontAttributes.Bold,
                    HorizontalOptions = LayoutOptions.Center,
                    VerticalOptions = LayoutOptions.Center
                }
            };
            // Make it circular
            badge.StrokeShape = new Microsoft.Maui.Controls.Shapes.RoundRectangle { CornerRadius = 12 };
            grid.Add(badge, 1, 0);
        }
        else if (!isMember)
        {
            var joinBtn = new Button
            {
                Text = "Join",
                BackgroundColor = Color.FromArgb("#6366f1"),
                TextColor = Colors.White,
                FontSize = 10,
                HeightRequest = 24,
                Padding = new Thickness(8, 0),
                CornerRadius = 4
            };
            var roomId = room.Id;
            joinBtn.Clicked += (s, e) => _spacetime.JoinRoom(roomId);
            grid.Add(joinBtn, 1, 0);
        }

        var border = new Border
        {
            BackgroundColor = bgColor,
            StrokeThickness = 0,
            Content = grid
        };
        border.StrokeShape = new Microsoft.Maui.Controls.Shapes.RoundRectangle { CornerRadius = 8 };

        if (isMember)
        {
            var tapGesture = new TapGestureRecognizer();
            var roomId = room.Id;
            tapGesture.Tapped += (s, e) => SelectRoom(roomId);
            border.GestureRecognizers.Add(tapGesture);
        }

        return border;
    }

    private void RefreshOnlineUsers()
    {
        var conn = _spacetime.Connection;
        if (conn == null) return;

        OnlineUsersList.Children.Clear();

        var onlineUsers = conn.Db.User.Iter()
            .Where(u => u.Online)
            .OrderBy(u => u.DisplayName)
            .ToList();

        foreach (var user in onlineUsers)
        {
            var userLabel = new Label
            {
                Text = $"●  {user.DisplayName}",
                TextColor = Color.FromArgb("#22c55e"),
                FontSize = 12
            };
            OnlineUsersList.Children.Add(userLabel);
        }

        if (!onlineUsers.Any())
        {
            OnlineUsersList.Children.Add(new Label
            {
                Text = "No users online",
                TextColor = Color.FromArgb("#71717a"),
                FontSize = 12,
                FontAttributes = FontAttributes.Italic
            });
        }
    }

    private void RefreshMessages()
    {
        var conn = _spacetime.Connection;
        var identity = _spacetime.Identity;
        if (conn == null || identity == null || _currentRoomId == null)
        {
            LblEmptyState.IsVisible = _currentRoomId == null;
            return;
        }

        LblEmptyState.IsVisible = false;
        MessagesList.Children.Clear();
        MessagesList.Children.Add(LblEmptyState);
        LblEmptyState.IsVisible = false;

        var messages = conn.Db.Message.RoomId.Filter(_currentRoomId.Value)
            .OrderBy(m => m.CreatedAt.MicrosecondsSinceUnixEpoch)
            .ToList();

        if (!messages.Any())
        {
            var emptyLabel = new Label
            {
                Text = "No messages yet. Start the conversation!",
                TextColor = Color.FromArgb("#71717a"),
                FontSize = 14,
                HorizontalOptions = LayoutOptions.Center,
                Margin = new Thickness(0, 50, 0, 0)
            };
            MessagesList.Children.Add(emptyLabel);
            return;
        }

        foreach (var message in messages)
        {
            var messageView = CreateMessageView(message);
            MessagesList.Children.Add(messageView);
        }

        // Scroll to bottom
        MainThread.BeginInvokeOnMainThread(async () =>
        {
            await Task.Delay(50);
            await MessagesScrollView.ScrollToAsync(0, MessagesList.Height, false);
        });
    }

    private View CreateMessageView(Message message)
    {
        var conn = _spacetime.Connection;
        var identity = _spacetime.Identity;
        if (conn == null || identity == null) return new Label();

        var sender = conn.Db.User.Identity.Find(message.SenderId);
        var senderName = sender?.DisplayName ?? "Unknown";
        var isOwn = message.SenderId == identity.Value;
        var timestamp = DateTimeOffset.FromUnixTimeMilliseconds(
            message.CreatedAt.MicrosecondsSinceUnixEpoch / 1000).LocalDateTime;

        // Get reactions for this message
        var reactions = conn.Db.Reaction.MessageId.Filter(message.Id).ToList();
        var reactionGroups = reactions.GroupBy(r => r.Emoji)
            .Select(g => new { Emoji = g.Key, Count = g.Count(), Users = g.Select(r => r.UserId).ToList() })
            .ToList();

        // Get read receipts
        var readReceipts = conn.Db.MessageRead.MessageId.Filter(message.Id)
            .Where(r => r.UserId != message.SenderId)
            .ToList();
        var readByUsers = readReceipts
            .Select(r => conn.Db.User.Identity.Find(r.UserId)?.DisplayName ?? "Unknown")
            .Take(3)
            .ToList();

        var outerGrid = new Grid
        {
            ColumnDefinitions = isOwn 
                ? new ColumnDefinitionCollection { new ColumnDefinition(GridLength.Star), new ColumnDefinition(GridLength.Auto) }
                : new ColumnDefinitionCollection { new ColumnDefinition(GridLength.Auto), new ColumnDefinition(GridLength.Star) }
        };

        var messageContainer = new VerticalStackLayout { Spacing = 4, MaximumWidthRequest = 500 };

        // Header with name and time
        var headerStack = new HorizontalStackLayout { Spacing = 8 };
        headerStack.Children.Add(new Label
        {
            Text = senderName,
            TextColor = isOwn ? Color.FromArgb("#6366f1") : Color.FromArgb("#22d3ee"),
            FontSize = 12,
            FontAttributes = FontAttributes.Bold
        });
        headerStack.Children.Add(new Label
        {
            Text = timestamp.ToString("HH:mm"),
            TextColor = Color.FromArgb("#71717a"),
            FontSize = 10
        });
        if (message.IsEdited)
        {
            headerStack.Children.Add(new Label
            {
                Text = "(edited)",
                TextColor = Color.FromArgb("#71717a"),
                FontSize = 10,
                FontAttributes = FontAttributes.Italic
            });
        }
        if (message.IsEphemeral && message.ExpiresAt.HasValue)
        {
            var remaining = TimeSpan.FromMilliseconds(
                (message.ExpiresAt.Value.MicrosecondsSinceUnixEpoch - DateTimeOffset.UtcNow.ToUnixTimeMilliseconds() * 1000) / 1000);
            var expiryText = remaining.TotalSeconds > 0 ? $"💨 {(int)remaining.TotalSeconds}s" : "💨 expiring...";
            headerStack.Children.Add(new Label
            {
                Text = expiryText,
                TextColor = Color.FromArgb("#f59e0b"),
                FontSize = 10
            });
        }
        messageContainer.Children.Add(headerStack);

        // Message content bubble
        var bubble = new Border
        {
            BackgroundColor = isOwn ? Color.FromArgb("#4f46e5") : Color.FromArgb("#1a1a25"),
            Padding = new Thickness(12, 8),
            Content = new Label
            {
                Text = message.Content,
                TextColor = Colors.White,
                FontSize = 14
            }
        };
        bubble.StrokeShape = new Microsoft.Maui.Controls.Shapes.RoundRectangle 
        { 
            CornerRadius = new CornerRadius(isOwn ? 16 : 4, isOwn ? 4 : 16, 16, 16) 
        };
        bubble.StrokeThickness = 0;
        messageContainer.Children.Add(bubble);

        // Reactions display
        if (reactionGroups.Any())
        {
            var reactionsStack = new HorizontalStackLayout { Spacing = 4 };
            foreach (var group in reactionGroups)
            {
                var hasMyReaction = group.Users.Contains(identity.Value);
                var reactionBorder = new Border
                {
                    BackgroundColor = hasMyReaction ? Color.FromArgb("#6366f1") : Color.FromArgb("#27272a"),
                    Padding = new Thickness(6, 2),
                    Content = new Label
                    {
                        Text = $"{group.Emoji} {group.Count}",
                        TextColor = Colors.White,
                        FontSize = 12
                    }
                };
                reactionBorder.StrokeShape = new Microsoft.Maui.Controls.Shapes.RoundRectangle { CornerRadius = 10 };
                reactionBorder.StrokeThickness = 0;

                var tapGesture = new TapGestureRecognizer();
                var emoji = group.Emoji;
                var msgId = message.Id;
                tapGesture.Tapped += (s, e) => _spacetime.ToggleReaction(msgId, emoji);
                reactionBorder.GestureRecognizers.Add(tapGesture);
                
                reactionsStack.Children.Add(reactionBorder);
            }
            messageContainer.Children.Add(reactionsStack);
        }

        // Read receipts
        if (isOwn && readByUsers.Any())
        {
            var seenText = readByUsers.Count switch
            {
                1 => $"Seen by {readByUsers[0]}",
                2 => $"Seen by {readByUsers[0]}, {readByUsers[1]}",
                _ => $"Seen by {readByUsers[0]}, {readByUsers[1]} +{readReceipts.Count - 2}"
            };
            messageContainer.Children.Add(new Label
            {
                Text = seenText,
                TextColor = Color.FromArgb("#71717a"),
                FontSize = 10,
                HorizontalOptions = LayoutOptions.End
            });
        }

        // Action buttons (for own messages: edit, react; for others: react)
        var actionsStack = new HorizontalStackLayout { Spacing = 4, Margin = new Thickness(0, 4, 0, 0) };
        
        var reactBtn = new Button
        {
            Text = "😊",
            BackgroundColor = Colors.Transparent,
            TextColor = Color.FromArgb("#71717a"),
            FontSize = 14,
            HeightRequest = 28,
            WidthRequest = 36,
            Padding = 0
        };
        var messageId = message.Id;
        reactBtn.Clicked += (s, e) => ShowReactionPicker(messageId);
        actionsStack.Children.Add(reactBtn);

        if (isOwn && !message.IsEphemeral)
        {
            var editBtn = new Button
            {
                Text = "✏️",
                BackgroundColor = Colors.Transparent,
                TextColor = Color.FromArgb("#71717a"),
                FontSize = 14,
                HeightRequest = 28,
                WidthRequest = 36,
                Padding = 0
            };
            var msgContent = message.Content;
            editBtn.Clicked += (s, e) => ShowEditDialog(messageId, msgContent);
            actionsStack.Children.Add(editBtn);
        }

        // View edit history button
        var editCount = conn.Db.MessageEdit.MessageId.Filter(message.Id).Count();
        if (message.IsEdited && editCount > 0)
        {
            var historyBtn = new Button
            {
                Text = $"📜 {editCount}",
                BackgroundColor = Colors.Transparent,
                TextColor = Color.FromArgb("#71717a"),
                FontSize = 12,
                HeightRequest = 28,
                Padding = 0
            };
            historyBtn.Clicked += (s, e) => ShowEditHistory(messageId, message.Content);
            actionsStack.Children.Add(historyBtn);
        }

        messageContainer.Children.Add(actionsStack);

        outerGrid.Add(messageContainer, isOwn ? 1 : 0, 0);
        return outerGrid;
    }

    private void RefreshTypingIndicators()
    {
        var conn = _spacetime.Connection;
        var identity = _spacetime.Identity;
        if (conn == null || identity == null || _currentRoomId == null)
        {
            TypingIndicatorArea.IsVisible = false;
            return;
        }

        var currentMicros = DateTimeOffset.UtcNow.ToUnixTimeMilliseconds() * 1000;
        var typingUsers = conn.Db.TypingIndicator.RoomId.Filter(_currentRoomId.Value)
            .Where(t => t.UserId != identity.Value && t.ExpiresAt.MicrosecondsSinceUnixEpoch > currentMicros)
            .Select(t => conn.Db.User.Identity.Find(t.UserId)?.DisplayName ?? "Someone")
            .ToList();

        if (typingUsers.Any())
        {
            TypingIndicatorArea.IsVisible = true;
            LblTypingIndicator.Text = typingUsers.Count switch
            {
                1 => $"{typingUsers[0]} is typing...",
                2 => $"{typingUsers[0]} and {typingUsers[1]} are typing...",
                _ => "Multiple users are typing..."
            };
        }
        else
        {
            TypingIndicatorArea.IsVisible = false;
        }
    }

    private void RefreshScheduledMessages()
    {
        var conn = _spacetime.Connection;
        var identity = _spacetime.Identity;
        if (conn == null || identity == null || _currentRoomId == null) return;

        ScheduledMessagesList.Children.Clear();

        var scheduled = conn.Db.ScheduledMessage.RoomId.Filter(_currentRoomId.Value)
            .Where(s => s.SenderId == identity.Value)
            .OrderBy(s => s.ScheduledAt)
            .ToList();

        foreach (var msg in scheduled)
        {
            // Get scheduled time from CreatedAt for display (the actual trigger is handled by backend)
            var scheduleTime = DateTimeOffset.FromUnixTimeMilliseconds(
                msg.CreatedAt.MicrosecondsSinceUnixEpoch / 1000).LocalDateTime.AddMinutes(1);

            var item = new Border
            {
                BackgroundColor = Color.FromArgb("#1a1a25"),
                Padding = new Thickness(12),
                Margin = new Thickness(0, 4)
            };
            item.StrokeShape = new Microsoft.Maui.Controls.Shapes.RoundRectangle { CornerRadius = 8 };
            item.StrokeThickness = 0;

            var grid = new Grid
            {
                ColumnDefinitions = new ColumnDefinitionCollection
                {
                    new ColumnDefinition(GridLength.Star),
                    new ColumnDefinition(GridLength.Auto)
                },
                RowDefinitions = new RowDefinitionCollection
                {
                    new RowDefinition(GridLength.Auto),
                    new RowDefinition(GridLength.Auto)
                }
            };

            grid.Add(new Label
            {
                Text = msg.Content.Length > 50 ? msg.Content.Substring(0, 50) + "..." : msg.Content,
                TextColor = Colors.White,
                FontSize = 14
            }, 0, 0);

            grid.Add(new Label
            {
                Text = $"⏰ {scheduleTime:MMM dd, HH:mm}",
                TextColor = Color.FromArgb("#22d3ee"),
                FontSize = 12
            }, 0, 1);

            var cancelBtn = new Button
            {
                Text = "Cancel",
                BackgroundColor = Color.FromArgb("#ef4444"),
                TextColor = Colors.White,
                FontSize = 12,
                HeightRequest = 30,
                CornerRadius = 4
            };
            var schedId = msg.ScheduledId;
            cancelBtn.Clicked += (s, e) => _spacetime.CancelScheduledMessage(schedId);
            grid.Add(cancelBtn, 1, 0);
            Grid.SetRowSpan(cancelBtn, 2);

            item.Content = grid;
            ScheduledMessagesList.Children.Add(item);
        }

        if (!scheduled.Any())
        {
            ScheduledMessagesList.Children.Add(new Label
            {
                Text = "No scheduled messages",
                TextColor = Color.FromArgb("#71717a"),
                FontSize = 14,
                FontAttributes = FontAttributes.Italic
            });
        }
    }

    // ========================================================================
    // ROOM SELECTION
    // ========================================================================

    private void SelectRoom(ulong roomId)
    {
        _currentRoomId = roomId;
        
        var conn = _spacetime.Connection;
        if (conn != null)
        {
            var room = conn.Db.Room.Id.Find(roomId);
            if (room != null)
            {
                LblRoomName.Text = $"# {room.Name}";
                
                var memberCount = conn.Db.RoomMember.RoomId.Filter(roomId).Count();
                LblRoomMembers.Text = $"{memberCount} member{(memberCount != 1 ? "s" : "")}";
            }
        }

        BtnLeaveRoom.IsVisible = true;
        BtnScheduleMessage.IsVisible = true;
        
        // Mark room as read
        _spacetime.MarkRoomRead(roomId);
        
        RefreshAll();
    }

    // ========================================================================
    // EVENT HANDLERS
    // ========================================================================

    private void OnEditDisplayName(object? sender, EventArgs e)
    {
        DisplayNameEditor.IsVisible = !DisplayNameEditor.IsVisible;
        if (DisplayNameEditor.IsVisible)
        {
            EntryDisplayName.Text = LblDisplayName.Text;
            EntryDisplayName.Focus();
        }
    }

    private void OnSaveDisplayName(object? sender, EventArgs e)
    {
        var name = EntryDisplayName.Text?.Trim();
        if (!string.IsNullOrEmpty(name))
        {
            _spacetime.SetDisplayName(name);
        }
        DisplayNameEditor.IsVisible = false;
    }

    private void OnCreateRoomToggle(object? sender, EventArgs e)
    {
        NewRoomEditor.IsVisible = !NewRoomEditor.IsVisible;
        if (NewRoomEditor.IsVisible)
        {
            EntryNewRoom.Text = "";
            EntryNewRoom.Focus();
        }
    }

    private void OnCreateRoom(object? sender, EventArgs e)
    {
        var name = EntryNewRoom.Text?.Trim();
        if (!string.IsNullOrEmpty(name))
        {
            _spacetime.CreateRoom(name);
            EntryNewRoom.Text = "";
            NewRoomEditor.IsVisible = false;
        }
    }

    private void OnLeaveRoom(object? sender, EventArgs e)
    {
        if (_currentRoomId != null)
        {
            _spacetime.LeaveRoom(_currentRoomId.Value);
            _currentRoomId = null;
            LblRoomName.Text = "Select a room";
            LblRoomMembers.Text = "";
            BtnLeaveRoom.IsVisible = false;
            BtnScheduleMessage.IsVisible = false;
            LblEmptyState.IsVisible = true;
            MessagesList.Children.Clear();
            MessagesList.Children.Add(LblEmptyState);
        }
    }

    private void OnMessageTextChanged(object? sender, TextChangedEventArgs e)
    {
        if (_currentRoomId == null) return;

        // Send typing indicator (debounced)
        var now = DateTime.Now;
        if ((now - _lastTypingSent).TotalMilliseconds > 2000)
        {
            _spacetime.SetTyping(_currentRoomId.Value);
            _lastTypingSent = now;
        }
    }

    private void OnSendMessage(object? sender, EventArgs e)
    {
        if (_currentRoomId == null) return;
        
        var content = EntryMessage.Text?.Trim();
        if (string.IsNullOrEmpty(content)) return;

        if (_isEphemeralMode)
        {
            var durationIndex = PickerEphemeralDuration.SelectedIndex;
            if (durationIndex < 0) durationIndex = 1;
            var duration = _ephemeralDurations[durationIndex];
            _spacetime.SendEphemeralMessage(_currentRoomId.Value, content, duration);
        }
        else
        {
            _spacetime.SendMessage(_currentRoomId.Value, content);
        }

        EntryMessage.Text = "";
        _spacetime.ClearTyping(_currentRoomId.Value);
    }

    private void OnToggleEphemeral(object? sender, EventArgs e)
    {
        _isEphemeralMode = !_isEphemeralMode;
        BtnEphemeral.BackgroundColor = _isEphemeralMode 
            ? Color.FromArgb("#f59e0b") 
            : Color.FromArgb("#0a0a0f");
        PickerEphemeralDuration.IsVisible = _isEphemeralMode;
    }

    private void OnScheduleMessageToggle(object? sender, EventArgs e)
    {
        // Toggle the schedule message input panel
        ScheduleMessagePanel.IsVisible = !ScheduleMessagePanel.IsVisible;
        
        // Reset date/time pickers to reasonable defaults
        if (ScheduleMessagePanel.IsVisible)
        {
            ScheduleDatePicker.Date = DateTime.Today;
            ScheduleTimePicker.Time = DateTime.Now.TimeOfDay.Add(TimeSpan.FromMinutes(5));
        }
    }
    
    private void OnViewScheduledMessages(object? sender, EventArgs e)
    {
        // Show the list of existing scheduled messages
        RefreshScheduledMessages();
        ScheduledMessagesOverlay.IsVisible = true;
    }

    private void OnCloseScheduledMessages(object? sender, EventArgs e)
    {
        ScheduledMessagesOverlay.IsVisible = false;
    }

    private void OnCancelSchedulePanel(object? sender, EventArgs e)
    {
        ScheduleMessagePanel.IsVisible = false;
    }

    private void OnScheduleMessage(object? sender, EventArgs e)
    {
        if (_currentRoomId == null) return;
        
        var content = EntryMessage.Text?.Trim();
        if (string.IsNullOrEmpty(content)) return;

        var scheduledDateTime = ScheduleDatePicker.Date.Add(ScheduleTimePicker.Time);
        var scheduledTimeMs = new DateTimeOffset(scheduledDateTime).ToUnixTimeMilliseconds();

        if (scheduledTimeMs <= DateTimeOffset.UtcNow.ToUnixTimeMilliseconds())
        {
            DisplayAlert("Invalid Time", "Please select a future time.", "OK");
            return;
        }

        _spacetime.ScheduleMessage(_currentRoomId.Value, content, scheduledTimeMs);
        EntryMessage.Text = "";
        ScheduleMessagePanel.IsVisible = false;
    }

    private void ShowReactionPicker(ulong messageId)
    {
        _reactionMessageId = messageId;
        ReactionPickerOverlay.IsVisible = true;
    }

    private void OnCloseReactionPicker(object? sender, EventArgs e)
    {
        ReactionPickerOverlay.IsVisible = false;
        _reactionMessageId = null;
    }

    private void OnReactionSelected(object? sender, EventArgs e)
    {
        if (sender is Button btn && _reactionMessageId != null)
        {
            _spacetime.ToggleReaction(_reactionMessageId.Value, btn.Text);
        }
        ReactionPickerOverlay.IsVisible = false;
        _reactionMessageId = null;
    }

    private void ShowEditDialog(ulong messageId, string currentContent)
    {
        _editingMessageId = messageId;
        EditorEditMessage.Text = currentContent;
        
        // Load edit history
        var conn = _spacetime.Connection;
        if (conn != null)
        {
            EditHistoryList.Children.Clear();
            var edits = conn.Db.MessageEdit.MessageId.Filter(messageId)
                .OrderByDescending(e => e.EditedAt.MicrosecondsSinceUnixEpoch)
                .ToList();

            foreach (var edit in edits)
            {
                var editTime = DateTimeOffset.FromUnixTimeMilliseconds(
                    edit.EditedAt.MicrosecondsSinceUnixEpoch / 1000).LocalDateTime;

                var editItem = new Border
                {
                    BackgroundColor = Color.FromArgb("#1a1a25"),
                    Padding = new Thickness(12, 8),
                    Margin = new Thickness(0, 2)
                };
                editItem.StrokeShape = new Microsoft.Maui.Controls.Shapes.RoundRectangle { CornerRadius = 4 };
                editItem.StrokeThickness = 0;

                var stack = new VerticalStackLayout { Spacing = 2 };
                stack.Children.Add(new Label
                {
                    Text = edit.OldContent,
                    TextColor = Color.FromArgb("#a1a1aa"),
                    FontSize = 13,
                    TextDecorations = TextDecorations.Strikethrough
                });
                stack.Children.Add(new Label
                {
                    Text = editTime.ToString("MMM dd, HH:mm:ss"),
                    TextColor = Color.FromArgb("#71717a"),
                    FontSize = 10
                });

                editItem.Content = stack;
                EditHistoryList.Children.Add(editItem);
            }

            if (!edits.Any())
            {
                EditHistoryList.Children.Add(new Label
                {
                    Text = "No previous versions",
                    TextColor = Color.FromArgb("#71717a"),
                    FontSize = 12,
                    FontAttributes = FontAttributes.Italic
                });
            }
        }

        EditMessageOverlay.IsVisible = true;
    }

    private void ShowEditHistory(ulong messageId, string currentContent)
    {
        ShowEditDialog(messageId, currentContent);
    }

    private void OnSaveEdit(object? sender, EventArgs e)
    {
        if (_editingMessageId != null)
        {
            var newContent = EditorEditMessage.Text?.Trim();
            if (!string.IsNullOrEmpty(newContent))
            {
                _spacetime.EditMessage(_editingMessageId.Value, newContent);
            }
        }
        EditMessageOverlay.IsVisible = false;
        _editingMessageId = null;
    }

    private void OnCancelEdit(object? sender, EventArgs e)
    {
        EditMessageOverlay.IsVisible = false;
        _editingMessageId = null;
    }
}

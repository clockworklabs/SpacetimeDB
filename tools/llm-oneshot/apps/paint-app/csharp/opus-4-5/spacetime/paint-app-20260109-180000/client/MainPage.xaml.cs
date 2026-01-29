using SkiaSharp;
using SkiaSharp.Views.Maui;
using SkiaSharp.Views.Maui.Controls;
using SpacetimeDB;
using SpacetimeDB.Types;
using System.Collections.Concurrent;
using System.Text.Json;

namespace PaintApp.Client;

public partial class MainPage : ContentPage
{
    // SpacetimeDB connection
    private DbConnection? _conn;
    private Identity? _myIdentity;
    private string? _authToken;
    private readonly System.Timers.Timer _tickTimer;
    private bool _isConnected;
    private bool _subscriptionApplied;

    // Current state
    private ulong _currentCanvasId;
    private ulong _currentLayerId;
    private string _currentTool = "brush";
    private SKColor _strokeColor = SKColors.White;
    private SKColor _fillColor = SKColors.Transparent;
    private float _brushSize = 5;
    private float _opacity = 1;
    private float _zoom = 1;
    private SKPoint _panOffset = SKPoint.Empty;

    // Drawing state
    private List<SKPoint> _currentStroke = new();
    private bool _isDrawing;
    private SKPoint _shapeStart;
    private bool _isPanning;
    private SKPoint _lastPanPoint;

    // Cached data from SpacetimeDB
    private readonly ConcurrentDictionary<ulong, CanvasData> _canvases = new();
    private readonly ConcurrentDictionary<ulong, LayerData> _layers = new();
    private readonly ConcurrentDictionary<ulong, StrokeData> _strokes = new();
    private readonly ConcurrentDictionary<ulong, ShapeData> _shapes = new();
    private readonly ConcurrentDictionary<ulong, TextData> _texts = new();
    private readonly ConcurrentDictionary<ulong, ImageData> _images = new();
    private readonly ConcurrentDictionary<ulong, CursorData> _cursors = new();
    private readonly ConcurrentDictionary<ulong, MemberData> _members = new();
    private readonly ConcurrentDictionary<ulong, CommentData> _comments = new();
    private readonly ConcurrentDictionary<ulong, VersionData> _versions = new();
    private readonly ConcurrentDictionary<ulong, TemplateData> _templates = new();
    private readonly ConcurrentDictionary<Identity, UserData> _users = new();

    // Data classes for local caching
    record CanvasData(ulong Id, string Name, Identity CreatorId, bool IsPrivate, int Width, int Height, string BackgroundColor);
    record LayerData(ulong Id, ulong CanvasId, string Name, int ZOrder, bool Visible, double Opacity, bool Locked);
    record StrokeData(ulong Id, ulong CanvasId, ulong LayerId, Identity CreatorId, List<SKPoint> Points, string Color, double Size, double Opacity, string Tool);
    record ShapeData(ulong Id, ulong CanvasId, ulong LayerId, string ShapeType, double X, double Y, double Width, double Height, double Rotation, string StrokeColor, string FillColor, double StrokeWidth);
    record TextData(ulong Id, ulong CanvasId, ulong LayerId, string Content, double X, double Y, double FontSize, string FontFamily, string Color, double Rotation);
    record ImageData(ulong Id, ulong CanvasId, ulong LayerId, string Base64Data, string MimeType, double X, double Y, double Width, double Height, double Rotation);
    record CursorData(ulong Id, Identity UserId, ulong CanvasId, double X, double Y, string Tool, long LastUpdate);
    record MemberData(ulong Id, ulong CanvasId, Identity UserId, string Role, bool IsPresent);
    record CommentData(ulong Id, ulong CanvasId, Identity AuthorId, double X, double Y, string Content, bool IsResolved);
    record VersionData(ulong Id, ulong CanvasId, string Name, bool IsAutoSave, long CreatedAt);
    record TemplateData(ulong Id, Identity CreatorId, string Name, string Description, string Category, bool IsPublic);
    record UserData(Identity Identity, string DisplayName, bool Online);

    public MainPage()
    {
        InitializeComponent();

        // Setup tick timer for SpacetimeDB
        _tickTimer = new System.Timers.Timer(16); // ~60fps
        _tickTimer.Elapsed += (s, e) => _conn?.FrameTick();
        _tickTimer.AutoReset = true;

        // Load saved token
        _authToken = Preferences.Get("spacetimedb_token", null);

        // Start connection
        Dispatcher.Dispatch(ConnectToSpacetimeDB);
    }

    private void ConnectToSpacetimeDB()
    {
        try
        {
            UpdateStatus("Connecting...");

            _conn = DbConnection.Builder()
                .WithUri("http://localhost:3000")
                .WithModuleName("paint-app")
                .WithToken(_authToken)
                .OnConnect(OnConnected)
                .OnDisconnect((conn, err) => Dispatcher.Dispatch(() => OnDisconnected(err)))
                .OnConnectError(err => Dispatcher.Dispatch(() => OnConnectError(err)))
                .Build();

            _tickTimer.Start();
        }
        catch (Exception ex)
        {
            ShowError($"Connection failed: {ex.Message}");
        }
    }

    private void OnConnected(DbConnection conn, Identity identity, string token)
    {
        _myIdentity = identity;
        _authToken = token;
        _isConnected = true;

        // Save token
        Preferences.Set("spacetimedb_token", token);

        // Register callbacks BEFORE subscribing
        RegisterCallbacks();

        // Subscribe to all tables
        conn.SubscriptionBuilder()
            .OnApplied(OnSubscriptionApplied)
            .OnError((ctx, err) => Dispatcher.Dispatch(() => ShowError($"Subscription error: {err}")))
            .SubscribeToAllTables();

        Dispatcher.Dispatch(() =>
        {
            ConnectionIndicator.Fill = new SolidColorBrush(Color.FromArgb("#22c55e"));
            ConnectionStatus.Text = "Connected";
            UpdateStatus("Connected to SpacetimeDB");
        });
    }

    private void OnDisconnected(Exception? err)
    {
        _isConnected = false;
        ConnectionIndicator.Fill = new SolidColorBrush(Color.FromArgb("#ef4444"));
        ConnectionStatus.Text = "Disconnected";
        UpdateStatus(err != null ? $"Disconnected: {err.Message}" : "Disconnected");
    }

    private void OnConnectError(Exception err)
    {
        ShowError($"Connection error: {err.Message}");
        LoadingOverlay.IsVisible = false;
    }

    private void RegisterCallbacks()
    {
        if (_conn == null) return;

        // User callbacks
        _conn.Db.User.OnInsert += (ctx, row) => Dispatcher.Dispatch(() =>
        {
            _users[row.Identity] = new UserData(row.Identity, row.DisplayName, row.Online);
            RefreshCollaborators();
        });
        _conn.Db.User.OnUpdate += (ctx, old, row) => Dispatcher.Dispatch(() =>
        {
            _users[row.Identity] = new UserData(row.Identity, row.DisplayName, row.Online);
            RefreshCollaborators();
        });
        _conn.Db.User.OnDelete += (ctx, row) => Dispatcher.Dispatch(() =>
        {
            _users.TryRemove(row.Identity, out _);
            RefreshCollaborators();
        });

        // Canvas callbacks
        _conn.Db.Canvas.OnInsert += (ctx, row) => Dispatcher.Dispatch(() =>
        {
            _canvases[row.Id] = new CanvasData(row.Id, row.Name, row.CreatorId, row.IsPrivate, row.Width, row.Height, row.BackgroundColor);
            RefreshCanvasList();
        });
        _conn.Db.Canvas.OnUpdate += (ctx, old, row) => Dispatcher.Dispatch(() =>
        {
            _canvases[row.Id] = new CanvasData(row.Id, row.Name, row.CreatorId, row.IsPrivate, row.Width, row.Height, row.BackgroundColor);
            RefreshCanvasList();
        });
        _conn.Db.Canvas.OnDelete += (ctx, row) => Dispatcher.Dispatch(() =>
        {
            _canvases.TryRemove(row.Id, out _);
            RefreshCanvasList();
            if (_currentCanvasId == row.Id)
            {
                _currentCanvasId = 0;
                EmptyStateOverlay.IsVisible = true;
            }
        });

        // Layer callbacks
        _conn.Db.Layer.OnInsert += (ctx, row) => Dispatcher.Dispatch(() =>
        {
            _layers[row.Id] = new LayerData(row.Id, row.CanvasId, row.Name, row.ZOrder, row.Visible, row.Opacity, row.Locked);
            if (row.CanvasId == _currentCanvasId) RefreshLayersList();
        });
        _conn.Db.Layer.OnUpdate += (ctx, old, row) => Dispatcher.Dispatch(() =>
        {
            _layers[row.Id] = new LayerData(row.Id, row.CanvasId, row.Name, row.ZOrder, row.Visible, row.Opacity, row.Locked);
            if (row.CanvasId == _currentCanvasId) RefreshLayersList();
            CanvasView.InvalidateSurface();
        });
        _conn.Db.Layer.OnDelete += (ctx, row) => Dispatcher.Dispatch(() =>
        {
            _layers.TryRemove(row.Id, out _);
            if (row.CanvasId == _currentCanvasId) RefreshLayersList();
            if (_currentLayerId == row.Id) _currentLayerId = 0;
            CanvasView.InvalidateSurface();
        });

        // Stroke callbacks
        _conn.Db.Stroke.OnInsert += (ctx, row) => Dispatcher.Dispatch(() =>
        {
            var points = ParsePoints(row.PointsJson);
            _strokes[row.Id] = new StrokeData(row.Id, row.CanvasId, row.LayerId, row.CreatorId, points, row.Color, row.Size, row.Opacity, row.Tool);
            if (row.CanvasId == _currentCanvasId) CanvasView.InvalidateSurface();
        });
        _conn.Db.Stroke.OnDelete += (ctx, row) => Dispatcher.Dispatch(() =>
        {
            _strokes.TryRemove(row.Id, out _);
            if (row.CanvasId == _currentCanvasId) CanvasView.InvalidateSurface();
        });

        // Shape callbacks
        _conn.Db.Shape.OnInsert += (ctx, row) => Dispatcher.Dispatch(() =>
        {
            _shapes[row.Id] = new ShapeData(row.Id, row.CanvasId, row.LayerId, row.ShapeType, row.X, row.Y, row.Width, row.Height, row.Rotation, row.StrokeColor, row.FillColor, row.StrokeWidth);
            if (row.CanvasId == _currentCanvasId) CanvasView.InvalidateSurface();
        });
        _conn.Db.Shape.OnUpdate += (ctx, old, row) => Dispatcher.Dispatch(() =>
        {
            _shapes[row.Id] = new ShapeData(row.Id, row.CanvasId, row.LayerId, row.ShapeType, row.X, row.Y, row.Width, row.Height, row.Rotation, row.StrokeColor, row.FillColor, row.StrokeWidth);
            if (row.CanvasId == _currentCanvasId) CanvasView.InvalidateSurface();
        });
        _conn.Db.Shape.OnDelete += (ctx, row) => Dispatcher.Dispatch(() =>
        {
            _shapes.TryRemove(row.Id, out _);
            if (row.CanvasId == _currentCanvasId) CanvasView.InvalidateSurface();
        });

        // Text callbacks
        _conn.Db.TextElement.OnInsert += (ctx, row) => Dispatcher.Dispatch(() =>
        {
            _texts[row.Id] = new TextData(row.Id, row.CanvasId, row.LayerId, row.Content, row.X, row.Y, row.FontSize, row.FontFamily, row.Color, row.Rotation);
            if (row.CanvasId == _currentCanvasId) CanvasView.InvalidateSurface();
        });
        _conn.Db.TextElement.OnUpdate += (ctx, old, row) => Dispatcher.Dispatch(() =>
        {
            _texts[row.Id] = new TextData(row.Id, row.CanvasId, row.LayerId, row.Content, row.X, row.Y, row.FontSize, row.FontFamily, row.Color, row.Rotation);
            if (row.CanvasId == _currentCanvasId) CanvasView.InvalidateSurface();
        });
        _conn.Db.TextElement.OnDelete += (ctx, row) => Dispatcher.Dispatch(() =>
        {
            _texts.TryRemove(row.Id, out _);
            if (row.CanvasId == _currentCanvasId) CanvasView.InvalidateSurface();
        });

        // Image callbacks
        _conn.Db.ImageElement.OnInsert += (ctx, row) => Dispatcher.Dispatch(() =>
        {
            _images[row.Id] = new ImageData(row.Id, row.CanvasId, row.LayerId, row.Base64Data, row.MimeType, row.X, row.Y, row.Width, row.Height, row.Rotation);
            if (row.CanvasId == _currentCanvasId) CanvasView.InvalidateSurface();
        });
        _conn.Db.ImageElement.OnUpdate += (ctx, old, row) => Dispatcher.Dispatch(() =>
        {
            _images[row.Id] = new ImageData(row.Id, row.CanvasId, row.LayerId, row.Base64Data, row.MimeType, row.X, row.Y, row.Width, row.Height, row.Rotation);
            if (row.CanvasId == _currentCanvasId) CanvasView.InvalidateSurface();
        });
        _conn.Db.ImageElement.OnDelete += (ctx, row) => Dispatcher.Dispatch(() =>
        {
            _images.TryRemove(row.Id, out _);
            if (row.CanvasId == _currentCanvasId) CanvasView.InvalidateSurface();
        });

        // Cursor callbacks
        _conn.Db.Cursor.OnInsert += (ctx, row) => Dispatcher.Dispatch(() =>
        {
            _cursors[row.Id] = new CursorData(row.Id, row.UserId, row.CanvasId, row.X, row.Y, row.Tool, row.LastUpdate.MicrosecondsSinceUnixEpoch);
            if (row.CanvasId == _currentCanvasId) CanvasView.InvalidateSurface();
        });
        _conn.Db.Cursor.OnUpdate += (ctx, old, row) => Dispatcher.Dispatch(() =>
        {
            _cursors[row.Id] = new CursorData(row.Id, row.UserId, row.CanvasId, row.X, row.Y, row.Tool, row.LastUpdate.MicrosecondsSinceUnixEpoch);
            if (row.CanvasId == _currentCanvasId) CanvasView.InvalidateSurface();
        });
        _conn.Db.Cursor.OnDelete += (ctx, row) => Dispatcher.Dispatch(() =>
        {
            _cursors.TryRemove(row.Id, out _);
            if (row.CanvasId == _currentCanvasId) CanvasView.InvalidateSurface();
        });

        // Member callbacks
        _conn.Db.CanvasMember.OnInsert += (ctx, row) => Dispatcher.Dispatch(() =>
        {
            _members[row.Id] = new MemberData(row.Id, row.CanvasId, row.UserId, row.Role, row.IsPresent);
            if (row.CanvasId == _currentCanvasId) RefreshCollaborators();
        });
        _conn.Db.CanvasMember.OnUpdate += (ctx, old, row) => Dispatcher.Dispatch(() =>
        {
            _members[row.Id] = new MemberData(row.Id, row.CanvasId, row.UserId, row.Role, row.IsPresent);
            if (row.CanvasId == _currentCanvasId) RefreshCollaborators();
        });
        _conn.Db.CanvasMember.OnDelete += (ctx, row) => Dispatcher.Dispatch(() =>
        {
            _members.TryRemove(row.Id, out _);
            if (row.CanvasId == _currentCanvasId) RefreshCollaborators();
        });

        // Comment callbacks
        _conn.Db.Comment.OnInsert += (ctx, row) => Dispatcher.Dispatch(() =>
        {
            _comments[row.Id] = new CommentData(row.Id, row.CanvasId, row.AuthorId, row.X, row.Y, row.Content, row.IsResolved);
            if (row.CanvasId == _currentCanvasId) RefreshComments();
        });
        _conn.Db.Comment.OnUpdate += (ctx, old, row) => Dispatcher.Dispatch(() =>
        {
            _comments[row.Id] = new CommentData(row.Id, row.CanvasId, row.AuthorId, row.X, row.Y, row.Content, row.IsResolved);
            if (row.CanvasId == _currentCanvasId) RefreshComments();
        });
        _conn.Db.Comment.OnDelete += (ctx, row) => Dispatcher.Dispatch(() =>
        {
            _comments.TryRemove(row.Id, out _);
            if (row.CanvasId == _currentCanvasId) RefreshComments();
        });

        // Version callbacks
        _conn.Db.CanvasVersion.OnInsert += (ctx, row) => Dispatcher.Dispatch(() =>
        {
            _versions[row.Id] = new VersionData(row.Id, row.CanvasId, row.Name, row.IsAutoSave, row.CreatedAt.MicrosecondsSinceUnixEpoch);
            if (row.CanvasId == _currentCanvasId) RefreshVersions();
        });
        _conn.Db.CanvasVersion.OnDelete += (ctx, row) => Dispatcher.Dispatch(() =>
        {
            _versions.TryRemove(row.Id, out _);
            if (row.CanvasId == _currentCanvasId) RefreshVersions();
        });

        // Template callbacks
        _conn.Db.Template.OnInsert += (ctx, row) => Dispatcher.Dispatch(() =>
        {
            _templates[row.Id] = new TemplateData(row.Id, row.CreatorId, row.Name, row.Description, row.Category, row.IsPublic);
            RefreshTemplates();
        });
        _conn.Db.Template.OnDelete += (ctx, row) => Dispatcher.Dispatch(() =>
        {
            _templates.TryRemove(row.Id, out _);
            RefreshTemplates();
        });
    }

    private void OnSubscriptionApplied(SubscriptionEventContext ctx)
    {
        _subscriptionApplied = true;

        Dispatcher.Dispatch(() =>
        {
            // Load initial data
            foreach (var user in ctx.Db.User.Iter())
                _users[user.Identity] = new UserData(user.Identity, user.DisplayName, user.Online);

            foreach (var canvas in ctx.Db.Canvas.Iter())
                _canvases[canvas.Id] = new CanvasData(canvas.Id, canvas.Name, canvas.CreatorId, canvas.IsPrivate, canvas.Width, canvas.Height, canvas.BackgroundColor);

            foreach (var layer in ctx.Db.Layer.Iter())
                _layers[layer.Id] = new LayerData(layer.Id, layer.CanvasId, layer.Name, layer.ZOrder, layer.Visible, layer.Opacity, layer.Locked);

            foreach (var stroke in ctx.Db.Stroke.Iter())
            {
                var points = ParsePoints(stroke.PointsJson);
                _strokes[stroke.Id] = new StrokeData(stroke.Id, stroke.CanvasId, stroke.LayerId, stroke.CreatorId, points, stroke.Color, stroke.Size, stroke.Opacity, stroke.Tool);
            }

            foreach (var shape in ctx.Db.Shape.Iter())
                _shapes[shape.Id] = new ShapeData(shape.Id, shape.CanvasId, shape.LayerId, shape.ShapeType, shape.X, shape.Y, shape.Width, shape.Height, shape.Rotation, shape.StrokeColor, shape.FillColor, shape.StrokeWidth);

            foreach (var text in ctx.Db.TextElement.Iter())
                _texts[text.Id] = new TextData(text.Id, text.CanvasId, text.LayerId, text.Content, text.X, text.Y, text.FontSize, text.FontFamily, text.Color, text.Rotation);

            foreach (var img in ctx.Db.ImageElement.Iter())
                _images[img.Id] = new ImageData(img.Id, img.CanvasId, img.LayerId, img.Base64Data, img.MimeType, img.X, img.Y, img.Width, img.Height, img.Rotation);

            foreach (var cursor in ctx.Db.Cursor.Iter())
                _cursors[cursor.Id] = new CursorData(cursor.Id, cursor.UserId, cursor.CanvasId, cursor.X, cursor.Y, cursor.Tool, cursor.LastUpdate.MicrosecondsSinceUnixEpoch);

            foreach (var member in ctx.Db.CanvasMember.Iter())
                _members[member.Id] = new MemberData(member.Id, member.CanvasId, member.UserId, member.Role, member.IsPresent);

            foreach (var comment in ctx.Db.Comment.Iter())
                _comments[comment.Id] = new CommentData(comment.Id, comment.CanvasId, comment.AuthorId, comment.X, comment.Y, comment.Content, comment.IsResolved);

            foreach (var version in ctx.Db.CanvasVersion.Iter())
                _versions[version.Id] = new VersionData(version.Id, version.CanvasId, version.Name, version.IsAutoSave, version.CreatedAt.MicrosecondsSinceUnixEpoch);

            foreach (var template in ctx.Db.Template.Iter())
                _templates[template.Id] = new TemplateData(template.Id, template.CreatorId, template.Name, template.Description, template.Category, template.IsPublic);

            LoadingOverlay.IsVisible = false;
            RefreshCanvasList();
            RefreshTemplates();
            UpdateStatus("Ready");

            // Set display name if we have one
            if (_myIdentity != null && _users.TryGetValue(_myIdentity.Value, out var me) && !string.IsNullOrEmpty(me.DisplayName))
            {
                DisplayNameEntry.Text = me.DisplayName;
            }
        });
    }

    // ========================================================================
    // UI Event Handlers
    // ========================================================================

    private void OnDisplayNameCompleted(object? sender, EventArgs e) => OnSetDisplayName(sender, e);

    private void OnSetDisplayName(object? sender, EventArgs e)
    {
        var name = DisplayNameEntry.Text?.Trim();
        if (string.IsNullOrEmpty(name) || _conn == null) return;

        _conn.Reducers.SetDisplayName(name);
        UpdateStatus($"Display name set to: {name}");
    }

    private async void OnCreateCanvas(object? sender, EventArgs e)
    {
        var name = await DisplayPromptAsync("New Canvas", "Enter canvas name:", "Create", "Cancel", placeholder: "My Canvas");
        if (string.IsNullOrEmpty(name) || _conn == null) return;

        var isPrivate = await DisplayAlert("Canvas Privacy", "Make this canvas private?", "Private", "Public");
        _conn.Reducers.CreateCanvas(name, isPrivate, 1920, 1080, "#0a0a0f");
        UpdateStatus($"Creating canvas: {name}");
    }

    private void OnCreateLayer(object? sender, EventArgs e)
    {
        if (_currentCanvasId == 0 || _conn == null) return;
        _conn.Reducers.CreateLayer(_currentCanvasId, "");
    }

    private void OnSelectBrush(object? sender, EventArgs e) => SelectTool("brush", BtnBrush);
    private void OnSelectEraser(object? sender, EventArgs e) => SelectTool("eraser", BtnEraser);
    private void OnSelectLine(object? sender, EventArgs e) => SelectTool("line", BtnLine);
    private void OnSelectRect(object? sender, EventArgs e) => SelectTool("rectangle", BtnRect);
    private void OnSelectEllipse(object? sender, EventArgs e) => SelectTool("ellipse", BtnEllipse);
    private void OnSelectText(object? sender, EventArgs e) => SelectTool("text", BtnText);
    private void OnSelectFill(object? sender, EventArgs e) => SelectTool("fill", BtnFill);
    private void OnSelectTool(object? sender, EventArgs e) => SelectTool("select", BtnSelect);

    private void SelectTool(string tool, Button btn)
    {
        _currentTool = tool;
        ToolLabel.Text = $"Tool: {char.ToUpper(tool[0])}{tool[1..]}";

        // Reset all tool button backgrounds
        foreach (var b in new[] { BtnBrush, BtnEraser, BtnLine, BtnRect, BtnEllipse, BtnText, BtnFill, BtnSelect })
        {
            b.BackgroundColor = Colors.Transparent;
        }
        btn.BackgroundColor = Color.FromArgb("#6366f1");

        // Update cursor on server
        if (_currentCanvasId > 0 && _conn != null)
        {
            // Don't update cursor position yet, just tool
        }
    }

    private async void OnPickStrokeColor(object? sender, EventArgs e)
    {
        var colorHex = await DisplayPromptAsync("Stroke Color", "Enter hex color:", "OK", "Cancel", "#ffffff");
        if (!string.IsNullOrEmpty(colorHex))
        {
            try
            {
                _strokeColor = SKColor.Parse(colorHex);
                BtnStrokeColor.BackgroundColor = Color.FromArgb(colorHex);
            }
            catch { }
        }
    }

    private async void OnPickFillColor(object? sender, EventArgs e)
    {
        var colorHex = await DisplayPromptAsync("Fill Color", "Enter hex color (or 'none' for transparent):", "OK", "Cancel", "none");
        if (!string.IsNullOrEmpty(colorHex))
        {
            if (colorHex.ToLower() == "none")
            {
                _fillColor = SKColors.Transparent;
                BtnFillColor.BackgroundColor = Colors.Transparent;
            }
            else
            {
                try
                {
                    _fillColor = SKColor.Parse(colorHex);
                    BtnFillColor.BackgroundColor = Color.FromArgb(colorHex);
                }
                catch { }
            }
        }
    }

    private void OnBrushSizeChanged(object? sender, ValueChangedEventArgs e)
    {
        _brushSize = (float)e.NewValue;
        BrushSizeLabel.Text = ((int)_brushSize).ToString();
    }

    private void OnOpacityChanged(object? sender, ValueChangedEventArgs e)
    {
        _opacity = (float)e.NewValue;
    }

    private void OnUndo(object? sender, EventArgs e)
    {
        if (_currentCanvasId == 0 || _conn == null) return;
        _conn.Reducers.Undo(_currentCanvasId);
    }

    private void OnRedo(object? sender, EventArgs e)
    {
        if (_currentCanvasId == 0 || _conn == null) return;
        _conn.Reducers.Redo(_currentCanvasId);
    }

    private void OnZoomIn(object? sender, EventArgs e)
    {
        _zoom = Math.Min(_zoom * 1.2f, 5f);
        ZoomLabel.Text = $"{(int)(_zoom * 100)}%";
        CanvasView.InvalidateSurface();
        UpdateViewportOnServer();
    }

    private void OnZoomOut(object? sender, EventArgs e)
    {
        _zoom = Math.Max(_zoom / 1.2f, 0.1f);
        ZoomLabel.Text = $"{(int)(_zoom * 100)}%";
        CanvasView.InvalidateSurface();
        UpdateViewportOnServer();
    }

    private void OnFitToScreen(object? sender, EventArgs e)
    {
        _zoom = 1;
        _panOffset = SKPoint.Empty;
        ZoomLabel.Text = "100%";
        CanvasView.InvalidateSurface();
        UpdateViewportOnServer();
    }

    private async void OnAddComment(object? sender, EventArgs e)
    {
        if (_currentCanvasId == 0 || _conn == null) return;

        var content = await DisplayPromptAsync("New Comment", "Enter your comment:", "Add", "Cancel");
        if (!string.IsNullOrEmpty(content))
        {
            // Add comment at center of view
            _conn.Reducers.AddComment(_currentCanvasId, 100, 100, content);
        }
    }

    private async void OnSaveVersion(object? sender, EventArgs e)
    {
        if (_currentCanvasId == 0 || _conn == null) return;

        var name = await DisplayPromptAsync("Save Version", "Version name:", "Save", "Cancel", placeholder: "Version 1");
        if (!string.IsNullOrEmpty(name))
        {
            _conn.Reducers.SaveVersion(_currentCanvasId, name);
            UpdateStatus($"Saved version: {name}");
        }
    }

    private async void OnSaveAsTemplate(object? sender, EventArgs e)
    {
        if (_currentCanvasId == 0 || _conn == null) return;

        var name = await DisplayPromptAsync("Save as Template", "Template name:", "Save", "Cancel");
        if (string.IsNullOrEmpty(name)) return;

        var description = await DisplayPromptAsync("Template Description", "Description (optional):", "OK", "Skip");
        var isPublic = await DisplayAlert("Template Visibility", "Make this template public?", "Public", "Private");

        _conn.Reducers.SaveAsTemplate(_currentCanvasId, name, description ?? "", "General", isPublic);
        UpdateStatus($"Saved template: {name}");
    }

    // ========================================================================
    // Canvas Drawing
    // ========================================================================

    private void OnCanvasPaint(object? sender, SKPaintSurfaceEventArgs e)
    {
        var canvas = e.Surface.Canvas;
        var info = e.Info;

        // Clear with background
        var bgColor = SKColor.Parse("#0a0a0f");
        if (_currentCanvasId > 0 && _canvases.TryGetValue(_currentCanvasId, out var canvasData))
        {
            try { bgColor = SKColor.Parse(canvasData.BackgroundColor); } catch { }
        }
        canvas.Clear(bgColor);

        if (_currentCanvasId == 0) return;

        // Apply zoom and pan
        canvas.Save();
        canvas.Translate(_panOffset.X, _panOffset.Y);
        canvas.Scale(_zoom);

        // Get layers for current canvas sorted by z-order
        var canvasLayers = _layers.Values
            .Where(l => l.CanvasId == _currentCanvasId && l.Visible)
            .OrderBy(l => l.ZOrder)
            .ToList();

        foreach (var layer in canvasLayers)
        {
            using var layerPaint = new SKPaint { Color = SKColors.White.WithAlpha((byte)(layer.Opacity * 255)) };
            canvas.SaveLayer(layerPaint);

            // Draw strokes on this layer
            foreach (var stroke in _strokes.Values.Where(s => s.CanvasId == _currentCanvasId && s.LayerId == layer.Id))
            {
                DrawStroke(canvas, stroke);
            }

            // Draw shapes on this layer
            foreach (var shape in _shapes.Values.Where(s => s.CanvasId == _currentCanvasId && s.LayerId == layer.Id))
            {
                DrawShape(canvas, shape);
            }

            // Draw texts on this layer
            foreach (var text in _texts.Values.Where(t => t.CanvasId == _currentCanvasId && t.LayerId == layer.Id))
            {
                DrawText(canvas, text);
            }

            // Draw images on this layer
            foreach (var image in _images.Values.Where(i => i.CanvasId == _currentCanvasId && i.LayerId == layer.Id))
            {
                DrawImage(canvas, image);
            }

            canvas.Restore();
        }

        // Draw current stroke being drawn
        if (_isDrawing && _currentStroke.Count > 1)
        {
            using var paint = new SKPaint
            {
                Color = _currentTool == "eraser" ? bgColor : _strokeColor.WithAlpha((byte)(_opacity * 255)),
                StrokeWidth = _brushSize,
                Style = SKPaintStyle.Stroke,
                StrokeCap = SKStrokeCap.Round,
                StrokeJoin = SKStrokeJoin.Round,
                IsAntialias = true
            };

            var path = new SKPath();
            path.MoveTo(_currentStroke[0]);
            for (int i = 1; i < _currentStroke.Count; i++)
            {
                path.LineTo(_currentStroke[i]);
            }
            canvas.DrawPath(path, paint);
        }

        // Draw shape preview
        if (_isDrawing && (_currentTool == "rectangle" || _currentTool == "ellipse" || _currentTool == "line"))
        {
            DrawShapePreview(canvas);
        }

        canvas.Restore();

        // Draw other users' cursors
        DrawCursors(canvas);

        // Draw comment pins
        DrawCommentPins(canvas);
    }

    private void DrawStroke(SKCanvas canvas, StrokeData stroke)
    {
        if (stroke.Points.Count < 2) return;

        var color = SKColor.Parse(stroke.Color).WithAlpha((byte)(stroke.Opacity * 255));
        using var paint = new SKPaint
        {
            Color = color,
            StrokeWidth = (float)stroke.Size,
            Style = SKPaintStyle.Stroke,
            StrokeCap = SKStrokeCap.Round,
            StrokeJoin = SKStrokeJoin.Round,
            IsAntialias = true
        };

        var path = new SKPath();
        path.MoveTo(stroke.Points[0]);
        for (int i = 1; i < stroke.Points.Count; i++)
        {
            path.LineTo(stroke.Points[i]);
        }
        canvas.DrawPath(path, paint);
    }

    private void DrawShape(SKCanvas canvas, ShapeData shape)
    {
        var strokeColor = SKColor.Parse(shape.StrokeColor);
        var fillColor = string.IsNullOrEmpty(shape.FillColor) || shape.FillColor == "transparent"
            ? SKColors.Transparent : SKColor.Parse(shape.FillColor);

        using var strokePaint = new SKPaint
        {
            Color = strokeColor,
            StrokeWidth = (float)shape.StrokeWidth,
            Style = SKPaintStyle.Stroke,
            IsAntialias = true
        };

        using var fillPaint = new SKPaint
        {
            Color = fillColor,
            Style = SKPaintStyle.Fill,
            IsAntialias = true
        };

        canvas.Save();
        canvas.Translate((float)(shape.X + shape.Width / 2), (float)(shape.Y + shape.Height / 2));
        canvas.RotateDegrees((float)shape.Rotation);
        canvas.Translate((float)(-shape.Width / 2), (float)(-shape.Height / 2));

        var rect = new SKRect(0, 0, (float)shape.Width, (float)shape.Height);

        switch (shape.ShapeType)
        {
            case "rectangle":
                if (fillColor != SKColors.Transparent) canvas.DrawRect(rect, fillPaint);
                canvas.DrawRect(rect, strokePaint);
                break;
            case "ellipse":
                if (fillColor != SKColors.Transparent) canvas.DrawOval(rect, fillPaint);
                canvas.DrawOval(rect, strokePaint);
                break;
            case "line":
                canvas.DrawLine(0, 0, (float)shape.Width, (float)shape.Height, strokePaint);
                break;
        }

        canvas.Restore();
    }

    private void DrawText(SKCanvas canvas, TextData text)
    {
        using var paint = new SKPaint
        {
            Color = SKColor.Parse(text.Color),
            TextSize = (float)text.FontSize,
            IsAntialias = true
        };

        canvas.Save();
        canvas.Translate((float)text.X, (float)text.Y);
        canvas.RotateDegrees((float)text.Rotation);
        canvas.DrawText(text.Content, 0, (float)text.FontSize, paint);
        canvas.Restore();
    }

    private void DrawImage(SKCanvas canvas, ImageData img)
    {
        try
        {
            var bytes = Convert.FromBase64String(img.Base64Data);
            using var bitmap = SKBitmap.Decode(bytes);
            if (bitmap == null) return;

            canvas.Save();
            canvas.Translate((float)(img.X + img.Width / 2), (float)(img.Y + img.Height / 2));
            canvas.RotateDegrees((float)img.Rotation);
            canvas.Translate((float)(-img.Width / 2), (float)(-img.Height / 2));

            var destRect = new SKRect(0, 0, (float)img.Width, (float)img.Height);
            canvas.DrawBitmap(bitmap, destRect);
            canvas.Restore();
        }
        catch { }
    }

    private void DrawShapePreview(SKCanvas canvas)
    {
        using var paint = new SKPaint
        {
            Color = _strokeColor.WithAlpha(128),
            StrokeWidth = _brushSize,
            Style = _fillColor == SKColors.Transparent ? SKPaintStyle.Stroke : SKPaintStyle.StrokeAndFill,
            IsAntialias = true
        };

        if (_fillColor != SKColors.Transparent)
        {
            paint.Color = _fillColor.WithAlpha(128);
        }

        var endPoint = _currentStroke.Count > 0 ? _currentStroke[^1] : _shapeStart;
        var rect = new SKRect(
            Math.Min(_shapeStart.X, endPoint.X),
            Math.Min(_shapeStart.Y, endPoint.Y),
            Math.Max(_shapeStart.X, endPoint.X),
            Math.Max(_shapeStart.Y, endPoint.Y)
        );

        switch (_currentTool)
        {
            case "rectangle":
                canvas.DrawRect(rect, paint);
                break;
            case "ellipse":
                canvas.DrawOval(rect, paint);
                break;
            case "line":
                paint.Style = SKPaintStyle.Stroke;
                paint.Color = _strokeColor.WithAlpha(128);
                canvas.DrawLine(_shapeStart, endPoint, paint);
                break;
        }
    }

    private void DrawCursors(SKCanvas canvas)
    {
        foreach (var cursor in _cursors.Values.Where(c => c.CanvasId == _currentCanvasId && c.UserId != _myIdentity))
        {
            // Get user info
            var userName = "User";
            if (_users.TryGetValue(cursor.UserId, out var user) && !string.IsNullOrEmpty(user.DisplayName))
            {
                userName = user.DisplayName;
            }

            // Generate color from identity
            var hash = cursor.UserId.GetHashCode();
            var hue = Math.Abs(hash % 360);
            var cursorColor = SKColor.FromHsl(hue, 80, 60);

            var x = (float)(cursor.X * _zoom + _panOffset.X);
            var y = (float)(cursor.Y * _zoom + _panOffset.Y);

            // Draw cursor
            using var cursorPaint = new SKPaint
            {
                Color = cursorColor,
                Style = SKPaintStyle.Fill,
                IsAntialias = true
            };

            // Cursor shape (pointer)
            var path = new SKPath();
            path.MoveTo(x, y);
            path.LineTo(x + 12, y + 14);
            path.LineTo(x + 5, y + 14);
            path.LineTo(x, y + 20);
            path.Close();
            canvas.DrawPath(path, cursorPaint);

            // Name label
            using var textPaint = new SKPaint
            {
                Color = cursorColor,
                TextSize = 11,
                IsAntialias = true
            };
            using var bgPaint = new SKPaint
            {
                Color = SKColor.Parse("#0a0a0f").WithAlpha(200),
                Style = SKPaintStyle.Fill
            };

            var textWidth = textPaint.MeasureText(userName);
            var labelRect = new SKRect(x + 14, y + 14, x + 20 + textWidth, y + 28);
            canvas.DrawRoundRect(labelRect, 4, 4, bgPaint);
            canvas.DrawText(userName, x + 17, y + 25, textPaint);

            // Tool icon
            var toolIcon = cursor.Tool switch
            {
                "brush" => "✏️",
                "eraser" => "🧹",
                "rectangle" => "⬜",
                "ellipse" => "⭕",
                "line" => "📏",
                "text" => "T",
                "fill" => "🪣",
                "select" => "◻️",
                _ => "✏️"
            };

            using var iconPaint = new SKPaint { TextSize = 10, IsAntialias = true };
            canvas.DrawText(toolIcon, x - 10, y - 5, iconPaint);
        }
    }

    private void DrawCommentPins(SKCanvas canvas)
    {
        foreach (var comment in _comments.Values.Where(c => c.CanvasId == _currentCanvasId))
        {
            var x = (float)(comment.X * _zoom + _panOffset.X);
            var y = (float)(comment.Y * _zoom + _panOffset.Y);

            var pinColor = comment.IsResolved ? SKColor.Parse("#22c55e") : SKColor.Parse("#eab308");

            using var paint = new SKPaint
            {
                Color = pinColor,
                Style = SKPaintStyle.Fill,
                IsAntialias = true
            };

            // Pin shape
            canvas.DrawCircle(x, y, 12, paint);

            using var textPaint = new SKPaint
            {
                Color = SKColors.White,
                TextSize = 14,
                IsAntialias = true
            };
            canvas.DrawText("💬", x - 7, y + 5, textPaint);
        }
    }

    private void OnCanvasTouch(object? sender, SKTouchEventArgs e)
    {
        var point = new SKPoint(
            (e.Location.X - _panOffset.X) / _zoom,
            (e.Location.Y - _panOffset.Y) / _zoom
        );

        CursorPosLabel.Text = $"X: {(int)point.X}, Y: {(int)point.Y}";

        // Update cursor position on server (throttled)
        if (_currentCanvasId > 0 && _conn != null && e.ActionType == SKTouchAction.Moved)
        {
            _conn.Reducers.UpdateCursor(_currentCanvasId, point.X, point.Y, _currentTool);
        }

        switch (e.ActionType)
        {
            case SKTouchAction.Pressed:
                HandleTouchPressed(point, e);
                break;
            case SKTouchAction.Moved:
                HandleTouchMoved(point, e);
                break;
            case SKTouchAction.Released:
                HandleTouchReleased(point, e);
                break;
        }

        e.Handled = true;
    }

    private void HandleTouchPressed(SKPoint point, SKTouchEventArgs e)
    {
        if (_currentCanvasId == 0 || _currentLayerId == 0) return;

        // Check if shift key for panning (would need keyboard handling)
        // For now, middle mouse or specific tool

        if (_currentTool == "brush" || _currentTool == "eraser")
        {
            _isDrawing = true;
            _currentStroke.Clear();
            _currentStroke.Add(point);
        }
        else if (_currentTool == "rectangle" || _currentTool == "ellipse" || _currentTool == "line")
        {
            _isDrawing = true;
            _shapeStart = point;
            _currentStroke.Clear();
            _currentStroke.Add(point);
        }
        else if (_currentTool == "text")
        {
            // Handle text tool - prompt for text
            Dispatcher.Dispatch(async () =>
            {
                var text = await DisplayPromptAsync("Add Text", "Enter text:", "Add", "Cancel");
                if (!string.IsNullOrEmpty(text) && _conn != null)
                {
                    _conn.Reducers.AddText(_currentCanvasId, _currentLayerId, text, point.X, point.Y,
                        _brushSize * 3, "Arial", ColorToHex(_strokeColor));
                }
            });
        }
        else if (_currentTool == "fill")
        {
            if (_conn != null)
            {
                _conn.Reducers.AddFill(_currentCanvasId, _currentLayerId, point.X, point.Y, ColorToHex(_fillColor), 32);
            }
        }

        CanvasView.InvalidateSurface();
    }

    private void HandleTouchMoved(SKPoint point, SKTouchEventArgs e)
    {
        if (!_isDrawing) return;

        _currentStroke.Add(point);
        CanvasView.InvalidateSurface();
    }

    private void HandleTouchReleased(SKPoint point, SKTouchEventArgs e)
    {
        if (!_isDrawing || _conn == null) return;

        _isDrawing = false;

        if ((_currentTool == "brush" || _currentTool == "eraser") && _currentStroke.Count > 1)
        {
            var pointsJson = SerializePoints(_currentStroke);
            _conn.Reducers.AddStroke(_currentCanvasId, _currentLayerId, pointsJson,
                ColorToHex(_strokeColor), _brushSize, _opacity, _currentTool);
        }
        else if (_currentTool == "rectangle" || _currentTool == "ellipse" || _currentTool == "line")
        {
            var endPoint = _currentStroke.Count > 0 ? _currentStroke[^1] : _shapeStart;
            var x = Math.Min(_shapeStart.X, endPoint.X);
            var y = Math.Min(_shapeStart.Y, endPoint.Y);
            var width = Math.Abs(endPoint.X - _shapeStart.X);
            var height = Math.Abs(endPoint.Y - _shapeStart.Y);

            if (_currentTool == "line")
            {
                x = _shapeStart.X;
                y = _shapeStart.Y;
                width = endPoint.X - _shapeStart.X;
                height = endPoint.Y - _shapeStart.Y;
            }

            if (width > 2 || height > 2)
            {
                _conn.Reducers.AddShape(_currentCanvasId, _currentLayerId, _currentTool,
                    x, y, width, height, 0, ColorToHex(_strokeColor),
                    _fillColor == SKColors.Transparent ? "transparent" : ColorToHex(_fillColor), _brushSize);
            }
        }

        _currentStroke.Clear();
        CanvasView.InvalidateSurface();
    }

    // ========================================================================
    // UI Refresh Methods
    // ========================================================================

    private void RefreshCanvasList()
    {
        CanvasList.Children.Clear();

        foreach (var canvas in _canvases.Values.OrderByDescending(c => c.Id))
        {
            var frame = new Frame
            {
                BackgroundColor = canvas.Id == _currentCanvasId ? Color.FromArgb("#1c1c26") : Colors.Transparent,
                BorderColor = Color.FromArgb("#334155"),
                CornerRadius = 8,
                Padding = new Thickness(12, 8),
                Margin = new Thickness(0, 2)
            };

            var stack = new HorizontalStackLayout { Spacing = 8 };

            var icon = new Label
            {
                Text = canvas.IsPrivate ? "🔒" : "📄",
                VerticalOptions = LayoutOptions.Center
            };

            var nameLabel = new Label
            {
                Text = canvas.Name,
                TextColor = Color.FromArgb("#f8fafc"),
                VerticalOptions = LayoutOptions.Center
            };

            stack.Children.Add(icon);
            stack.Children.Add(nameLabel);
            frame.Content = stack;

            var canvasId = canvas.Id;
            var tapGesture = new TapGestureRecognizer();
            tapGesture.Tapped += (s, e) => JoinCanvas(canvasId);
            frame.GestureRecognizers.Add(tapGesture);

            CanvasList.Children.Add(frame);
        }
    }

    private void JoinCanvas(ulong canvasId)
    {
        if (_conn == null) return;

        _conn.Reducers.JoinCanvas(canvasId);
        _currentCanvasId = canvasId;

        // Select first layer
        var firstLayer = _layers.Values
            .Where(l => l.CanvasId == canvasId)
            .OrderBy(l => l.ZOrder)
            .FirstOrDefault();

        if (firstLayer != default)
        {
            _currentLayerId = firstLayer.Id;
            LayerLabel.Text = $"Layer: {firstLayer.Name}";
        }

        EmptyStateOverlay.IsVisible = false;

        if (_canvases.TryGetValue(canvasId, out var canvas))
        {
            CanvasNameLabel.Text = canvas.Name;
            UpdateStatus($"Joined canvas: {canvas.Name}");
        }

        // Update role
        var member = _members.Values.FirstOrDefault(m => m.CanvasId == canvasId && m.UserId == _myIdentity);
        if (member != default)
        {
            RoleLabel.Text = $"Role: {char.ToUpper(member.Role[0])}{member.Role[1..]}";
        }

        RefreshLayersList();
        RefreshCollaborators();
        RefreshComments();
        RefreshVersions();
        RefreshCanvasList();
        CanvasView.InvalidateSurface();
    }

    private void RefreshLayersList()
    {
        LayersList.Children.Clear();

        var layers = _layers.Values
            .Where(l => l.CanvasId == _currentCanvasId)
            .OrderByDescending(l => l.ZOrder)
            .ToList();

        foreach (var layer in layers)
        {
            var frame = new Frame
            {
                BackgroundColor = layer.Id == _currentLayerId ? Color.FromArgb("#6366f1") : Color.FromArgb("#1c1c26"),
                BorderColor = Color.FromArgb("#334155"),
                CornerRadius = 6,
                Padding = new Thickness(10, 6),
                Margin = new Thickness(0, 2)
            };

            var stack = new HorizontalStackLayout { Spacing = 8 };

            var visIcon = new Label
            {
                Text = layer.Visible ? "👁️" : "👁️‍🗨️",
                VerticalOptions = LayoutOptions.Center,
                Opacity = layer.Visible ? 1 : 0.5
            };

            var lockIcon = new Label
            {
                Text = layer.Locked ? "🔒" : "",
                VerticalOptions = LayoutOptions.Center
            };

            var nameLabel = new Label
            {
                Text = layer.Name,
                TextColor = Color.FromArgb("#f8fafc"),
                VerticalOptions = LayoutOptions.Center
            };

            stack.Children.Add(visIcon);
            if (layer.Locked) stack.Children.Add(lockIcon);
            stack.Children.Add(nameLabel);
            frame.Content = stack;

            var layerId = layer.Id;
            var layerName = layer.Name;
            var tapGesture = new TapGestureRecognizer();
            tapGesture.Tapped += (s, e) =>
            {
                _currentLayerId = layerId;
                LayerLabel.Text = $"Layer: {layerName}";
                RefreshLayersList();
            };
            frame.GestureRecognizers.Add(tapGesture);

            LayersList.Children.Add(frame);
        }
    }

    private void RefreshCollaborators()
    {
        CollaboratorsList.Children.Clear();

        var members = _members.Values
            .Where(m => m.CanvasId == _currentCanvasId && m.IsPresent)
            .ToList();

        foreach (var member in members)
        {
            var userName = "Unknown";
            var isOnline = false;

            if (_users.TryGetValue(member.UserId, out var user))
            {
                userName = string.IsNullOrEmpty(user.DisplayName) ? "Anonymous" : user.DisplayName;
                isOnline = user.Online;
            }

            var stack = new HorizontalStackLayout { Spacing = 8 };

            var status = new BoxView
            {
                Color = isOnline ? Color.FromArgb("#22c55e") : Color.FromArgb("#64748b"),
                WidthRequest = 8,
                HeightRequest = 8,
                CornerRadius = 4,
                VerticalOptions = LayoutOptions.Center
            };

            var nameLabel = new Label
            {
                Text = userName,
                TextColor = Color.FromArgb("#f8fafc"),
                FontSize = 12,
                VerticalOptions = LayoutOptions.Center
            };

            var roleLabel = new Label
            {
                Text = $"({member.Role})",
                TextColor = Color.FromArgb("#64748b"),
                FontSize = 10,
                VerticalOptions = LayoutOptions.Center
            };

            stack.Children.Add(status);
            stack.Children.Add(nameLabel);
            stack.Children.Add(roleLabel);

            CollaboratorsList.Children.Add(stack);
        }
    }

    private void RefreshComments()
    {
        CommentsList.Children.Clear();

        var comments = _comments.Values
            .Where(c => c.CanvasId == _currentCanvasId)
            .OrderByDescending(c => c.Id)
            .ToList();

        foreach (var comment in comments)
        {
            var authorName = "Unknown";
            if (_users.TryGetValue(comment.AuthorId, out var user))
            {
                authorName = string.IsNullOrEmpty(user.DisplayName) ? "Anonymous" : user.DisplayName;
            }

            var frame = new Frame
            {
                BackgroundColor = comment.IsResolved ? Color.FromArgb("#1c1c26") : Color.FromArgb("#2a2a36"),
                BorderColor = comment.IsResolved ? Color.FromArgb("#22c55e") : Color.FromArgb("#eab308"),
                CornerRadius = 8,
                Padding = new Thickness(10, 8),
                Opacity = comment.IsResolved ? 0.6 : 1
            };

            var stack = new VerticalStackLayout { Spacing = 4 };

            var header = new HorizontalStackLayout { Spacing = 8 };
            header.Children.Add(new Label
            {
                Text = authorName,
                FontAttributes = FontAttributes.Bold,
                TextColor = Color.FromArgb("#22d3ee"),
                FontSize = 12
            });

            if (comment.IsResolved)
            {
                header.Children.Add(new Label
                {
                    Text = "✓ Resolved",
                    TextColor = Color.FromArgb("#22c55e"),
                    FontSize = 10
                });
            }

            stack.Children.Add(header);
            stack.Children.Add(new Label
            {
                Text = comment.Content,
                TextColor = Color.FromArgb("#f8fafc"),
                FontSize = 12
            });

            frame.Content = stack;
            CommentsList.Children.Add(frame);
        }
    }

    private void RefreshVersions()
    {
        VersionsList.Children.Clear();

        var versions = _versions.Values
            .Where(v => v.CanvasId == _currentCanvasId)
            .OrderByDescending(v => v.CreatedAt)
            .Take(10)
            .ToList();

        foreach (var version in versions)
        {
            var time = DateTimeOffset.FromUnixTimeMilliseconds(version.CreatedAt / 1000).LocalDateTime;

            var frame = new Frame
            {
                BackgroundColor = Color.FromArgb("#1c1c26"),
                BorderColor = Color.FromArgb("#334155"),
                CornerRadius = 6,
                Padding = new Thickness(8, 6)
            };

            var stack = new HorizontalStackLayout { Spacing = 8 };

            var icon = new Label
            {
                Text = version.IsAutoSave ? "⏰" : "💾",
                VerticalOptions = LayoutOptions.Center
            };

            var nameLabel = new Label
            {
                Text = version.Name,
                TextColor = Color.FromArgb("#f8fafc"),
                FontSize = 11,
                VerticalOptions = LayoutOptions.Center
            };

            var timeLabel = new Label
            {
                Text = time.ToString("HH:mm"),
                TextColor = Color.FromArgb("#64748b"),
                FontSize = 10,
                VerticalOptions = LayoutOptions.Center
            };

            stack.Children.Add(icon);
            stack.Children.Add(nameLabel);
            stack.Children.Add(timeLabel);
            frame.Content = stack;

            VersionsList.Children.Add(frame);
        }
    }

    private void RefreshTemplates()
    {
        TemplatesList.Children.Clear();

        var templates = _templates.Values
            .Where(t => t.IsPublic || t.CreatorId == _myIdentity)
            .OrderByDescending(t => t.Id)
            .Take(10)
            .ToList();

        foreach (var template in templates)
        {
            var frame = new Frame
            {
                BackgroundColor = Color.FromArgb("#1c1c26"),
                BorderColor = Color.FromArgb("#334155"),
                CornerRadius = 6,
                Padding = new Thickness(8, 6)
            };

            var stack = new HorizontalStackLayout { Spacing = 8 };

            var icon = new Label
            {
                Text = template.IsPublic ? "🌍" : "🔒",
                VerticalOptions = LayoutOptions.Center
            };

            var nameLabel = new Label
            {
                Text = template.Name,
                TextColor = Color.FromArgb("#f8fafc"),
                FontSize = 11,
                VerticalOptions = LayoutOptions.Center
            };

            stack.Children.Add(icon);
            stack.Children.Add(nameLabel);
            frame.Content = stack;

            var templateId = template.Id;
            var templateName = template.Name;
            var tapGesture = new TapGestureRecognizer();
            tapGesture.Tapped += async (s, e) =>
            {
                var use = await DisplayAlert("Use Template", $"Create canvas from '{templateName}'?", "Create", "Cancel");
                if (use && _conn != null)
                {
                    var name = await DisplayPromptAsync("New Canvas", "Canvas name:", placeholder: templateName);
                    if (!string.IsNullOrEmpty(name))
                    {
                        _conn.Reducers.CreateCanvasFromTemplate(templateId, name, false);
                    }
                }
            };
            frame.GestureRecognizers.Add(tapGesture);

            TemplatesList.Children.Add(frame);
        }
    }

    // ========================================================================
    // Helper Methods
    // ========================================================================

    private void UpdateStatus(string message)
    {
        StatusLabel.Text = message;
    }

    private void ShowError(string message)
    {
        Dispatcher.Dispatch(async () =>
        {
            await DisplayAlert("Error", message, "OK");
            UpdateStatus($"Error: {message}");
        });
    }

    private void UpdateViewportOnServer()
    {
        if (_currentCanvasId > 0 && _conn != null)
        {
            _conn.Reducers.UpdateViewport(_currentCanvasId, _panOffset.X, _panOffset.Y, _zoom);
        }
    }

    private static List<SKPoint> ParsePoints(string json)
    {
        var points = new List<SKPoint>();
        try
        {
            using var doc = JsonDocument.Parse(json);
            foreach (var elem in doc.RootElement.EnumerateArray())
            {
                var x = elem.GetProperty("x").GetSingle();
                var y = elem.GetProperty("y").GetSingle();
                points.Add(new SKPoint(x, y));
            }
        }
        catch { }
        return points;
    }

    private static string SerializePoints(List<SKPoint> points)
    {
        var items = points.Select(p => $"{{\"x\":{p.X},\"y\":{p.Y}}}");
        return $"[{string.Join(",", items)}]";
    }

    private static string ColorToHex(SKColor color)
    {
        return $"#{color.Red:X2}{color.Green:X2}{color.Blue:X2}";
    }
}

using System;
using Godot;
using SpacetimeDB;
using SpacetimeDB.Types;

public partial class GameManager : Node2D
{
    public static event Action OnConnected;
    public static event Action OnSubscriptionApplied;
    
    [Export]    
    private string ServerUrl { get; set; } = "http://127.0.0.1:3000";
    
    [Export]
    private string DatabaseName { get; set; } = "blackholio";

    [Export]
    private Color BackgroundColor { get; set; } = Colors.MidnightBlue;

    [Export]
    private float BorderThickness { get; set; } = 5.0f;

    [Export]
    private Color BorderColor { get; set; } = Colors.Goldenrod;

    [Export]
    private string DefaultPlayerName { get; set; } = "3Blave";

    private static GameManager Instance { get; set; }
    public static Identity LocalIdentity { get; private set; }
    public static DbConnection Conn { get; private set; }

    public GameManager()
    {
        var builder = DbConnection.Builder()
            .OnConnect(HandleConnect)
            .OnConnectError(HandleConnectError)
            .OnDisconnect(HandleDisconnect)
            .WithUri(ServerUrl)
            .WithDatabaseName(DatabaseName);

        if (AuthToken.TryGetToken(out var authToken))
        {
            builder = builder.WithToken(authToken);
        }

        Conn = builder.Build();
        STDBUpdateManager.Add(Conn);
    }

    public override void _EnterTree()
    {
        Instance = this;
    }

    public override void _ExitTree()
    {
        Disconnect();

        if (Instance == this)
        {
            Instance = null;
        }
    }

    public static bool IsConnected() => Conn != null && Conn.IsActive;

    private void Disconnect()
    {
        STDBUpdateManager.Remove(Conn, true);
        Conn = null;
    }

    private void HandleConnect(DbConnection conn, Identity identity, string token)
    {
        GD.Print("Connected.");
        AuthToken.SaveToken(token);
        LocalIdentity = identity;

        OnConnected?.Invoke();
        
        AddChild(new Instantiator(conn));

        Conn.SubscriptionBuilder()
            .OnApplied(HandleSubscriptionApplied)
            .SubscribeToAllTables();
    }

    private void HandleConnectError(Exception ex)
    {
        GD.PrintErr($"Connection error: {ex}");
    }

    private void HandleDisconnect(DbConnection conn, Exception ex)
    {
        GD.Print("Disconnected.");
        if (ex != null)
        {
            GD.PrintErr(ex);
        }
    }

    private void HandleSubscriptionApplied(SubscriptionEventContext ctx)
    {
        GD.Print("Subscription applied!");
        OnSubscriptionApplied?.Invoke();

        // Once we have the initial subscription sync'd to the client cache
        // Get the world size from the config table and set up the arena
        var worldSize = Conn.Db.Config.Id.Find(0).WorldSize;
        SetupArena(worldSize);
        
        ctx.Reducers.EnterGame(DefaultPlayerName);
    }

    private void SetupArena(float worldSize)
    {
        var polygon = new[]
        {
            new Vector2(0, 0),
            new Vector2(worldSize, 0),
            new Vector2(worldSize, worldSize),
            new Vector2(0, worldSize),
        };
        var background = new Polygon2D
        {
            Name = "Background",
            Color = BackgroundColor,
            Position = Vector2.Zero,
            Polygon = polygon
        };
        background.AddChild(new Polygon2D
        {
            Name = "Border",
            Color = BorderColor,
            Position = Vector2.Zero,
            InvertEnabled = true,
            InvertBorder = BorderThickness,
            Polygon = polygon
        });
        AddChild(background, @internal: InternalMode.Front);

        AddChild(new CameraController(worldSize));
    }
}

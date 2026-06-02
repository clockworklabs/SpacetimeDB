using System;
using System.Collections.Generic;
using System.Linq;
using SpacetimeDB;
using SpacetimeDB.Types;
using Godot;

public partial class GameManager : Node
{
	public static event Action OnConnected;
	public static event Action OnSubscriptionApplied;
	
	[Export]    
	private string ServerUrl { get; set; } = "http://127.0.0.1:3000";
	
	[Export]
	private string DatabaseName { get; set; } = "blackholio";

	[Export]
	private Color BackgroundColor { get; set; } = new(0.006f, 0.009f, 0.024f);

	[Export]
	private float BorderThickness { get; set; } = 5.0f;

	[Export]
	private Color BorderColor { get; set; } = Colors.Goldenrod;

	[Export]
	private string DefaultPlayerName { get; set; } = "3Blave";

	private static GameManager Instance { get; set; }
	public static Identity LocalIdentity { get; private set; }
	public static DbConnection Conn { get; private set; }

	private HudController Hud { get; set; }

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
		Conn.OnUnhandledReducerError += HandleUnhandledReducerError;
		STDBUpdateManager.Add(Conn);
	}

	public override void _EnterTree()
	{
		Instance = this;
		Hud = new HudController(DefaultPlayerName);
		AddChild(Hud);
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
		if (Conn != null)
		{
			Conn.OnUnhandledReducerError -= HandleUnhandledReducerError;
			Conn.Db.Player.OnUpdate -= HideUsernameChooserAfterNameUpdate;
		}

		STDBUpdateManager.Remove(Conn, true);
		Conn = null;
	}

	// Called when we connect to SpacetimeDB and receive our client identity
	private void HandleConnect(DbConnection conn, Identity identity, string token)
	{
		GD.Print("Connected.");
		AuthToken.SaveToken(token);
		LocalIdentity = identity;

		OnConnected?.Invoke();

		AddChild(new Instantiator(conn));

		// Request all tables
		Conn.SubscriptionBuilder()
			.OnApplied(HandleSubscriptionApplied)
			.SubscribeToAllTables();
	}

	private void HandleConnectError(Exception ex)
	{
		GD.PrintErr($"Connection error: {ex}");
	}

	private void HandleDisconnect(DbConnection _conn, Exception ex)
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

		var player = ctx.Db.Player.Identity.Find(LocalIdentity);
		if (player == null || string.IsNullOrEmpty(player.Name))
		{
			HudController.Instance?.ShowUsernameChooser(true);
			Conn.Db.Player.OnUpdate += HideUsernameChooserAfterNameUpdate;
			return;
		}

		HudController.Instance?.ShowUsernameChooser(false);
		if (!ctx.Db.Circle.PlayerId.Filter(player.PlayerId).Any())
		{
			ctx.Reducers.EnterGame(player.Name);
		}
	}

	private static void HideUsernameChooserAfterNameUpdate(EventContext context, Player oldPlayer, Player newPlayer)
	{
		if (newPlayer.Identity != LocalIdentity || string.IsNullOrEmpty(newPlayer.Name)) return;

		HudController.Instance?.ShowUsernameChooser(false);
		Conn.Db.Player.OnUpdate -= HideUsernameChooserAfterNameUpdate;
	}

	private static void HandleUnhandledReducerError(ReducerEventContext context, Exception ex)
	{
		GD.PrintErr($"Reducer error: {ex.Message}");
	}
	
	private void SetupArena(float worldSize)
	{
		AddChild(new StarfieldBackground(worldSize, BackgroundColor), @internal: InternalMode.Back);

		var border = new Polygon2D
		{
			Name = "Arena Border",
			Color = BorderColor,
			Position = Vector2.Zero,
			InvertEnabled = true,
			InvertBorder = BorderThickness,
			Polygon = new[]
			{
				new Vector2(0, 0),
				new Vector2(worldSize, 0),
				new Vector2(worldSize, worldSize),
				new Vector2(0, worldSize),
			},
			ZIndex = -500
		};
		AddChild(border);

		AddChild(new CameraController(worldSize));
	}
}

using System;
using System.Collections.Generic;
using Godot;
using SpacetimeDB;
using SpacetimeDB.Types;

public partial class GameManager : Node2D
{
	private const string ServerUrl = "http://127.0.0.1:3000";
	private const string ModuleName = "blackholio";

	public static event Action OnConnected;
	public static event Action OnSubscriptionApplied;

	[Export]
	private Color BackgroundColor { get; set; } = Colors.MidnightBlue;

	[Export]
	private float BorderThickness { get; set; } = 2.0f;

	[Export]
	private Color BorderColor { get; set; } = Colors.Goldenrod;

	[Export]
	private string DefaultPlayerName { get; set; } = "3Blave";

	public static GameManager Instance { get; private set; }
	public static Identity LocalIdentity { get; private set; }
	public static DbConnection Conn { get; private set; }

	public static Dictionary<int, EntityController> Entities { get; } = new();
	public static Dictionary<int, PlayerController> Players { get; } = new();
	
	private Instantiator _instantiator;
	public Instantiator Instantiator => _instantiator ??= GetNode<Instantiator>("Instantiator") ?? new Instantiator();

	public GameManager()
	{
		var builder = DbConnection.Builder()
			.OnConnect(HandleConnect)
			.OnConnectError(HandleConnectError)
			.OnDisconnect(HandleDisconnect)
			.WithUri(ServerUrl)
			.WithDatabaseName(ModuleName);

		AuthToken.Init();
		if (AuthToken.Token != string.Empty)
		{
			builder = builder.WithToken(AuthToken.Token);
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
		if (Conn != null)
		{
			STDBUpdateManager.Remove(Conn, true);
			Conn = null;
		}

		Entities.Clear();
		Players.Clear();

		if (Instance == this)
		{
			Instance = null;
		}
	}

	public static bool IsConnected() => Conn != null && Conn.IsActive;

	public void Disconnect()
	{
		if (Conn == null)
		{
			return;
		}

		STDBUpdateManager.Remove(Conn, true);
		Conn = null;
	}

	private void HandleConnect(DbConnection conn, Identity identity, string token)
	{
		GD.Print("Connected.");
		AuthToken.SaveToken(token);
		LocalIdentity = identity;

		conn.Db.Circle.OnInsert += CircleOnInsert;
		conn.Db.Entity.OnUpdate += EntityOnUpdate;
		conn.Db.Entity.OnDelete += EntityOnDelete;
		conn.Db.Food.OnInsert += FoodOnInsert;
		conn.Db.Player.OnInsert += PlayerOnInsert;
		conn.Db.Player.OnDelete += PlayerOnDelete;

		OnConnected?.Invoke();

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

		CameraController.WorldSize = worldSize;
	}

	private void CircleOnInsert(EventContext context, Circle insertedValue)
	{
		var player = GetOrCreatePlayer(insertedValue.PlayerId);
		var entityController = Instantiator.SpawnCircle(insertedValue, player);
		Entities[insertedValue.EntityId] = entityController;
	}

	private void EntityOnUpdate(EventContext context, Entity oldEntity, Entity newEntity)
	{
		if (Entities.TryGetValue(newEntity.EntityId, out var entityController))
		{
			entityController.OnEntityUpdated(newEntity);
		}
	}

	private void EntityOnDelete(EventContext context, Entity oldEntity)
	{
		if (Entities.Remove(oldEntity.EntityId, out var entityController))
		{
			entityController.OnDelete(context);
		}
	}

	private void FoodOnInsert(EventContext context, Food insertedValue)
	{
		var entityController = Instantiator.SpawnFood(insertedValue);
		Entities[insertedValue.EntityId] = entityController;
	}

	private void PlayerOnInsert(EventContext context, Player insertedPlayer)
	{
		GetOrCreatePlayer(insertedPlayer.PlayerId);
	}

	private void PlayerOnDelete(EventContext context, Player deletedValue)
	{
		if (Players.Remove(deletedValue.PlayerId, out var playerController))
		{
			playerController.QueueFree();
		}
	}

	private PlayerController GetOrCreatePlayer(int playerId)
	{
		if (!Players.TryGetValue(playerId, out var playerController))
		{
			var player = Conn.Db.Player.PlayerId.Find(playerId);
			playerController = Instantiator.SpawnPlayer(player);
			Players[playerId] = playerController;
		}

		return playerController;
	}
}

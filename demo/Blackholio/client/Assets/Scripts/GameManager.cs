using System;
using System.Collections.Generic;
using System.Linq;
using SpacetimeDB;
using SpacetimeDB.Types;
using UnityEngine;

public class GameManager : MonoBehaviour
{
    const string SERVER_URL = "http://127.0.0.1:3000";
    const string MODULE_NAME = "untitled-circle-game";

    public static event Action OnConnected;
    public static event Action OnSubscriptionApplied;

	public static GameManager Instance { get; private set; }
    public static Identity LocalIdentity { get; private set; }
    public static DbConnection Conn { get; private set; }

    public static Dictionary<uint, EntityController> Actors = new Dictionary<uint, EntityController>();
	public static Dictionary<uint, PlayerController> Players = new Dictionary<uint, PlayerController>();

    private void Start()
    {
        Instance = this;
        Application.targetFrameRate = 60;

        // In order to build a connection to SpacetimeDB we need to register
        // our callbacks and specify a SpacetimeDB server URI and module name.
        var builder = DbConnection.Builder()
            .OnConnect(HandleConnect)
            .OnConnectError(HandleConnectError)
            .OnDisconnect(HandleDisconnect)
            .WithUri(SERVER_URL)
            .WithModuleName(MODULE_NAME);

        // If the user has a SpacetimeDB auth token stored in the Unity PlayerPrefs,
        // we can use it to authenticate the connection.
		if (PlayerPrefs.HasKey(AuthToken.GetTokenKey()))
        {
			builder = builder.WithCredentials((default, AuthToken.Token));
        }

        // Building the connection will establish a connection to the SpacetimeDB
        // server.
        Conn = builder.Build();

    /* BEGIN: not in tutorial */
#pragma warning disable CS0612 // Type or member is obsolete
		Conn.onUnhandledReducerError += InstanceOnUnhandledReducerError;
#pragma warning restore CS0612 // Type or member is obsolete
    /* END: not in tutorial */
    }

    // Called when we connect to SpacetimeDB and receive our client identity
    void HandleConnect(DbConnection conn, Identity identity, string token)
    {
        Debug.Log("Connected.");
        AuthToken.SaveToken(token);
        LocalIdentity = identity;

		conn.Db.Circle.OnInsert += CircleOnInsert;
		conn.Db.Entity.OnUpdate += EntityOnUpdate;
		conn.Db.Entity.OnDelete += EntityOnDelete;
		conn.Db.Food.OnInsert += FoodOnInsert;
		conn.Db.Player.OnInsert += PlayerOnInsert;
		conn.Db.Player.OnDelete += PlayerOnDelete;
        OnConnected?.Invoke();

        // Request all tables
        Conn.SubscriptionBuilder().OnApplied(ctx =>
        {
            Debug.Log("Subscription applied!");
            OnSubscriptionApplied?.Invoke();
        }).Subscribe("SELECT * FROM *");
    }

    void HandleConnectError(Exception ex)
    {
        Debug.LogError($"Connection error: {ex}");
    }

    void HandleDisconnect(DbConnection _conn, Exception ex)
    {
        Debug.Log("Disconnected.");
        if (ex != null)
        {
            Debug.LogException(ex);
        }
    }

    private static void CircleOnInsert(EventContext context, Circle insertedValue)
	{
		var player = GetOrCreatePlayer(insertedValue.PlayerId);
		var actor = PrefabManager.SpawnCircle(insertedValue, player);
		Actors.Add(insertedValue.EntityId, actor);
	}

	private static void EntityOnUpdate(EventContext context, Entity oldEntity, Entity newEntity)
	{
		if (!Actors.TryGetValue(newEntity.EntityId, out var actor))
		{
			return;
		}
		actor.OnEntityUpdated(newEntity);
	}

	private static void EntityOnDelete(EventContext context, Entity oldEntity)
	{
		if (Actors.Remove(oldEntity.EntityId, out var actor))
		{
			actor.OnDelete(context);
		}
	}

	private static void FoodOnInsert(EventContext context, Food insertedValue)
	{
		var actor = PrefabManager.SpawnFood(insertedValue);
		Actors.Add(insertedValue.EntityId, actor);
	}

	private static void PlayerOnInsert(EventContext context, Player insertedPlayer)
	{
		GetOrCreatePlayer(insertedPlayer.PlayerId);
	}

	private static void PlayerOnDelete(EventContext context, Player deletedvalue)
	{
		if (Players.Remove(deletedvalue.PlayerId, out var playerController))
		{
			GameObject.Destroy(playerController.gameObject);
		}
	}

	private static PlayerController GetOrCreatePlayer(uint playerId)
	{
		if (!Players.TryGetValue(playerId, out var playerController))
		{
			var player = Conn.Db.Player.PlayerId.Find(playerId);
			playerController = PrefabManager.SpawnPlayer(player);
			Players.Add(playerId, playerController);
		}

		return playerController;
	}

    /* BEGIN: not in tutorial */
    private void InstanceOnUnhandledReducerError(ReducerEvent<Reducer> reducerEvent)
    {
        Debug.LogError($"There was an error!\r\n{(reducerEvent.Status as Status.Failed)?.Failed_}");
    }

    public void Disconnect()
    {
        Conn.Disconnect();
        Conn = null;
    }

    public static bool IsConnected()
    {
        return Conn != null && Conn.IsActive;
    }
    /* END: not in tutorial */
}

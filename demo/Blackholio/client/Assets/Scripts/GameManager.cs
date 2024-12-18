using System;
using System.Collections;
using System.Collections.Generic;
using System.Linq;
using System.Security.Claims;
using SpacetimeDB;
using SpacetimeDB.Types;
using UnityEngine;
using UnityEngine.Serialization;
using Random = UnityEngine.Random;

public class GameManager : MonoBehaviour
{
    public CircleController circlePrefab;
    public FoodController foodPrefab;
    public GameObject deathScreen;
    public PlayerController playerPrefab;

    public delegate void CallbackDelegate();

    public static event CallbackDelegate OnConnect;
    public static event CallbackDelegate OnSubscriptionApplied;

    public static Color[] colorPalettePlayer = new[]
    {
        //Yellow
        (Color)new Color32(251, 220, 241, 255),
		(Color)new Color32(175, 159, 49, 255),
		(Color)new Color32(175, 116, 49, 255),
        
        //Purple
        (Color)new Color32(112, 47, 252, 255),
		(Color)new Color32(199, 32, 252, 255),
		(Color)new Color32(51, 91, 252, 255),
        
        //Red
        (Color)new Color32(176, 54, 54, 255),
		(Color)new Color32(176, 109, 54, 255),
		(Color)new Color32(141, 43, 99, 255),
        
        //Blue
        (Color)new Color32(2, 188, 250, 255),
		(Color)new Color32(7, 50, 251, 255),
		(Color)new Color32(2, 28, 146, 255),
    };

	public static Color[] colorPaletteFood = new[]
	{
		(Color)new Color32(119, 252, 173, 255),
        (Color)new Color32(76, 250, 146, 255), //4cfa90
		(Color)new Color32(35, 246, 120, 255),

		(Color)new Color32(119, 251, 201, 255),
		(Color)new Color32(76, 249, 184, 255),
		(Color)new Color32(35, 245, 165, 255),
	};

	public static GameManager instance;
    public static Camera localCamera;
    public static Dictionary<uint, PlayerController> playerIdToPlayerController =
        new Dictionary<uint, PlayerController>();

    public static Identity localIdentity = default;
    public static DbConnection conn;

    private void Start()
    {
        instance = this;
        //Application.targetFrameRate = 60;

        // Now that we’ve registered all our callbacks, lets connect to spacetimedb
        conn = DbConnection.Builder().OnConnect((_conn, identity, token) => {
            // Called when we connect to SpacetimeDB and receive our client identity
            Debug.Log("Connected.");
            AuthToken.SaveToken(token);
            localIdentity = identity;
            
            conn.Db.Circle.OnInsert += CircleOnInsert;
            conn.Db.Circle.OnDelete += CircleOnDelete;
            conn.Db.Entity.OnUpdate += EntityOnUpdate;
            conn.Db.Food.OnInsert += FoodOnInsert;
            conn.Db.Player.OnInsert += PlayerOnInsert;
            conn.Db.Player.OnDelete += PlayerOnDelete;

            // Request all tables
            conn.SubscriptionBuilder().OnApplied(ctx =>
            {
                Debug.Log("Subscription applied!");
                OnSubscriptionApplied?.Invoke();
            }).Subscribe("SELECT * FROM *");

            OnConnect?.Invoke();
        }).OnConnectError((ex) =>
        {
            // Called when we have an error connecting to SpacetimeDB
            Debug.LogError($"Connection error: {ex}");
        }).OnDisconnect((_conn, ex) =>
        {
            // Called when we are disconnected from SpacetimeDB
            Debug.Log("Disconnected.");
            if (ex != null)
            {
                Debug.LogException(ex);
            }
        }).WithUri("http://127.0.0.1:3000")
            .WithModuleName("untitled-circle-game")
            // .WithCredentials((localIdentity.Value, PlayerPrefs.GetString(AuthToken.GetTokenKey())))
            .Build();

#pragma warning disable CS0612 // Type or member is obsolete
        conn.onUnhandledReducerError += InstanceOnUnhandledReducerError;
#pragma warning restore CS0612 // Type or member is obsolete

        localCamera = Camera.main;
    }

    private void InstanceOnUnhandledReducerError(ReducerEvent<Reducer> reducerEvent)
    {
        Debug.LogError("There was an error!");
    }

    private void PlayerOnDelete(EventContext context, Player deletedvalue)
    {
        if (playerIdToPlayerController.TryGetValue(deletedvalue.PlayerId, out var playerController))
        {
            Destroy(playerController.gameObject);
        }
    }

    private void PlayerOnInsert(EventContext context, Player insertedPlayer)
    {
        if (insertedPlayer.Identity == localIdentity && !conn.Db.Circle.PlayerId.Filter(insertedPlayer.PlayerId).Any())
        {
            // We have a player, but no circle, let's respawn
            Respawn();
        }
    }

    private void EntityOnUpdate(EventContext context, Entity oldEntity, Entity newEntity)
    {
        var circle = conn.Db.Circle.EntityId.Find(newEntity.EntityId);
        if (circle == null)
        {
            return;
        }

        var player = GetOrCreatePlayer(circle.PlayerId);
        player.CircleUpdate(oldEntity, newEntity);
    }

    private void CircleOnDelete(EventContext context, Circle deletedCircle)
    {
        var player = GetOrCreatePlayer(deletedCircle.PlayerId);
        player.DespawnCircle(deletedCircle);
    }

    private void CircleOnInsert(EventContext context, Circle insertedValue)
    {
        var player = GetOrCreatePlayer(insertedValue.PlayerId);
        // Spawn the new circle
        player.SpawnCircle(insertedValue, circlePrefab);
    }

    PlayerController GetOrCreatePlayer(uint playerId)
    {
        var player = conn.Db.Player.PlayerId.Find(playerId);
        // Get the PlayerController for this circle
        if (!playerIdToPlayerController.TryGetValue(playerId, out var playerController))
        {
            playerController = Instantiate(playerPrefab);
            playerController.name = "PlayerController - " + player.Name;
            playerIdToPlayerController[playerId] = playerController;
            playerController.Spawn(player.Identity);
        }

        return playerController;
    }

    private void FoodOnInsert(EventContext context, Food insertedValue)
    {
        // Spawn the new food
        var food = Instantiate(foodPrefab);
        food.Spawn(insertedValue.EntityId);
	}

	public static Color GetRandomPlayerColor(uint entityId)
	{
		return colorPalettePlayer[entityId % colorPalettePlayer.Length];
	}

	public static Color GetRandomFoodColor(uint entityId)
    {
        return colorPaletteFood[entityId % colorPaletteFood.Length];
    }

    public static float MassToRadius(uint mass)
    {
        return Mathf.Sqrt(mass);
    }

    public void Respawn()
    {
        deathScreen.SetActive(false);
        conn.Reducers.Respawn();
    }

    public void Disconnect()
    {
        conn.Disconnect();
        conn = null;
    }

    public static bool IsConnected()
    {
        return conn != null && conn.IsActive;
    }
}

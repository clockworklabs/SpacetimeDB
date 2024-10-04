using System;
using System.Collections;
using System.Collections.Generic;
using System.Linq;
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
    
    public static Color[] colorPalette = new[]
    {
        (Color)new Color32(248, 72, 245, 255),
        (Color)new Color32(248, 72, 245, 255),
        (Color)new Color32(170, 67, 247, 255),
        (Color)new Color32(62, 223, 56, 255),
        (Color)new Color32(56, 250, 193, 255),
        (Color)new Color32(56, 225, 68, 255),
        (Color)new Color32(39, 229, 245, 255),
        (Color)new Color32(231, 250, 65, 255),
        (Color)new Color32(0, 140, 247, 255),
        (Color)new Color32(48, 53, 244, 255),
        (Color)new Color32(247, 26, 37, 255),
        (Color)new Color32(253, 121, 43, 255),
    };
    
    public static GameManager instance;
    public static Camera localCamera;
    public static Dictionary<uint, PlayerController> playerIdToPlayerController =
        new Dictionary<uint, PlayerController>();

    public static Identity? localIdentity;
    public static DbConnection conn;
    
    private void Start()
    {
        instance = this;
        Application.targetFrameRate = 60;
        
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
            }).Subscribe("SELECT * FROM *");
        }).OnConnectError((status, message) =>
        {
            // Called when we have an error connecting to SpacetimeDB
            Debug.LogError($"Connection error: {status} {message}");
        }).OnDisconnect((_conn, closeStatus, error) =>
        {
            // Called when we are disconnected from SpacetimeDB
            Debug.Log("Disconnected.");
        }).WithUri("http://localhost:3000")
            .WithModuleName("untitled-circle-game")
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
        var circle = conn.Db.Circle.EntityId.Find(newEntity.Id);
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

    public static Color GetRandomColor(uint entityId)
    {
        return colorPalette[entityId % colorPalette.Length];
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
}

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
    [FormerlySerializedAs("playerPrefab")] public CircleController circlePrefab;
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
    
    private void Start()
    {
        instance = this;
        Application.targetFrameRate = 60;

        SpacetimeDBClient.instance.onConnect += () =>
        {
            Debug.Log("Connected.");

            // Request all tables
            SpacetimeDBClient.instance.Subscribe(new List<string>()
            {
                "SELECT * FROM *",
            });
        };

        // Called when we have an error connecting to SpacetimeDB
        SpacetimeDBClient.instance.onConnectError += (error, message) =>
        {
            Debug.LogError($"Connection error: " + message);
        };

        // Called when we are disconnected from SpacetimeDB
        SpacetimeDBClient.instance.onDisconnect += (closeStatus, error) =>
        {
            Debug.Log("Disconnected.");
        };

        // Called when we receive the client identity from SpacetimeDB
        SpacetimeDBClient.instance.onIdentityReceived += (token, identity, address) => {
            AuthToken.SaveToken(token);
            localIdentity = identity;
            Debug.Log("Got identity.");
        };

        Circle.OnInsert += CircleOnInsert;
        Circle.OnDelete += CircleOnDelete;
        Entity.OnUpdate += EntityOnUpdate;
        Food.OnInsert += FoodOnOnInsert;
        Player.OnInsert += PlayerOnInsert;
        Player.OnDelete += PlayerOnDelete;
        
        SpacetimeDBClient.instance.onUnhandledReducerError += InstanceOnUnhandledReducerError;

        // Now that we’ve registered all our callbacks, lets connect to spacetimedb
        SpacetimeDBClient.instance.Connect(AuthToken.Token, "http://localhost:3000", "untitled-circle-game");
        localCamera = Camera.main;
    }

    private void InstanceOnUnhandledReducerError(ReducerEventBase obj)
    {
        Debug.LogError(obj.ErrMessage);
    }
    
    private void PlayerOnDelete(Player deletedvalue, ReducerEvent dbevent)
    {
        if (playerIdToPlayerController.TryGetValue(deletedvalue.PlayerId, out var playerController))
        {
            Destroy(playerController.gameObject);
        }
    }

    private void PlayerOnInsert(Player insertedPlayer, ReducerEvent dbEvent)
    {
        if (insertedPlayer.Identity == localIdentity && !Circle.FilterByPlayerId(insertedPlayer.PlayerId).Any())
        {
            // We have a player, but no circle, let's respawn
            Respawn();
        }    
    }

    private void EntityOnUpdate(Entity oldEntity, Entity newEntity, ReducerEvent dbEvent)
    {
        var circle = Circle.FindByEntityId(newEntity.Id);
        if (circle == null)
        {
            return;
        }
        
        var player = GetOrCreatePlayer(circle.PlayerId);
        player.CircleUpdate(oldEntity, newEntity);
    }

    private void CircleOnDelete(Circle deletedCircle, ReducerEvent dbEvent)
    {
        var player = GetOrCreatePlayer(deletedCircle.PlayerId);
        player.DespawnCircle(deletedCircle);
    }
    
    private void CircleOnInsert(Circle insertedValue, ReducerEvent dbEvent)
    {
        var player = GetOrCreatePlayer(insertedValue.PlayerId);
        // Spawn the new circle 
        player.SpawnCircle(insertedValue, circlePrefab);
    }

    PlayerController GetOrCreatePlayer(uint playerId)
    {
        var player = Player.FindByPlayerId(playerId);
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
    
    private void FoodOnOnInsert(Food insertedValue, ReducerEvent dbEvent)
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
        Reducer.Respawn();
    }
}

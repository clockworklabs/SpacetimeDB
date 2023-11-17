using System;
using System.Collections;
using System.Collections.Generic;
using SpacetimeDB;
using SpacetimeDB.Types;
using UnityEngine;
using Random = UnityEngine.Random;

public class GameManager : MonoBehaviour
{
    public PlayerController playerPrefab;
    public FoodController foodPrefab;
    public GameObject deathScreen;
    
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
            PlayerController.localIdentity = identity;
            Debug.Log("Got identity.");
        };

        Circle.OnInsert += CircleOnInsert;
        Circle.OnDelete += CircleOnDelete;
        Entity.OnUpdate += EntityOnUpdate;
        Food.OnInsert += FoodOnOnInsert;

        // Now that we’ve registered all our callbacks, lets connect to spacetimedb
        SpacetimeDBClient.instance.Connect(AuthToken.Token, "https://testnet.spacetimedb.com", "untitled-circle-game-4");
        localCamera = Camera.main;
    }

    private void EntityOnUpdate(Entity oldEntity, Entity newEntity, ReducerEvent dbEvent)
    {
        if(PlayerController.playersByEntityId.TryGetValue(newEntity.Id, out var player))
        {
            player.UpdatePosition(newEntity);
        }
    }

    private void CircleOnDelete(Circle deletedCircle, ReducerEvent dbEvent)
    {
        // This means we got eaten
        if(PlayerController.playersByEntityId.TryGetValue(deletedCircle.EntityId, out var player))
        {
            // If the local player died, show the death screen
            if (player.IsLocalPlayer())
            {
                deathScreen.SetActive(true);    
            }
            player.Despawn(); 
        }
    }
    
    private void FoodOnOnInsert(Food insertedValue, ReducerEvent dbEvent)
    {
        // Spawn the new food
        var food = Instantiate(foodPrefab);
        food.Spawn(insertedValue.EntityId);
    }

    private void CircleOnInsert(Circle insertedValue, ReducerEvent dbEvent)
    {
        // Spawn the new player
        var player = Instantiate(playerPrefab);
        player.Spawn(insertedValue, insertedValue);
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

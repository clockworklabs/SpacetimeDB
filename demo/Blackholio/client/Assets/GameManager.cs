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

        // Called after our local cache is populated from a Subscribe call
        SpacetimeDBClient.instance.onSubscriptionApplied += () =>
        {
            Reducer.CreatePlayer("" + Random.Range(100000, 999999));
        };

        Circle.OnInsert += CircleOnOnInsert;
        Food.OnInsert += FoodOnOnInsert;

        // Now that we’ve registered all our callbacks, lets connect to spacetimedb
        SpacetimeDBClient.instance.Connect(AuthToken.Token, "localhost:3000", "untitled-circle-game");
        localCamera = Camera.main;
    }

    private void FoodOnOnInsert(Food insertedValue, ReducerEvent dbevent)
    {
        // Spawn the new food
        var food = Instantiate(foodPrefab);
        food.Spawn(insertedValue.EntityId);
    }

    private void CircleOnOnInsert(Circle insertedValue, ReducerEvent dbevent)
    {
        // Spawn the new player
        var player = Instantiate(playerPrefab);
        player.Spawn(insertedValue.CircleId, insertedValue);
    }

    public static Color GetRandomColor(uint entityId)
    {
        return colorPalette[entityId % colorPalette.Length];
    }

    public static float MassToRadius(uint mass)
    {
        return Mathf.Sqrt(mass);
    }
}

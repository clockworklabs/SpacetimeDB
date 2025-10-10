using System;
using System.Collections.Generic;
using System.Linq;
using System.Linq.Expressions;
using SpacetimeDB;
using SpacetimeDB.Types;
using UnityEngine;

public class GameManager : MonoBehaviour
{
    const string SERVER_URL = "http://127.0.0.1:3000";
    const string MODULE_NAME = "blackholio";

    public static event Action OnConnected;
    public static event Action OnSubscriptionApplied;

    public SpriteRenderer backgroundInstance;
    public float borderThickness = 2;
    public Material borderMaterial;
    public ParallaxBackground starBackgroundPrefab;
    public DeathScreen deathScreen;

    public static GameManager Instance { get; private set; }
    public static Identity LocalIdentity { get; private set; }
    public static DbConnection Conn { get; private set; }

    public static Dictionary<int, EntityController> Entities = new Dictionary<int, EntityController>();
    public static Dictionary<int, PlayerController> Players = new Dictionary<int, PlayerController>();

    private void Start()
    {
        // Clear game state in case we've disconnected and reconnected
        Entities.Clear();
        Players.Clear();
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
        // For testing purposes, it is often convenient to comment the following lines out and
        // export an executable for the project using File -> Build Settings.
        // Then, you can run the executable multiple times. Since the executable will not check for
        // a saved auth token, each run of will receive a different Identifier,
        // and their circles will be able to eat each other.
        if (AuthToken.Token != "")
        {
            Debug.Log("Using auth token!");
            builder = builder.WithToken(AuthToken.Token);
        }

        // Building the connection will establish a connection to the SpacetimeDB
        // server.
        Conn = builder.Build();
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
        Conn.SubscriptionBuilder()
            .OnApplied(HandleSubscriptionApplied)
            .SubscribeToAllTables();
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

    private void HandleSubscriptionApplied(SubscriptionEventContext ctx)
    {
        Debug.Log("Subscription applied!");
        OnSubscriptionApplied?.Invoke();

        // Once we have the initial subscription sync'd to the client cache
        // Get the world size from the config table and set up the arena
        var worldSize = Conn.Db.Config.Id.Find(0).WorldSize;
        SetupArena(worldSize);

        // Check to see if we already have a player, if we don't we'll need to create one
        var player = ctx.Db.Player.Identity.Find(LocalIdentity);
        if (string.IsNullOrEmpty(player.Name))
        {
            // The player has to choose a username
            UIUsernameChooser.Instance.Show(true);

            // When our username is updated, hide the username chooser
            Conn.Db.Player.OnUpdate += (_, _, newPlayer) =>
            {
                if (newPlayer.Identity == LocalIdentity)
                {
                    UIUsernameChooser.Instance.Show(false);
                }
            };
        }
        else
        {
            // We already have a player
            if (ctx.Db.Circle.PlayerId.Filter(player.PlayerId).Any())
            {
                // We already have at least one circle, we should just be able to start
                // playing immediately.
                UIUsernameChooser.Instance.Show(false);
            }
            else
            {
                // Create a new circle for our player.
                ctx.Reducers.EnterGame(player.Name);
            }
        }
    }

    private void SetupArena(float worldSize)
    {
        CreateBorderCube(new Vector2(worldSize / 2.0f, worldSize + borderThickness / 2),
            new Vector2(worldSize + borderThickness * 2.0f, borderThickness)); //North
        CreateBorderCube(new Vector2(worldSize / 2.0f, -borderThickness / 2),
            new Vector2(worldSize + borderThickness * 2.0f, borderThickness)); //South
        CreateBorderCube(new Vector2(worldSize + borderThickness / 2, worldSize / 2.0f),
            new Vector2(borderThickness, worldSize + borderThickness * 2.0f)); //East
        CreateBorderCube(new Vector2(-borderThickness / 2, worldSize / 2.0f),
            new Vector2(borderThickness, worldSize + borderThickness * 2.0f)); //West

        backgroundInstance.gameObject.SetActive(true); ;
        var size = worldSize / backgroundInstance.transform.localScale.x;
        backgroundInstance.size = new Vector2(size, size);
        backgroundInstance.transform.position = new Vector3((float)worldSize / 2, (float)worldSize / 2);

        // Set the world size for the camera controller
        CameraController.WorldSize = worldSize;
    }

    private void CreateBorderCube(Vector2 position, Vector2 scale)
    {
        var cube = GameObject.CreatePrimitive(PrimitiveType.Cube);
        cube.name = "Border";
        cube.transform.localScale = new Vector3(scale.x, scale.y, 1);
        cube.transform.position = new Vector3(position.x, position.y, 1);
        cube.GetComponent<MeshRenderer>().material = borderMaterial;
    }

    private static void CircleOnInsert(EventContext context, Circle insertedValue)
    {
        var player = GetOrCreatePlayer(insertedValue.PlayerId);
        var entityController = PrefabManager.SpawnCircle(insertedValue, player);
        Entities.Add(insertedValue.EntityId, entityController);
    }

    private static void EntityOnUpdate(EventContext context, Entity oldEntity, Entity newEntity)
    {
        if (!Entities.TryGetValue(newEntity.EntityId, out var entityController))
        {
            return;
        }
        entityController.OnEntityUpdated(newEntity);
    }

    private static void EntityOnDelete(EventContext context, Entity oldEntity)
    {
        if (Entities.Remove(oldEntity.EntityId, out var entityController))
        {
            entityController.OnDelete(context);
        }
    }

    private static void FoodOnInsert(EventContext context, Food insertedValue)
    {
        var entityController = PrefabManager.SpawnFood(insertedValue);
        Entities.Add(insertedValue.EntityId, entityController);
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

    private static PlayerController GetOrCreatePlayer(int playerId)
    {
        if (!Players.TryGetValue(playerId, out var playerController))
        {
            var player = Conn.Db.Player.PlayerId.Find(playerId);
            playerController = PrefabManager.SpawnPlayer(player);
            Players.Add(playerId, playerController);
        }

        return playerController;
    }

    public static bool IsConnected()
    {
        return Conn != null && Conn.IsActive;
    }

    public void Disconnect()
    {
        Conn.Disconnect();
        Conn = null;
    }

    /* BEGIN: not in tutorial */
    private void InstanceOnUnhandledReducerError(ReducerEvent<Reducer> reducerEvent)
    {
        Debug.LogError($"There was an error!\r\n{(reducerEvent.Status as Status.Failed)?.Failed_}");
    }
    /* END: not in tutorial */
}

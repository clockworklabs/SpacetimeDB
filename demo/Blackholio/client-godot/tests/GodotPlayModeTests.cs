using System;
using System.Linq;
using System.Threading.Tasks;
using Godot;
using SpacetimeDB;
using SpacetimeDB.Types;

public partial class GodotPlayModeTests : Node
{
    private const string ServerUrl = "http://127.0.0.1:3000";
    private const string DatabaseName = "blackholio";
    private const string DefaultPlayerName = "3Blave";

    public override async void _Ready()
    {
        await ToSignal(GetTree(), SceneTree.SignalName.ProcessFrame);
        
        var failures = 0;
        failures += await RunTest(nameof(SimpleConnectionTest), SimpleConnectionTest);
        failures += await RunTest(nameof(CreatePlayerAndTestDecay), CreatePlayerAndTestDecay);
        failures += await RunTest(nameof(OneOffQueryTest), OneOffQueryTest);
        failures += await RunTest(nameof(ReconnectionViaReloadingScene), ReconnectionViaReloadingScene);

        GetTree().Quit(failures == 0 ? 0 : 1);
    }

    private async Task<int> RunTest(string name, Func<Task> test)
    {
        GD.Print($"[GodotTests] START {name}");
        try
        {
            await test();
            GD.Print($"[GodotTests] PASS {name}");
            return 0;
        }
        catch (Exception ex)
        {
            GD.PrintErr($"[GodotTests] FAIL {name}: {ex}");
            return 1;
        }
        finally
        {
            await UnloadMainScene();
        }
    }
    
    private async Task SimpleConnectionTest()
    {
        var connected = false;
        Exception connectError = null;
        var conn = DbConnection.Builder()
            .OnConnect((_, _, _) => connected = true)
            .OnConnectError(ex => connectError = ex)
            .WithUri(ServerUrl)
            .WithDatabaseName(DatabaseName)
            .Build();
    
        STDBUpdateManager.Add(conn);
        try
        {
            await WaitUntil(() => connected || connectError != null, "Connection did not complete.");
            Assert(connectError == null, $"Connection failed: {connectError}");
            Assert(connected, "Connection callback did not run.");
        }
        finally
        {
            STDBUpdateManager.Remove(conn, true);
        }
    }
    
    private async Task CreatePlayerAndTestDecay()
    {
        ClearSavedAuthToken();
        await LoadMainScene();
        await WaitForLocalPlayer();
    
        var player = FindLocalPlayer();
        var circle = FindPlayerCircle(player.PlayerId);
        Assert(circle != null, "Local player circle was not created.");
    
        var foodEaten = 0;
        GameManager.Conn.Db.Food.OnDelete += (_, _) => foodEaten++;
    
        PlayerController.Local.EnableTestInput();
        await WaitUntil(() =>
        {
            SteerTowardNearestFood(circle, foodEaten);
            return foodEaten >= 50;
        }, "Player did not eat enough food.", timeoutSeconds: 60);
    
        PlayerController.Local.SetTestInput(Vector2.Zero);
        var massStart = GameManager.Conn.Db.Entity.EntityId.Find(circle.EntityId).Mass;
        await WaitSeconds(10);
        var massEnd = GameManager.Conn.Db.Entity.EntityId.Find(circle.EntityId).Mass;
        Assert(massEnd < massStart, $"Mass should decay. start={massStart}, end={massEnd}");
    }
    
    private async Task OneOffQueryTest()
    {
        ClearSavedAuthToken();
        await LoadMainScene();
        await WaitForLocalPlayer();
    
        var task = GameManager.Conn.Db.Player.RemoteQuery($"WHERE identity=0x{GameManager.LocalIdentity}");
        Task.Run(() => task.RunSynchronously());
        await WaitUntil(() => task.IsCompleted, "One-off query did not complete.");
    
        var players = task.Result;
        Assert(players.Length == 1, $"Expected one player, found {players.Length}.");
        Assert(players[0].Name == DefaultPlayerName, $"Expected username {DefaultPlayerName}, found {players[0].Name}.");
    }
    
    private async Task ReconnectionViaReloadingScene()
    {
        ClearSavedAuthToken();
        await LoadMainScene();
        await WaitForLocalPlayer();
    
        var player = FindLocalPlayer();
        var circle = FindPlayerCircle(player.PlayerId);
        Assert(circle != null, "Local player circle was not created before reconnect.");
    
        await UnloadMainScene();
    
        await LoadMainScene(clearAuthToken: false);
        await WaitForLocalPlayer();
    
        var newPlayer = FindLocalPlayer();
        var newCircle = FindPlayerCircle(newPlayer.PlayerId);
        Assert(newCircle != null, "Local player circle was not restored after reconnect.");
        Assert(player.PlayerId == newPlayer.PlayerId, "Player ids should match after reconnect.");
        Assert(circle.EntityId == newCircle.EntityId, "Circle entity ids should match after reconnect.");
    }
    
    private async Task LoadMainScene(bool clearAuthToken = true)
    {
        if (clearAuthToken)
        {
            ClearSavedAuthToken();
        }
    
        var connected = false;
        var subscribed = false;
        void OnConnected() => connected = true;
        void OnSubscriptionApplied() => subscribed = true;
    
        GameManager.OnConnected += OnConnected;
        GameManager.OnSubscriptionApplied += OnSubscriptionApplied;
    
        var scene = GD.Load<PackedScene>("res://main.tscn");
        AddChild(scene.Instantiate());
    
        try
        {
            await WaitUntil(() => connected, "GameManager did not connect.");
            await WaitUntil(() => subscribed, "GameManager subscription did not apply.");
            SubmitUsernameIfNeeded();
        }
        finally
        {
            GameManager.OnConnected -= OnConnected;
            GameManager.OnSubscriptionApplied -= OnSubscriptionApplied;
        }
    }
    
    private async Task UnloadMainScene()
    {
        var main = GetNodeOrNull("Main");
        if (main != null)
        {
            main.QueueFree();
            await NextFrame();
        }
    
        if (GameManager.Conn != null)
        {
            await WaitUntil(() => GameManager.Conn == null || !GameManager.IsConnected(), "GameManager did not disconnect.", timeoutSeconds: 5);
        }
    }
    
    private async Task WaitForLocalPlayer()
    {
        await WaitUntil(() =>
        {
            if (GameManager.Conn == null || GameManager.LocalIdentity == default)
            {
                return false;
            }
    
            var player = GameManager.Conn.Db.Player.Identity.Find(GameManager.LocalIdentity);
            return player != null
                && !string.IsNullOrEmpty(player.Name)
                && FindPlayerCircle(player.PlayerId) != null
                && PlayerController.Local != null;
        }, "Local player was not ready.");
    }
    
    private static void SubmitUsernameIfNeeded()
    {
        var player = GameManager.Conn?.Db.Player.Identity.Find(GameManager.LocalIdentity);
        if (player == null || string.IsNullOrEmpty(player.Name))
        {
            HudController.Instance?.SubmitUsernameForTests(DefaultPlayerName);
        }
    }
    
    private static Player FindLocalPlayer() => GameManager.Conn.Db.Player.Identity.Find(GameManager.LocalIdentity);
    
    private static Circle FindPlayerCircle(int playerId) =>
        GameManager.Conn.Db.Circle.PlayerId.Filter(playerId).FirstOrDefault();
    
    private static void SteerTowardNearestFood(Circle circle, int foodEaten)
    {
        var ourEntity = GameManager.Conn.Db.Entity.EntityId.Find(circle.EntityId);
        Assert(ourEntity != null, "Local circle entity was not found.");
    
        var toChosenFood = new Vector2(1000, 0);
        var chosenFoodId = 0;
        foreach (var food in GameManager.Conn.Db.Food.Iter())
        {
            var foodEntity = GameManager.Conn.Db.Entity.EntityId.Find(food.EntityId);
            if (foodEntity == null)
            {
                continue;
            }
    
            var toThisFood = (Vector2)foodEntity.Position - (Vector2)ourEntity.Position;
            if (toThisFood.LengthSquared() == 0.0f)
            {
                continue;
            }
    
            if (toChosenFood.LengthSquared() > toThisFood.LengthSquared())
            {
                chosenFoodId = food.EntityId;
                toChosenFood = toThisFood;
            }
        }
    
        if (chosenFoodId == 0 || GameManager.Conn.Db.Entity.EntityId.Find(chosenFoodId) == null)
        {
            PlayerController.Local.SetTestInput(Vector2.Zero);
            return;
        }
    
        var foodTarget = GameManager.Conn.Db.Entity.EntityId.Find(chosenFoodId);
        var currentEntity = GameManager.Conn.Db.Entity.EntityId.Find(circle.EntityId);
        Assert(foodTarget != null, "Chosen food entity was not found.");
        Assert(currentEntity != null, "Local circle entity was not found.");
    
        var direction = (Vector2)foodTarget.Position - (Vector2)currentEntity.Position;
        if (foodEaten < 10)
        {
            direction = direction.Normalized() * 0.5f;
        }
    
        PlayerController.Local.SetTestInput(direction);
    }
    
    private async Task WaitUntil(Func<bool> predicate, string message, double timeoutSeconds = 30)
    {
        var deadline = Time.GetTicksMsec() + (ulong)(timeoutSeconds * 1000);
        while (!predicate())
        {
            if (Time.GetTicksMsec() >= deadline)
            {
                throw new TimeoutException(message);
            }
    
            await NextFrame();
        }
    }
    
    private async Task WaitSeconds(double seconds)
    {
        var deadline = Time.GetTicksMsec() + (ulong)(seconds * 1000);
        while (Time.GetTicksMsec() < deadline)
        {
            await NextFrame();
        }
    }
    
    private async Task NextFrame() => await ToSignal(GetTree(), SceneTree.SignalName.ProcessFrame);
    
    private static void ClearSavedAuthToken() => AuthToken.SaveToken("");
    
    private static void Assert(bool condition, string message)
    {
        if (!condition)
        {
            throw new Exception(message);
        }
    }
}

using System;
using System.Collections;
using System.Collections.Generic;
using System.Linq;
using System.Net.Sockets;
using System.Text.RegularExpressions;
using System.Threading.Tasks;
using NUnit.Framework;
using SpacetimeDB;
using SpacetimeDB.Types;
using UnityEngine;
using UnityEngine.SceneManagement;
using UnityEngine.TestTools;
using Random = UnityEngine.Random;

public class PlayModeExampleTest
{
    // [UnityTest] - This won't work until we have reconnections
    public IEnumerator SimpleConnectionTest()
    {
        PlayerPrefs.DeleteAll();
        var connected = false;
        var conn = DbConnection.Builder().OnConnect((a, b, c) =>
        {
            connected = true;
        }).OnConnectError((_) =>
        {
            Debug.Assert(false, "Connection failed!");
        }).WithUri("http://127.0.0.1:3000")
            .WithModuleName("blackholio").Build();

        while (!connected)
        {
            conn.FrameTick();
            yield return null;
        }
    }

    [UnityTest]
    public IEnumerator CreatePlayerAndTestDecay()
    {
        var connected = false;
        GameManager.OnConnected += () =>
        {
            Debug.Log("Connected");
            connected = true;
        };

        PlayerPrefs.DeleteAll();
        SceneManager.LoadScene("Scenes/Main");
        while (UIUsernameChooser.Instance == null)
            yield return null;
        var playerCreated = false;

        GameManager.OnConnected += () =>
        {
            Debug.Log("Connected");
            connected = true;
        };

        while (!connected) yield return null;

        GameManager.Conn.Reducers.OnEnterGame += (_, _) =>
        {
            Debug.Log("Player created");
            playerCreated = true;
        };

        UIUsernameChooser.Instance.UsernameInputField.text = "Test " + Random.Range(100000, 999999);
        UIUsernameChooser.Instance.PlayPressed();
        while (!playerCreated) yield return null;

        Debug.Assert(GameManager.LocalIdentity != default, "GameManager.localIdentity != default");
        var player = GameManager.Conn.Db.Player.Identity.Find(GameManager.LocalIdentity);
        Debug.Assert(player != null, nameof(player) + " != null");
        var circle = GameManager.Conn.Db.Circle.Iter().FirstOrDefault(a => a.PlayerId == player.PlayerId);

        var foodEaten = 0;
        GameManager.Conn.Db.Food.OnDelete += (ctx, food) =>
        {
            foodEaten++;
            Debug.Log("Food eaten!");
        };

        // Standing still should decay a bit
        PlayerController.Local.EnableTestInput();
        while (foodEaten < 200)
        {
            Debug.Assert(circle != null, nameof(circle) + " != null");
            var ourEntity = GameManager.Conn.Db.Entity.EntityId.Find(circle.EntityId);
            var toChosenFood = new UnityEngine.Vector2(1000, 0);
            uint chosenFoodId = 0;
            foreach (var food in GameManager.Conn.Db.Food.Iter())
            {
                var thisFoodId = food.EntityId;
                var foodEntity = GameManager.Conn.Db.Entity.EntityId.Find(thisFoodId);
                Debug.Assert(foodEntity != null, nameof(foodEntity) + " != null");
                Debug.Assert(ourEntity != null, nameof(ourEntity) + " != null");
                var foodEntityPosition = foodEntity.Position;
                var ourEntityPosition = ourEntity.Position;
                Debug.Assert(foodEntityPosition != null, nameof(foodEntityPosition) + " != null");
                Debug.Assert(ourEntityPosition != null, nameof(ourEntityPosition) + " != null");
                var toThisFood = (Vector2)foodEntity.Position - (Vector2)ourEntity.Position;
                if (toThisFood.sqrMagnitude == 0.0f) continue;
                if (toChosenFood.sqrMagnitude > toThisFood.sqrMagnitude)
                {
                    chosenFoodId = thisFoodId;
                    toChosenFood = toThisFood;
                }
            }

            if (GameManager.Conn.Db.Entity.EntityId.Find(chosenFoodId) != null)
            {
                var ourNewEntity = GameManager.Conn.Db.Entity.EntityId.Find(circle.EntityId);
                var foodEntity = GameManager.Conn.Db.Entity.EntityId.Find(chosenFoodId);
                Debug.Assert(foodEntity != null, nameof(foodEntity) + " != null");
                Debug.Assert(ourNewEntity != null, nameof(ourNewEntity) + " != null");
                var toThisFood = (Vector2)foodEntity.Position - (Vector2)ourNewEntity.Position;
                PlayerController.Local.SetTestInput(toThisFood);
            }

            yield return null;
        }


        PlayerController.Local.SetTestInput(UnityEngine.Vector2.zero);
        Debug.Assert(circle != null, nameof(circle) + " != null");
        var massStart = GameManager.Conn.Db.Entity.EntityId.Find(circle.EntityId)!.Mass;
        yield return new WaitForSeconds(10);
        var massEnd = GameManager.Conn.Db.Entity.EntityId.Find(circle.EntityId)!.Mass;
        Debug.Assert(massEnd < massStart, "Mass should have decayed");
    }

    // [UnityTest] - This won't work until we have reconnections
    public IEnumerator OneOffTest1()
    {
        var connected = false;
        GameManager.OnConnected += () =>
        {
            Debug.Log("Connected");
            connected = true;
        };

        PlayerPrefs.DeleteAll();
        SceneManager.LoadScene("Scenes/Main");
        while (UIUsernameChooser.Instance == null)
            yield return null;
        var playerCreated = false;

        GameManager.OnConnected += () =>
        {
            Debug.Log("Connected");
            connected = true;
        };

        while (!connected) yield return null;

        GameManager.Conn.Reducers.OnEnterGame += (ctx, username) =>
        {
            Debug.Log("Player created");
            playerCreated = true;
        };

        var name = "Test " + Random.Range(100000, 999999);
        UIUsernameChooser.Instance.UsernameInputField.text = name;
        UIUsernameChooser.Instance.PlayPressed();
        while (!playerCreated) yield return null;

        var task = GameManager.Conn.Db.Player.RemoteQuery(
            $"WHERE identity=0x{GameManager.LocalIdentity}");
        Task.Run(() => task.RunSynchronously());
        while (!task.IsCompleted) yield return null;
        var players = task.Result;
        Debug.Assert(players.Length == 1, "Should have found one player");
        Debug.Assert(players[0].Name == name, "Username should match");
        Debug.Log($"id: {players[0].PlayerId} Username: {players[0].Name}");
    }

    // [UnityTest] - This won't work until we have reconnections
    public IEnumerator OneOffTest2()
    {
        var connected = false;
        GameManager.OnConnected += () =>
        {
            Debug.Log("Connected");
            connected = true;
        };

        PlayerPrefs.DeleteAll();
        SceneManager.LoadScene("Scenes/Main");
        while (UIUsernameChooser.Instance == null)
            yield return null;
        var playerCreated = false;

        GameManager.OnConnected += () =>
        {
            Debug.Log("Connected");
            connected = true;
        };

        while (!connected) yield return null;

        GameManager.Conn.Reducers.OnEnterGame += (ctx, username) =>
        {
            Debug.Log("Player created");
            playerCreated = true;
        };

        var name = "Test " + Random.Range(100000, 999999);
        UIUsernameChooser.Instance.UsernameInputField.text = name;
        UIUsernameChooser.Instance.PlayPressed();
        while (!playerCreated) yield return null;

        var task = GameManager.Conn.Db.Player.RemoteQuery($"WHERE identity=0x{GameManager.LocalIdentity}");
        Task.Run(() => task.RunSynchronously());
        while (!task.IsCompleted) yield return null;
        var players = task.Result;
        Debug.Assert(players.Length == 1, "Should have found one player");
        Debug.Assert(players[0].Name == name, "Username should match");
        Debug.Log($"id: {players[0].PlayerId} Username: {players[0].Name}");
    }

    //[UnityTest]
    public IEnumerator ReconnectionViaReloadingScene()
    {
        var connected = false;
        var subscribed = false;
        GameManager.OnConnected += () =>
        {
            connected = true;
        };
        GameManager.OnSubscriptionApplied += () =>
        {
            subscribed = true;
        };

        Debug.Log("Initial scene load!");
        PlayerPrefs.DeleteAll();
        SceneManager.LoadScene("Scenes/Main");
        while (UIUsernameChooser.Instance == null)
            yield return null;
        var playerCreated = false;

        while (!connected) yield return null;

        GameManager.Conn.Reducers.OnEnterGame += (_, _) =>
        {
            Debug.Log("Player created");
            playerCreated = true;
        };

        var username = "Test " + Random.Range(100000, 999999);
        UIUsernameChooser.Instance.UsernameInputField.text = username;
        UIUsernameChooser.Instance.PlayPressed();
        while (!playerCreated) yield return null;

        Debug.Assert(GameManager.LocalIdentity != default, "GameManager.localIdentity != default");
        var player = GameManager.Conn.Db.Player.Identity.Find(GameManager.LocalIdentity);
        Debug.Assert(player != null, nameof(player) + " != null");
        var circle = GameManager.Conn.Db.Circle.Iter().FirstOrDefault(a => a.PlayerId == player.PlayerId);

        connected = false;
        subscribed = false;
        GameManager.Instance.Disconnect();

        Debug.Log("Second scene load!");
        // Reload
        SceneManager.LoadScene("Scenes/Main");

        while (!connected || !subscribed) yield return null;
        var newPlayer = GameManager.Conn.Db.Player.Identity.Find(GameManager.LocalIdentity);
        Debug.Assert(player.PlayerId == newPlayer.PlayerId, "PlayerIds should match!");
        var newCircle = GameManager.Conn.Db.Circle.Iter().FirstOrDefault(a => a.PlayerId == newPlayer.PlayerId);
        Debug.Assert(circle.EntityId == newCircle.EntityId, "Circle EntityIds should match!");
    }
}

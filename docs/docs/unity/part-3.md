# Unity Tutorial - Blackholio - Part 3 - Gameplay

Need help with the tutorial? [Join our Discord server](https://discord.gg/spacetimedb)!

This progressive tutorial is continued from part 2:

- [Connecting to SpacetimeDB](/docs/unity/part-2)

### Logging Players In

Let's start by adding more functionality to our server module. We're going to modify our module to log in a player when they connect to the database, or to create a new player if they've never connected before.

Let's add a second table to our `Player` struct. Modify the `Player` struct by adding this above the struct:

```rust
#[spacetimedb::table(name = logged_out_player)]
```

Your struct should now look like this:

```rust
#[spacetimedb::table(name = player, public)]
#[spacetimedb::table(name = logged_out_player)]
#[derive(Debug, Clone)]
pub struct Player {
    #[primary_key]
    identity: Identity,
    #[unique]
    #[auto_inc]
    player_id: u32,
    name: String,
}
```

This line creates an additional tabled called `logged_out_player` whose rows share the same `Player` type as in the `player` table.

> IMPORTANT! Note that this new table is not marked `public`. This means that it can only be accessed by the database owner (which is almost always the database creator). In order to prevent any unintended data access, all SpacetimeDB tables are private by default.
>
> If your client isn't syncing rows from the server, check that your table is not accidentally marked private.

Next, modify your `connect` reducer and add a new `disconnect` reducer below it:

```rust
#[spacetimedb::reducer(client_connected)]
pub fn connect(ctx: &ReducerContext) -> Result<(), String> {
    if let Some(player) = ctx.db.logged_out_player().identity().find(&ctx.sender) {
        ctx.db.player().insert(player.clone());
        ctx.db.logged_out_player().delete(player);
    } else {
        ctx.db.player().try_insert(Player {
            identity: ctx.sender,
            player_id: 0,
            name: String::new(),
        })?;
    }
    Ok(())
}

#[spacetimedb::reducer(client_disconnected)]
pub fn disconnect(ctx: &ReducerContext) -> Result<(), String> {
    let player = ctx
        .db
        .player()
        .identity()
        .find(&ctx.sender)
        .ok_or("Player not found")?;
    ctx.db.logged_out_player().insert(player);
    ctx.db.player().identity().delete(&ctx.sender);
    Ok(())
}
```

Now when a client connects, if the player corresponding to the client is in the `logged_out_player` table, we will move them into the `player` table, thus indicating that they are logged in and connected. For any new unrecognized client connects we will create a `Player` and insert it into the `player` table.

When a player disconnects, we will transfer their player row from the `player` table to the `logged_out_player` table to indicate they're offline.

> Note that we could have added a `logged_in` boolean to the `Player` type to indicated whether the player is logged in. There's nothing incorrect about that approach, however for several reasons we recommend this two table approach:
> - We can iterate over all logged in players without any `if` statements or branching
> - The `Player` type now uses less program memory improving cache efficiency
> - We can easily check whether a player is logged in, based on whether their row exists in the `player` table
>
> This approach is more generally referred to as [existence based processing](https://www.dataorienteddesign.com/dodmain/node4.html) and it is a common technique in data-oriented design.






































### Connect to Your SpacetimeDB Module

The Unity SpacetimeDB SDK relies on there being a `NetworkManager` somewhere in the scene. Click on the GameManager object in the scene, and in the inspector, add the `NetworkManager` component.

![Unity-AddNetworkManager](/images/unity-tutorial/Unity-AddNetworkManager.JPG)

Next we are going to connect to our SpacetimeDB module. Open `Assets/_Project/Game/BitcraftMiniGameManager.cs` in your editor of choice and add the following code at the top of the file:

Subscribing to tables tells SpacetimeDB what rows we want in our local client cache. We will also not get row update callbacks or event callbacks for any reducer that does not modify a row that matches at least one of our queries. This means that events can happen on the server and the client won't be notified unless they are subscribed to at least 1 row in the change.

Next we write the `OnSubscriptionApplied` callback. When this event occurs for the first time, it signifies that our local client cache is fully populated. At this point, we can verify if a player entity already exists for the corresponding user. If we do not have a player entity, we need to show the `UserNameChooser` dialog so the user can enter a username. We also put the message of the day into the chat window. Finally we unsubscribe from the callback since we only need to do this once.

**Append after the Start() function in BitcraftMiniGameManager.cs**

```csharp
void OnSubscriptionApplied()
{
    // If we don't have any data for our player, then we are creating a
    // new one. Let's show the username dialog, which will then call the
    // create player reducer
    var player = PlayerComponent.FindByOwnerId(local_identity);
    if (player == null)
    {
       // Show username selection
       UIUsernameChooser.instance.Show();
    }

    // Show the Message of the Day in our Config table of the Client Cache
    UIChatController.instance.OnChatMessageReceived("Message of the Day: " + Config.FindByVersion(0).MessageOfTheDay);

    // Now that we've done this work we can unregister this callback
    SpacetimeDBClient.instance.onSubscriptionApplied -= OnSubscriptionApplied;
}
```

### Adding the Multiplayer Functionality

Now we have to change what happens when you press the "Continue" button in the name dialog window. Instead of calling start game like we did in the single player version, we call the `create_player` reducer on the SpacetimeDB module using the auto-generated code. Open `Assets/_Project/Username/UIUsernameChooser.cs`.

**Append to the top of UIUsernameChooser.cs**

```csharp
using SpacetimeDB.Types;
```

Then we're doing a modification to the `ButtonPressed()` function:

**Modify the ButtonPressed function in UIUsernameChooser.cs**

```csharp
public void ButtonPressed()
{
    CameraController.RemoveDisabler(GetHashCode());
    _panel.SetActive(false);

    // Call the SpacetimeDB CreatePlayer reducer
    Reducer.CreatePlayer(_usernameField.text);
}
```

We need to create a `RemotePlayer` script that we attach to remote player objects. In the same folder as `Assets/_Project/Player/LocalPlayer.cs`, create a new C# script called `RemotePlayer`. In the start function, we will register an OnUpdate callback for the `EntityComponent` and query the local cache to get the player’s initial position. **Make sure you include a `using SpacetimeDB.Types;`** at the top of the file.

First append this using to the top of `RemotePlayer.cs`

**Create file RemotePlayer.cs, then replace its contents:**

```csharp
using System.Collections;
using System.Collections.Generic;
using UnityEngine;
using SpacetimeDB.Types;
using TMPro;

public class RemotePlayer : MonoBehaviour
{
    public ulong EntityId;

    public TMP_Text UsernameElement;

    public string Username { set { UsernameElement.text = value; } }

    void Start()
    {
        // Initialize overhead name
        UsernameElement = GetComponentInChildren<TMP_Text>();
        var canvas = GetComponentInChildren<Canvas>();
        canvas.worldCamera = Camera.main;

        // Get the username from the PlayerComponent for this object and set it in the UI
        PlayerComponent? playerComp = PlayerComponent.FindByEntityId(EntityId);
        if (playerComp is null)
        {
            string inputUsername = UsernameElement.text;
            Debug.Log($"PlayerComponent not found - Creating a new player ({inputUsername})");
            Reducer.CreatePlayer(inputUsername);

            // Try again, optimistically assuming success for simplicity
            PlayerComponent? playerComp = PlayerComponent.FindByEntityId(EntityId);
        }

        Username = playerComp.Username;

        // Get the last location for this player and set the initial position
        EntityComponent entity = EntityComponent.FindByEntityId(EntityId);
        transform.position = new Vector3(entity.Position.X, entity.Position.Y, entity.Position.Z);

        // Register for a callback that is called when the client gets an
        // update for a row in the EntityComponent table
        EntityComponent.OnUpdate += EntityComponent_OnUpdate;
    }
}
```

We now write the `EntityComponent_OnUpdate` callback which sets the movement direction in the `MovementController` for this player. We also set the target position to the current location in the latest update.

**Append to bottom of RemotePlayer class in RemotePlayer.cs:**

```csharp
private void EntityComponent_OnUpdate(EntityComponent oldObj, EntityComponent obj, ReducerEvent callInfo)
{
    // If the update was made to this object
    if(obj.EntityId == EntityId)
    {
        var movementController = GetComponent<PlayerMovementController>();

        // Update target position, rotation, etc.
        movementController.RemoteTargetPosition = new Vector3(obj.Position.X, obj.Position.Y, obj.Position.Z);
        movementController.RemoteTargetRotation = obj.Direction;
        movementController.SetMoving(obj.Moving);
    }
}
```

Next we need to handle what happens when a `PlayerComponent` is added to our local cache. We will handle it differently based on if it’s our local player entity or a remote player. We are going to register for the `OnInsert` event for our `PlayerComponent` table. Add the following code to the `Start` function in `TutorialGameManager`.

**Append to bottom of Start() function in BitcraftMiniGameManager.cs:**

```csharp
PlayerComponent.OnInsert += PlayerComponent_OnInsert;
```

Create the `PlayerComponent_OnInsert` function which does something different depending on if it's the component for the local player or a remote player. If it's the local player, we set the local player object's initial position and call `StartGame`. If it's a remote player, we instantiate a `PlayerPrefab` with the `RemotePlayer` component. The start function of `RemotePlayer` handles initializing the player position.

**Append to bottom of TutorialGameManager class in BitcraftMiniGameManager.cs:**

```csharp
private void PlayerComponent_OnInsert(PlayerComponent obj, ReducerEvent callInfo)
{
    // If the identity of the PlayerComponent matches our user identity then this is the local player
    if(obj.Identity == local_identity)
    {
        // Now that we have our initial position we can start the game
        StartGame();
    }
    else
    {
        // Spawn the player object and attach the RemotePlayer component
        var remotePlayer = Instantiate(PlayerPrefab);

        // Lookup and apply the position for this new player
        var entity = EntityComponent.FindByEntityId(obj.EntityId);
        var position = new Vector3(entity.Position.X, entity.Position.Y, entity.Position.Z);
        remotePlayer.transform.position = position;

        var movementController = remotePlayer.GetComponent<PlayerMovementController>();
        movementController.RemoteTargetPosition = position;
        movementController.RemoteTargetRotation = entity.Direction;

        remotePlayer.AddComponent<RemotePlayer>().EntityId = obj.EntityId;
    }
}
```

Next, we will add a `FixedUpdate()` function to the `LocalPlayer` class so that we can send the local player's position to SpacetimeDB. We will do this by calling the auto-generated reducer function `Reducer.UpdatePlayerPosition(...)`. When we invoke this reducer from the client, a request is sent to SpacetimeDB and the reducer `update_player_position(...)` (Rust) or `UpdatePlayerPosition(...)` (C#) is executed on the server and a transaction is produced. All clients connected to SpacetimeDB will start receiving the results of these transactions.

**Append to the top of LocalPlayer.cs**

```csharp
using SpacetimeDB.Types;
using SpacetimeDB;
```

**Append to the bottom of LocalPlayer class in LocalPlayer.cs**

```csharp
private float? lastUpdateTime;
private void FixedUpdate()
{
   float? deltaTime = Time.time - lastUpdateTime;
   bool hasUpdatedRecently = deltaTime.HasValue && deltaTime.Value < 1.0f / movementUpdateSpeed;
   bool isConnected = SpacetimeDBClient.instance.IsConnected();

   if (hasUpdatedRecently || !isConnected)
   {
      return;
   }

   lastUpdateTime = Time.time;
   var p = PlayerMovementController.Local.GetModelPosition();

   Reducer.UpdatePlayerPosition(new StdbVector3
      {
         X = p.x,
         Y = p.y,
         Z = p.z,
      },
      PlayerMovementController.Local.GetModelRotation(),
      PlayerMovementController.Local.IsMoving());
}
```

Finally, we need to update our connection settings in the inspector for our GameManager object in the scene. Click on the GameManager in the Hierarchy tab. The the inspector tab you should now see fields for `Module Address` and `Host Name`. Set the `Module Address` to the name you used when you ran `spacetime publish`. This is likely `unity-tutorial`. If you don't remember, you can go back to your terminal and run `spacetime publish` again from the `server` folder.

![GameManager-Inspector2](/images/unity-tutorial/GameManager-Inspector2.JPG)

### Play the Game!

Go to File -> Build Settings... Replace the SampleScene with the Main scene we have been working in.

![Unity-AddOpenScenes](/images/unity-tutorial/Unity-AddOpenScenes.JPG)

When you hit the `Build` button, it will kick off a build of the game which will use a different identity than the Unity Editor. Create your character in the build and in the Unity Editor by entering a name and clicking `Continue`. Now you can see each other in game running around the map.

### Implement Player Logout

So far we have not handled the `logged_in` variable of the `PlayerComponent`. This means that remote players will not despawn on your screen when they disconnect. To fix this we need to handle the `OnUpdate` event for the `PlayerComponent` table in addition to `OnInsert`. We are going to use a common function that handles any time the `PlayerComponent` changes.

**Append to the bottom of Start() function in TutorialGameManager.cs**

```csharp
PlayerComponent.OnUpdate += PlayerComponent_OnUpdate;
```

We are going to add a check to determine if the player is logged for remote players. If the player is not logged in, we search for the `RemotePlayer` object with the corresponding `EntityId` and destroy it.

Next we'll be updating some of the code in `PlayerComponent_OnInsert`. For simplicity, just replace the entire function.

**REPLACE PlayerComponent_OnInsert in TutorialGameManager.cs**

```csharp
private void PlayerComponent_OnUpdate(PlayerComponent oldValue, PlayerComponent newValue, ReducerEvent dbEvent)
{
    OnPlayerComponentChanged(newValue);
}

private void PlayerComponent_OnInsert(PlayerComponent obj, ReducerEvent dbEvent)
{
    OnPlayerComponentChanged(obj);
}

private void OnPlayerComponentChanged(PlayerComponent obj)
{
    // If the identity of the PlayerComponent matches our user identity then this is the local player
    if(obj.OwnerId == local_identity)
    {
        // Now that we have our initial position we can start the game
        StartGame();
    }
    else
    {
        // otherwise we need to look for the remote player object in the scene (if it exists) and destroy it
        var existingPlayer = FindObjectsOfType<RemotePlayer>().FirstOrDefault(item => item.EntityId == obj.EntityId);
        if (obj.LoggedIn)
        {
            // Only spawn remote players who aren't already spawned
            if (existingPlayer == null)
            {
                // Spawn the player object and attach the RemotePlayer component
                var remotePlayer = Instantiate(PlayerPrefab);

                // Lookup and apply the position for this new player
                var entity = EntityComponent.FindByEntityId(obj.EntityId);
                var position = new Vector3(entity.Position.X, entity.Position.Y, entity.Position.Z);
                remotePlayer.transform.position = position;

                var movementController = remotePlayer.GetComponent<PlayerMovementController>();
                movementController.RemoteTargetPosition = position;
                movementController.RemoteTargetRotation = entity.Direction;

                remotePlayer.AddComponent<RemotePlayer>().EntityId = obj.EntityId;
            }
        }
        else
        {
            if (existingPlayer != null)
            {
                Destroy(existingPlayer.gameObject);
            }
        }
    }
}
```

Now you when you play the game you should see remote players disappear when they log out.

Before updating the client, let's generate the client files and update publish our module.

**Execute commands in the server/ directory**

```bash
spacetime generate --out-dir ../client/Assets/module_bindings --lang=csharp
spacetime publish -c unity-tutorial
```

On the client, let's add code to send the message when the chat button or enter is pressed. Update the `OnChatButtonPress` function in `UIChatController.cs`.

**Append to the top of UIChatController.cs:**

```csharp
using SpacetimeDB.Types;
```

**REPLACE the OnChatButtonPress function in UIChatController.cs:**

```csharp
public void OnChatButtonPress()
{
    Reducer.SendChatMessage(_chatInput.text);
    _chatInput.text = "";
}
```

Now we need to add a reducer to handle inserting new chat messages. First register for the ChatMessage reducer in the `Start()` function using the auto-generated function:

**Append to the bottom of the Start() function in TutorialGameManager.cs:**

```csharp
Reducer.OnSendChatMessageEvent += OnSendChatMessageEvent;
```

Now we write the `OnSendChatMessageEvent` function. We can find the `PlayerComponent` for the player who sent the message using the `Identity` of the sender. Then we get the `Username` and prepend it to the message before sending it to the chat window.

**Append after the Start() function in TutorialGameManager.cs**

```csharp
private void OnSendChatMessageEvent(ReducerEvent dbEvent, string message)
{
    var player = PlayerComponent.FindByOwnerId(dbEvent.Identity);
    if (player != null)
    {
        UIChatController.instance.OnChatMessageReceived(player.Username + ": " + message);
    }
}
```

Now when you run the game you should be able to send chat messages to other players. Be sure to make a new Unity client build and run it in a separate window so you can test chat between two clients.

## Conclusion

This concludes the SpacetimeDB basic multiplayer tutorial, where we learned how to create a multiplayer game. In the next Unity tutorial, we will add resource nodes to the game and learn about _scheduled_ reducers:

From here, the tutorial continues with more-advanced topics: The [next tutorial](/docs/unity/part-4) introduces Resources & Scheduling.

---

### Troubleshooting

- If you get an error when running the generate command, make sure you have an empty subfolder in your Unity project Assets folder called `module_bindings`

- If you get this exception when running the project:

```
NullReferenceException: Object reference not set to an instance of an object
TutorialGameManager.Start () (at Assets/_Project/Game/TutorialGameManager.cs:26)
```

Check to see if your GameManager object in the Scene has the NetworkManager component attached.

- If you get an error in your Unity console when starting the game, double check your connection settings in the Inspector for the `GameManager` object in the scene.

```
Connection error: Unable to connect to the remote server
```

# Part 1 - Basic Multiplayer

![UnityTutorial-HeroImage](/images/unity-tutorial/UnityTutorial-HeroImage.JPG)

The objective of this tutorial is to help you become acquainted with the basic features of SpacetimeDB. By the end of this tutorial you should have a basic understanding of what SpacetimeDB offers for developers making multiplayer games. It assumes that you have a basic understanding of the Unity Editor, using a command line terminal, and coding.

## Setting up the Tutorial Unity Project

In this section, we will guide you through the process of setting up the Unity Project that will serve as the starting point for our tutorial. By the end of this section, you will have a basic Unity project ready to integrate SpacetimeDB functionality.

### Step 1: Create a Blank Unity Project

1. Open Unity and create a new project by selecting "New" from the Unity Hub or going to **File -> New Project**.

![UnityHub-NewProject](/images/unity-tutorial/UnityHub-NewProject.JPG)

2. Choose a suitable project name and location. For this tutorial, we recommend creating an empty folder for your tutorial project and selecting that as the project location, with the project being named "Client".

This allows you to have a single subfolder that contains both the Unity project in a folder called "Client" and the SpacetimeDB server module in a folder called "Server" which we will create later in this tutorial.

Ensure that you have selected the **3D (URP)** template for this project.

![UnityHub-3DURP](/images/unity-tutorial/UnityHub-3DURP.JPG)

3. Click "Create" to generate the blank project.

### Step 2: Adding Required Packages

To work with SpacetimeDB and ensure compatibility, we need to add some essential packages to our Unity project. Follow these steps:

1. Open the Unity Package Manager by going to **Window -> Package Manager**.
2. In the Package Manager window, select the "Unity Registry" tab to view unity packages.
3. Search for and install the following package:
   - **Input System**: Enables the use of Unity's new Input system used by this project.

![PackageManager-InputSystem](/images/unity-tutorial/PackageManager-InputSystem.JPG)

4. You may need to restart the Unity Editor to switch to the new Input system.

![PackageManager-Restart](/images/unity-tutorial/PackageManager-Restart.JPG)

### Step 3: Importing the Tutorial Package

In this step, we will import the provided Unity tutorial package that contains the basic single-player game setup. Follow these instructions:

1. Download the tutorial package from the releases page on GitHub: [https://github.com/clockworklabs/com.clockworklabs.spacetimedbsdk/releases/latest](https://github.com/clockworklabs/com.clockworklabs.spacetimedbsdk/releases/latest)
2. In Unity, go to **Assets -> Import Package -> Custom Package**.

![Unity-ImportCustomPackageB](/images/unity-tutorial/Unity-ImportCustomPackageB.JPG)

3. Browse and select the downloaded tutorial package file.
4. Unity will prompt you with an import settings dialog. Ensure that all the files are selected and click "Import" to import the package into your project.

![Unity-ImportCustomPackage2](/images/unity-tutorial/Unity-ImportCustomPackage2.JPG)

### Step 4: Running the Project

Now that we have everything set up, let's run the project and see it in action:

1. Open the scene named "Main" in the Scenes folder provided in the project hierarchy by double-clicking it.

![Unity-OpenSceneMain](/images/unity-tutorial/Unity-OpenSceneMain.JPG)

NOTE: When you open the scene you may get a message saying you need to import TMP Essentials. When it appears, click the "Import TMP Essentials" button.

![Unity Import TMP Essentials](/images/unity-tutorial/Unity-ImportTMPEssentials.JPG)

2. Press the **Play** button located at the top of the Unity Editor.

![Unity-Play](/images/unity-tutorial/Unity-Play.JPG)

3. Enter any name and click "Continue"

4. You should see a character loaded in the scene, and you can use the keyboard or mouse controls to move the character around.

Congratulations! You have successfully set up the basic single-player game project. In the next section, we will start integrating SpacetimeDB functionality to enable multiplayer features.

## Writing our SpacetimeDB Server Module

### Step 1: Create the Module

1. It is important that you already have SpacetimeDB [installed](/install).

2. Run the SpacetimeDB standalone using the installed CLI. In your terminal or command window, run the following command:

```bash
spacetime start
```

3. Make sure your CLI is pointed to your local instance of SpacetimeDB. You can do this by running the following command:

```bash
spacetime server set http://localhost:3000
```

4. Open a new command prompt or terminal and navigate to the folder where your Unity project is located using the cd command. For example:

```bash
cd path/to/tutorial_project_folder
```

5. Run the following command to initialize the SpacetimeDB server project with Rust as the language:

```bash
spacetime init --lang=rust ./Server
```

This command creates a new folder named "Server" within your Unity project directory and sets up the SpacetimeDB server project with Rust as the programming language.

### Step 2: SpacetimeDB Tables

1. Using your favorite code editor (we recommend VS Code) open the newly created lib.rs file in the Server folder.
2. Erase everything in the file as we are going to be writing our module from scratch.

---

**Understanding ECS**

ECS is a game development architecture that separates game objects into components for better flexibility and performance. You can read more about the ECS design pattern [here](https://en.wikipedia.org/wiki/Entity_component_system).

We chose ECS for this example project because it promotes scalability, modularity, and efficient data management, making it ideal for building multiplayer games with SpacetimeDB.

---

3. Add the following code to lib.rs.

We are going to start by adding the global `Config` table. Right now it only contains the "message of the day" but it can be extended to store other configuration variables.

You'll notice we have a custom `spacetimedb(table)` attribute that tells SpacetimeDB that this is a SpacetimeDB table. SpacetimeDB automatically generates several functions for us for inserting, updating and querying the table created as a result of this attribute.

The `primarykey` attribute on the version not only ensures uniqueness, preventing duplicate values for the column, but also guides the client to determine whether an operation should be an insert or an update. NOTE: Our `version` column in this `Config` table is always 0. This is a trick we use to store
global variables that can be accessed from anywhere.

We also use the built in rust `derive(Clone)` function to automatically generate a clone function for this struct that we use when updating the row.

```rust
use spacetimedb::{spacetimedb, Identity, SpacetimeType, Timestamp, ReducerContext};
use log;

#[spacetimedb(table)]
#[derive(Clone)]
pub struct Config {
    // Config is a global table with a single row. This table will be used to
    // store configuration or global variables

    #[primarykey]
    // always 0
    // having a table with a primarykey field which is always zero is a way to store singleton global state
    pub version: u32,

    pub message_of_the_day: String,
}

```

The next few tables are all components in the ECS system for our spawnable entities. Spawnable Entities are any objects in the game simulation that can have a world location. In this tutorial we will have only one type of spawnable entity, the Player.

The first component is the `SpawnableEntityComponent` that allows us to access any spawnable entity in the world by its entity_id. The `autoinc` attribute designates an auto-incrementing column in SpacetimeDB, generating sequential values for new entries. When inserting 0 with this attribute, it gets replaced by the next value in the sequence.

```rust
#[spacetimedb(table)]
pub struct SpawnableEntityComponent {
    // All entities that can be spawned in the world will have this component.
    // This allows us to find all objects in the world by iterating through
    // this table. It also ensures that all world objects have a unique
    // entity_id.

    #[primarykey]
    #[autoinc]
    pub entity_id: u64,
}
```

The `PlayerComponent` table connects this entity to a SpacetimeDB identity - a user's "public key." In the context of this tutorial, each user is permitted to have just one Player entity. To guarantee this, we apply the `unique` attribute to the `owner_id` column. If a uniqueness constraint is required on a column aside from the `primarykey`, we make use of the `unique` attribute. This mechanism makes certain that no duplicate values exist within the designated column.

```rust
#[derive(Clone)]
#[spacetimedb(table)]
pub struct PlayerComponent {
    // All players have this component and it associates the spawnable entity
    // with the user's identity. It also stores their username.

    #[primarykey]
    pub entity_id: u64,
    #[unique]
    pub owner_id: Identity,

    // username is provided to the create_player reducer
    pub username: String,
    // this value is updated when the user logs in and out
    pub logged_in: bool,
}
```

The next component, `MobileLocationComponent`, is used to store the last known location and movement direction for spawnable entities that can move smoothly through the world.

Using the `derive(SpacetimeType)` attribute, we define a custom SpacetimeType, StdbVector2, that stores 2D positions. Marking it a `SpacetimeType` allows it to be used in SpacetimeDB columns and reducer calls.

We are also making use of the SpacetimeDB `Timestamp` type for the `move_start_timestamp` column. Timestamps represent the elapsed time since the Unix epoch (January 1, 1970, at 00:00:00 UTC) and are not dependent on any specific timezone.

```rust
#[derive(SpacetimeType, Clone)]
pub struct StdbVector2 {
    // A spacetime type which can be used in tables and reducers to represent
    // a 2d position.
    pub x: f32,
    pub z: f32,
}

impl StdbVector2 {
    // this allows us to use StdbVector2::ZERO in reducers
    pub const ZERO: StdbVector2 = StdbVector2 { x: 0.0, z: 0.0 };
}

#[spacetimedb(table)]
#[derive(Clone)]
pub struct MobileLocationComponent {
    // This component will be created for all world objects that can move
    // smoothly throughout the world. It keeps track of the position the last
    // time the component was updated and the direction the mobile object is
    // currently moving.

    #[primarykey]
    pub entity_id: u64,

    // The last known location of this entity
    pub location: StdbVector2,
    // Movement direction, {0,0} if not moving at all.
    pub direction: StdbVector2,
    // Timestamp when movement started. Timestamp::UNIX_EPOCH if not moving.
    pub move_start_timestamp: Timestamp,
}
```

Next we write our very first reducer, `create_player`. This reducer is called by the client after the user enters a username.

---

**SpacetimeDB Reducers**

"Reducer" is a term coined by SpacetimeDB that "reduces" a single function call into one or more database updates performed within a single transaction. Reducers can be called remotely using a client SDK or they can be scheduled to be called at some future time from another reducer call.

---

The first argument to all reducers is the `ReducerContext`. This struct contains: `sender` the identity of the user that called the reducer and `timestamp` which is the `Timestamp` when the reducer was called.

Before we begin creating the components for the player entity, we pass the sender identity to the auto-generated function `filter_by_owner_id` to see if there is already a player entity associated with this user's identity. Because the `owner_id` column is unique, the `filter_by_owner_id` function returns a `Option<PlayerComponent>` that we can check to see if a matching row exists.

---

**Rust Options**

Rust programs use Option in a similar way to how C#/Unity programs use nullable types. Rust's Option is an enumeration type that represents the possibility of a value being either present (Some) or absent (None), providing a way to handle optional values and avoid null-related errors. For more information, refer to the official Rust documentation: [Rust Option](https://doc.rust-lang.org/std/option/).

---

The first component we create and insert, `SpawnableEntityComponent`, automatically increments the `entity_id` property. When we use the insert function, it returns a result that includes the newly generated `entity_id`. We will utilize this generated `entity_id` in all other components associated with the player entity.

Note the Result that the insert function returns can fail with a "DuplicateRow" error if we insert two rows with the same unique column value. In this example we just use the rust `expect` function to check for this.

---

**Rust Results**

A Result is like an Option where the None is augmented with a value describing the error. Rust programs use Result and return Err in situations where Unity/C# programs would signal an exception. For more information, refer to the official Rust documentation: [Rust Result](https://doc.rust-lang.org/std/result/).

---

We then create and insert our `PlayerComponent` and `MobileLocationComponent` using the same `entity_id`.

We use the log crate to write to the module log. This can be viewed using the CLI command `spacetime logs <module-domain-or-address>`. If you add the -f switch it will continuously tail the log.

```rust
#[spacetimedb(reducer)]
pub fn create_player(ctx: ReducerContext, username: String) -> Result<(), String> {
    // This reducer is called when the user logs in for the first time and
    // enters a username

    let owner_id = ctx.sender;
    // We check to see if there is already a PlayerComponent with this identity.
    // this should never happen because the client only calls it if no player
    // is found.
    if PlayerComponent::filter_by_owner_id(&owner_id).is_some() {
        log::info!("Player already exists");
        return Err("Player already exists".to_string());
    }

    // Next we create the SpawnableEntityComponent. The entity_id for this
    // component automatically increments and we get it back from the result
    // of the insert call and use it for all components.

    let entity_id = SpawnableEntityComponent::insert(SpawnableEntityComponent { entity_id: 0 })
        .expect("Failed to create player spawnable entity component.")
        .entity_id;
    // The PlayerComponent uses the same entity_id and stores the identity of
    // the owner, username, and whether or not they are logged in.
    PlayerComponent::insert(PlayerComponent {
        entity_id,
        owner_id,
        username: username.clone(),
        logged_in: true,
    })
    .expect("Failed to insert player component.");
    // The MobileLocationComponent is used to calculate the current position
    // of an entity that can move smoothly in the world. We are using 2d
    // positions and the client will use the terrain height for the y value.
    MobileLocationComponent::insert(MobileLocationComponent {
        entity_id,
        location: StdbVector2::ZERO,
        direction: StdbVector2::ZERO,
        move_start_timestamp: Timestamp::UNIX_EPOCH,
    })
    .expect("Failed to insert player mobile entity component.");

    log::info!("Player created: {}({})", username, entity_id);

    Ok(())
}
```

SpacetimeDB also gives you the ability to define custom reducers that automatically trigger when certain events occur.

- `init` - Called the very first time you publish your module and anytime you clear the database. We'll learn about publishing a little later.
- `connect` - Called when a user connects to the SpacetimeDB module. Their identity can be found in the `sender` member of the `ReducerContext`.
- `disconnect` - Called when a user disconnects from the SpacetimeDB module.

Next we are going to write a custom `init` reducer that inserts the default message of the day into our `Config` table. The `Config` table only ever contains a single row with version 0, which we retrieve using `Config::filter_by_version(0)`.

```rust
#[spacetimedb(init)]
pub fn init() {
    // Called when the module is initially published


    // Create our global config table.
    Config::insert(Config {
        version: 0,
        message_of_the_day: "Hello, World!".to_string(),
    })
    .expect("Failed to insert config.");
}
```

We use the `connect` and `disconnect` reducers to update the logged in state of the player. The `update_player_login_state` helper function looks up the `PlayerComponent` row using the user's identity and if it exists, it updates the `logged_in` variable and calls the auto-generated `update` function on `PlayerComponent` to update the row.

```rust
#[spacetimedb(connect)]
pub fn client_connected(ctx: ReducerContext) {
    // called when the client connects, we update the logged_in state to true
    update_player_login_state(ctx, true);
}


#[spacetimedb(disconnect)]
pub fn client_disconnected(ctx: ReducerContext) {
    // Called when the client disconnects, we update the logged_in state to false
    update_player_login_state(ctx, false);
}


pub fn update_player_login_state(ctx: ReducerContext, logged_in: bool) {
    // This helper function gets the PlayerComponent, sets the logged
    // in variable and updates the SpacetimeDB table row.
    if let Some(player) = PlayerComponent::filter_by_owner_id(&ctx.sender) {
        let entity_id = player.entity_id;
        // We clone the PlayerComponent so we can edit it and pass it back.
        let mut player = player.clone();
        player.logged_in = logged_in;
        PlayerComponent::update_by_entity_id(&entity_id, player);
    }
}
```

Our final two reducers handle player movement. In `move_player` we look up the `PlayerComponent` using the user identity. If we don't find one, we return an error because the client should not be sending moves without creating a player entity first.

Using the `entity_id` in the `PlayerComponent` we retrieved, we can lookup the `MobileLocationComponent` that stores the entity's locations in the world. We update the values passed in from the client and call the auto-generated `update` function.

---

**Server Validation**

In a fully developed game, the server would typically perform server-side validation on player movements to ensure they comply with game boundaries, rules, and mechanics. This validation, which we omit for simplicity in this tutorial, is essential for maintaining game integrity, preventing cheating, and ensuring a fair gaming experience. Remember to incorporate appropriate server-side validation in your game's development to ensure a secure and fair gameplay environment.

---

```rust
#[spacetimedb(reducer)]
pub fn move_player(
    ctx: ReducerContext,
    start: StdbVector2,
    direction: StdbVector2,
) -> Result<(), String> {
    // Update the MobileLocationComponent with the current movement
    // values. The client will call this regularly as the direction of movement
    // changes. A fully developed game should validate these moves on the server
    // before committing them, but that is beyond the scope of this tutorial.

    let owner_id = ctx.sender;
    // First, look up the player using the sender identity, then use that
    // entity_id to retrieve and update the MobileLocationComponent
    if let Some(player) = PlayerComponent::filter_by_owner_id(&owner_id) {
        if let Some(mut mobile) = MobileLocationComponent::filter_by_entity_id(&player.entity_id) {
            mobile.location = start;
            mobile.direction = direction;
            mobile.move_start_timestamp = ctx.timestamp;
            MobileLocationComponent::update_by_entity_id(&player.entity_id, mobile);


            return Ok(());
        }
    }


    // If we can not find the PlayerComponent for this user something went wrong.
    // This should never happen.
    return Err("Player not found".to_string());
}


#[spacetimedb(reducer)]
pub fn stop_player(ctx: ReducerContext, location: StdbVector2) -> Result<(), String> {
    // Update the MobileLocationComponent when a player comes to a stop. We set
    // the location to the current location and the direction to {0,0}
    let owner_id = ctx.sender;
    if let Some(player) = PlayerComponent::filter_by_owner_id(&owner_id) {
        if let Some(mut mobile) = MobileLocationComponent::filter_by_entity_id(&player.entity_id) {
            mobile.location = location;
            mobile.direction = StdbVector2::ZERO;
            mobile.move_start_timestamp = Timestamp::UNIX_EPOCH;
            MobileLocationComponent::update_by_entity_id(&player.entity_id, mobile);


            return Ok(());
        }
    }


    return Err("Player not found".to_string());
}
```

4. Now that we've written the code for our server module, we need to publish it to SpacetimeDB. This will create the database and call the init reducer. Make sure your domain name is unique. You will get an error if someone has already created a database with that name. In your terminal or command window, run the following commands.

```bash
cd Server

spacetime publish -c yourname-bitcraftmini
```

If you get any errors from this command, double check that you correctly entered everything into lib.rs. You can also look at the Troubleshooting section at the end of this tutorial.

## Updating our Unity Project to use SpacetimeDB

Now we are ready to connect our bitcraft mini project to SpacetimeDB.

### Step 1: Import the SDK and Generate Module Files

1. Add the SpacetimeDB Unity Package using the Package Manager. Open the Package Manager window by clicking on Window -> Package Manager. Click on the + button in the top left corner of the window and select "Add package from git URL". Enter the following URL and click Add.

```bash
https://github.com/clockworklabs/com.clockworklabs.spacetimedbsdk.git
```

![Unity-PackageManager](/images/unity-tutorial/Unity-PackageManager.JPG)

3. The next step is to generate the module specific client files using the SpacetimeDB CLI. The files created by this command provide an interface for retrieving values from the local client cache of the database and for registering for callbacks to events. In your terminal or command window, run the following commands.

```bash
mkdir -p ../Client/Assets/module_bindings

spacetime generate --out-dir ../Client/Assets/module_bindings --lang=csharp
```

### Step 2: Connect to the SpacetimeDB Module

1. The Unity SpacetimeDB SDK relies on there being a `NetworkManager` somewhere in the scene. Click on the GameManager object in the scene, and in the inspector, add the `NetworkManager` component.

![Unity-AddNetworkManager](/images/unity-tutorial/Unity-AddNetworkManager.JPG)

2. Next we are going to connect to our SpacetimeDB module. Open BitcraftMiniGameManager.cs in your editor of choice and add the following code at the top of the file:

`SpacetimeDB.Types` is the namespace that your generated code is in. You can change this by specifying a namespace in the generate command using `--namespace`.

```csharp
using SpacetimeDB;
using SpacetimeDB.Types;
```

3. Inside the class definition add the following members:

```csharp
    // These are connection variables that are exposed on the GameManager
    // inspector. The cloud version of SpacetimeDB needs sslEnabled = true
    [SerializeField] private string moduleAddress = "YOUR_MODULE_DOMAIN_OR_ADDRESS";
    [SerializeField] private string hostName = "localhost:3000";
    [SerializeField] private bool sslEnabled = false;

    // This is the identity for this player that is automatically generated
    // the first time you log in. We set this variable when the
    // onIdentityReceived callback is triggered by the SDK after connecting
    private Identity local_identity;
```

The first three fields will appear in your Inspector so you can update your connection details without editing the code. The `moduleAddress` should be set to the domain you used in the publish command. You should not need to change `hostName` or `sslEnabled` if you are using the standalone version of SpacetimeDB.

4. Add the following code to the `Start` function. **Be sure to remove the line `UIUsernameChooser.instance.Show();`** since we will call this after we get the local state and find that the player for us.

In our `onConnect` callback we are calling `Subscribe` with a list of queries. This tells SpacetimeDB what rows we want in our local client cache. We will also not get row update callbacks or event callbacks for any reducer that does not modify a row that matches these queries.

---

**Local Client Cache**

The "local client cache" is a client-side view of the database, defined by the supplied queries to the Subscribe function. It contains relevant data, allowing efficient access without unnecessary server queries. Accessing data from the client cache is done using the auto-generated iter and filter_by functions for each table, and it ensures that update and event callbacks are limited to the subscribed rows.

---

```csharp
        // When we connect to SpacetimeDB we send our subscription queries
        // to tell SpacetimeDB which tables we want to get updates for.
        SpacetimeDBClient.instance.onConnect += () =>
        {
            Debug.Log("Connected.");

            SpacetimeDBClient.instance.Subscribe(new List<string>()
            {
                "SELECT * FROM Config",
                "SELECT * FROM SpawnableEntityComponent",
                "SELECT * FROM PlayerComponent",
                "SELECT * FROM MobileLocationComponent",
            });
        };

        // called when we have an error connecting to SpacetimeDB
        SpacetimeDBClient.instance.onConnectError += (error, message) =>
        {
            Debug.LogError($"Connection error: " + message);
        };

        // called when we are disconnected from SpacetimeDB
        SpacetimeDBClient.instance.onDisconnect += (closeStatus, error) =>
        {
            Debug.Log("Disconnected.");
        };


        // called when we receive the client identity from SpacetimeDB
        SpacetimeDBClient.instance.onIdentityReceived += (token, identity, address) => {
            AuthToken.SaveToken(token);
            local_identity = identity;
        };


        // called after our local cache is populated from a Subscribe call
        SpacetimeDBClient.instance.onSubscriptionApplied += OnSubscriptionApplied;

        // now that we’ve registered all our callbacks, lets connect to
        // spacetimedb
        SpacetimeDBClient.instance.Connect(AuthToken.Token, hostName, moduleAddress, sslEnabled);
```

5. Next we write the `OnSubscriptionUpdate` callback. When this event occurs for the first time, it signifies that our local client cache is fully populated. At this point, we can verify if a player entity already exists for the corresponding user. If we do not have a player entity, we need to show the `UserNameChooser` dialog so the user can enter a username. We also put the message of the day into the chat window. Finally we unsubscribe from the callback since we only need to do this once.

```csharp
void OnSubscriptionApplied()
{
    // If we don't have any data for our player, then we are creating a
    // new one. Let's show the username dialog, which will then call the
    // create player reducer
    var player = PlayerComponent.FilterByOwnerId(local_identity);
    if (player == null)
    {
       // Show username selection
       UIUsernameChooser.instance.Show();
    }

    // Show the Message of the Day in our Config table of the Client Cache
    UIChatController.instance.OnChatMessageReceived("Message of the Day: " + Config.FilterByVersion(0).MessageOfTheDay);

    // Now that we've done this work we can unregister this callback
    SpacetimeDBClient.instance.onSubscriptionApplied -= OnSubscriptionApplied;
}
```

### Step 3: Adding the Multiplayer Functionality

1. Now we have to change what happens when you press the "Continue" button in the name dialog window. Instead of calling start game like we did in the single player version, we call the `create_player` reducer on the SpacetimeDB module using the auto-generated code. Open `UIUsernameChooser`, **add `using SpacetimeDB.Types;`** at the top of the file, and replace:

```csharp
    LocalPlayer.instance.username = _usernameField.text;
    BitcraftMiniGameManager.instance.StartGame();
```

with:

```csharp
    // Call the SpacetimeDB CreatePlayer reducer
    Reducer.CreatePlayer(_usernameField.text);
```

2. We need to create a `RemotePlayer` component that we attach to remote player objects. In the same folder as `LocalPlayer`, create a new C# script called `RemotePlayer`. In the start function, we will register an OnUpdate callback for the `MobileLocationComponent` and query the local cache to get the player’s initial position. **Make sure you include a `using SpacetimeDB.Types;`** at the top of the file.

```csharp
    public ulong EntityId;

    public TMP_Text UsernameElement;

    public string Username { set { UsernameElement.text = value; } }

    void Start()
    {
        // initialize overhead name
        UsernameElement = GetComponentInChildren<TMP_Text>();
        var canvas = GetComponentInChildren<Canvas>();
        canvas.worldCamera = Camera.main;

        // get the username from the PlayerComponent for this object and set it in the UI
        PlayerComponent playerComp = PlayerComponent.FilterByEntityId(EntityId);
        Username = playerComp.Username;

        // get the last location for this player and set the initial
        // position
        MobileLocationComponent mobPos = MobileLocationComponent.FilterByEntityId(EntityId);
        Vector3 playerPos = new Vector3(mobPos.Location.X, 0.0f, mobPos.Location.Z);
        transform.position = new Vector3(playerPos.x, MathUtil.GetTerrainHeight(playerPos), playerPos.z);

        // register for a callback that is called when the client gets an
        // update for a row in the MobileLocationComponent table
        MobileLocationComponent.OnUpdate += MobileLocationComponent_OnUpdate;
    }
```

3. We now write the `MobileLocationComponent_OnUpdate` callback which sets the movement direction in the `MovementController` for this player. We also set the position to the current location when we stop moving (`DirectionVec` is zero)

```csharp
    private void MobileLocationComponent_OnUpdate(MobileLocationComponent oldObj, MobileLocationComponent obj, ReducerEvent callInfo)
    {
        // if the update was made to this object
        if(obj.EntityId == EntityId)
        {
            // update the DirectionVec in the PlayerMovementController component with the updated values
            var movementController = GetComponent<PlayerMovementController>();
            movementController.DirectionVec = new Vector3(obj.Direction.X, 0.0f, obj.Direction.Z);
            // if DirectionVec is {0,0,0} then we came to a stop so correct our position to match the server
            if (movementController.DirectionVec == Vector3.zero)
            {
                Vector3 playerPos = new Vector3(obj.Location.X, 0.0f, obj.Location.Z);
                transform.position = new Vector3(playerPos.x, MathUtil.GetTerrainHeight(playerPos), playerPos.z);
            }
        }
    }
```

4. Next we need to handle what happens when a `PlayerComponent` is added to our local cache. We will handle it differently based on if it’s our local player entity or a remote player. We are going to register for the `OnInsert` event for our `PlayerComponent` table. Add the following code to the `Start` function in `BitcraftMiniGameManager`.

```csharp
    PlayerComponent.OnInsert += PlayerComponent_OnInsert;
```

5. Create the `PlayerComponent_OnInsert` function which does something different depending on if it's the component for the local player or a remote player. If it's the local player, we set the local player object's initial position and call `StartGame`. If it's a remote player, we instantiate a `PlayerPrefab` with the `RemotePlayer` component. The start function of `RemotePlayer` handles initializing the player position.

```csharp
    private void PlayerComponent_OnInsert(PlayerComponent obj, ReducerEvent callInfo)
    {
        // if the identity of the PlayerComponent matches our user identity then this is the local player
        if(obj.OwnerId == local_identity)
        {
            // Set the local player username
            LocalPlayer.instance.Username = obj.Username;

            // Get the MobileLocationComponent for this object and update the position to match the server
            MobileLocationComponent mobPos = MobileLocationComponent.FilterByEntityId(obj.EntityId);
            Vector3 playerPos = new Vector3(mobPos.Location.X, 0.0f, mobPos.Location.Z);
            LocalPlayer.instance.transform.position = new Vector3(playerPos.x, MathUtil.GetTerrainHeight(playerPos), playerPos.z);

            // Now that we have our initial position we can start the game
            StartGame();
        }
        // otherwise this is a remote player
        else
        {
            // spawn the player object and attach the RemotePlayer component
            var remotePlayer = Instantiate(PlayerPrefab);
            remotePlayer.AddComponent<RemotePlayer>().EntityId = obj.EntityId;
        }
    }
```

6. Next, we need to update the `FixedUpdate` function in `LocalPlayer` to call the `move_player` and `stop_player` reducers using the auto-generated functions. **Don’t forget to add `using SpacetimeDB.Types;`** to LocalPlayer.cs

```csharp
    private Vector3? lastUpdateDirection;

    private void FixedUpdate()
    {
        var directionVec = GetDirectionVec();
        PlayerMovementController.Local.DirectionVec = directionVec;

        // first get the position of the player
        var ourPos = PlayerMovementController.Local.GetModelTransform().position;
        // if we are moving , and we haven't updated our destination yet, or we've moved more than .1 units, update our destination
        if (directionVec.sqrMagnitude != 0 && (!lastUpdateDirection.HasValue || (directionVec - lastUpdateDirection.Value).sqrMagnitude > .1f))
        {
            Reducer.MovePlayer(new StdbVector2() { X = ourPos.x, Z = ourPos.z }, new StdbVector2() { X = directionVec.x, Z = directionVec.z });
            lastUpdateDirection = directionVec;
        }
        // if we stopped moving, send the update
        else if(directionVec.sqrMagnitude == 0 && lastUpdateDirection != null)
        {
            Reducer.StopPlayer(new StdbVector2() { X = ourPos.x, Z = ourPos.z });
            lastUpdateDirection = null;
        }
    }
```

7. Finally, we need to update our connection settings in the inspector for our GameManager object in the scene. Click on the GameManager in the Hierarchy tab. The the inspector tab you should now see fields for `Module Address`, `Host Name` and `SSL Enabled`. Set the `Module Address` to the name you used when you ran `spacetime publish`. If you don't remember, you can go back to your terminal and run `spacetime publish` again from the `Server` folder.

![GameManager-Inspector2](/images/unity-tutorial/GameManager-Inspector2.JPG)

### Step 4: Play the Game!

1. Go to File -> Build Settings... Replace the SampleScene with the Main scene we have been working in.

![Unity-AddOpenScenes](/images/unity-tutorial/Unity-AddOpenScenes.JPG)

When you hit the `Build` button, it will kick off a build of the game which will use a different identity than the Unity Editor. Create your character in the build and in the Unity Editor by entering a name and clicking `Continue`. Now you can see each other in game running around the map.

### Step 5: Implement Player Logout

So far we have not handled the `logged_in` variable of the `PlayerComponent`. This means that remote players will not despawn on your screen when they disconnect. To fix this we need to handle the `OnUpdate` event for the `PlayerComponent` table in addition to `OnInsert`. We are going to use a common function that handles any time the `PlayerComponent` changes.

1. Open `BitcraftMiniGameManager.cs` and add the following code to the `Start` function:

```csharp
    PlayerComponent.OnUpdate += PlayerComponent_OnUpdate;
```

2. We are going to add a check to determine if the player is logged for remote players. If the player is not logged in, we search for the RemotePlayer object with the corresponding `EntityId` and destroy it. Add `using System.Linq;` to the top of the file and replace the `PlayerComponent_OnInsert` function with the following code.

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
        // if the identity of the PlayerComponent matches our user identity then this is the local player
        if (obj.OwnerId == local_identity)
        {
            // Set the local player username
            LocalPlayer.instance.Username = obj.Username;

            // Get the MobileLocationComponent for this object and update the position to match the server
            MobileLocationComponent mobPos = MobileLocationComponent.FilterByEntityId(obj.EntityId);
            Vector3 playerPos = new Vector3(mobPos.Location.X, 0.0f, mobPos.Location.Z);
            LocalPlayer.instance.transform.position = new Vector3(playerPos.x, MathUtil.GetTerrainHeight(playerPos), playerPos.z);

            // Now that we have our initial position we can start the game
            StartGame();
        }
        // otherwise this is a remote player
        else
        {
            // if the remote player is logged in, spawn it
            if (obj.LoggedIn)
            {
                // spawn the player object and attach the RemotePlayer component
                var remotePlayer = Instantiate(PlayerPrefab);
                remotePlayer.AddComponent<RemotePlayer>().EntityId = obj.EntityId;
            }
            // otherwise we need to look for the remote player object in the scene (if it exists) and destroy it
            else
            {
                var remotePlayer = FindObjectsOfType<RemotePlayer>().FirstOrDefault(item => item.EntityId == obj.EntityId);
                if (remotePlayer != null)
                {
                    Destroy(remotePlayer.gameObject);
                }
            }
        }
    }
```

3. Now you when you play the game you should see remote players disappear when they log out.

### Step 6: Add Chat Support

The project has a chat window but so far all it's used for is the message of the day. We are going to add the ability for players to send chat messages to each other.

1. First lets add a new `ChatMessage` table to the SpacetimeDB module. Add the following code to lib.rs.

```rust
#[spacetimedb(table)]
pub struct ChatMessage {
    // The primary key for this table will be auto-incremented
    #[primarykey]
    #[autoinc]
    pub chat_entity_id: u64,

    // The entity id of the player (or NPC) that sent the message
    pub source_entity_id: u64,
    // Message contents
    pub chat_text: String,
    // Timestamp of when the message was sent
    pub timestamp: Timestamp,
}
```

2. Now we need to add a reducer to handle inserting new chat messages. Add the following code to lib.rs.

```rust
#[spacetimedb(reducer)]
pub fn chat_message(ctx: ReducerContext, message: String) -> Result<(), String> {
    // Add a chat entry to the ChatMessage table

    // Get the player component based on the sender identity
    let owner_id = ctx.sender;
    if let Some(player) = PlayerComponent::filter_by_owner_id(&owner_id) {
        // Now that we have the player we can insert the chat message using the player entity id.
        ChatMessage::insert(ChatMessage {
            // this column auto-increments so we can set it to 0
            chat_entity_id: 0,
            source_entity_id: player.entity_id,
            chat_text: message,
            timestamp: ctx.timestamp,
        })
        .unwrap();

        return Ok(());
    }

    Err("Player not found".into())
}
```

3. Before updating the client, let's generate the client files and publish our module.

```bash
spacetime generate --out-dir ../Client/Assets/module_bindings --lang=csharp

spacetime publish -c yourname-bitcraftmini
```

4. On the client, let's add code to send the message when the chat button or enter is pressed. Update the `OnChatButtonPress` function in `UIChatController.cs`.

```csharp
public void OnChatButtonPress()
{
    Reducer.ChatMessage(_chatInput.text);
    _chatInput.text = "";
}
```

5. Next let's add the `ChatMessage` table to our list of subscriptions.

```csharp
            SpacetimeDBClient.instance.Subscribe(new List<string>()
            {
                "SELECT * FROM Config",
                "SELECT * FROM SpawnableEntityComponent",
                "SELECT * FROM PlayerComponent",
                "SELECT * FROM MobileLocationComponent",
                "SELECT * FROM ChatMessage",
            });
```

6. Now we need to add a reducer to handle inserting new chat messages. First register for the ChatMessage reducer in the `Start` function using the auto-generated function:

```csharp
        Reducer.OnChatMessageEvent += OnChatMessageEvent;
```

Then we write the `OnChatMessageEvent` function. We can find the `PlayerComponent` for the player who sent the message using the `Identity` of the sender. Then we get the `Username` and prepend it to the message before sending it to the chat window.

```csharp
    private void OnChatMessageEvent(ReducerEvent dbEvent, string message)
    {
        var player = PlayerComponent.FilterByOwnerId(dbEvent.Identity);
        if (player != null)
        {
            UIChatController.instance.OnChatMessageReceived(player.Username + ": " + message);
        }
    }
```

7. Now when you run the game you should be able to send chat messages to other players. Be sure to make a new Unity client build and run it in a separate window so you can test chat between two clients.

## Conclusion

This concludes the first part of the tutorial. We've learned about the basics of SpacetimeDB and how to use it to create a multiplayer game. In the next part of the tutorial we will add resource nodes to the game and learn about scheduled reducers.

---

### Troubleshooting

- If you get an error when running the generate command, make sure you have an empty subfolder in your Unity project Assets folder called `module_bindings`

- If you get this exception when running the project:

```
NullReferenceException: Object reference not set to an instance of an object
BitcraftMiniGameManager.Start () (at Assets/_Project/Game/BitcraftMiniGameManager.cs:26)
```

Check to see if your GameManager object in the Scene has the NetworkManager component attached.

- If you get an error in your Unity console when starting the game, double check your connection settings in the Inspector for the `GameManager` object in the scene.

```
Connection error: Unable to connect to the remote server
```

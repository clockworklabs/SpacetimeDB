---
title: 3 - Gameplay
slug: /unreal/part-3
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';

# Gameplay

Need help with the tutorial? [Join our Discord server](https://discord.gg/spacetimedb)!

This progressive tutorial is continued from [part 2](/unreal/part-2).

### Spawning Food

<Tabs groupId="server-language" defaultValue="rust">
<TabItem value="rust" label="Rust">
Let's start by spawning food into the map. The first thing we need to do is create a new, special reducer called the `init` reducer. SpacetimeDB calls the `init` reducer automatically when first publish your module, and also after any time you run with `publish --delete-data`. It gives you an opportunity to initialize the state of your database before any clients connect.

Add this new reducer above our `connect` reducer.

```rust
// Note the `init` parameter passed to the reducer macro.
// That indicates to SpacetimeDB that it should be called
// once upon database creation.
#[spacetimedb::reducer(init)]
pub fn init(ctx: &ReducerContext) -> Result<(), String> {
    log::info!("Initializing...");
    ctx.db.config().try_insert(Config {
        id: 0,
        world_size: 1000,
    })?;
    Ok(())
}
```

This reducer also demonstrates how to insert new rows into a table. Here we are adding a single `Config` row to the `config` table with the `try_insert` function. `try_insert` returns an error if inserting the row into the table would violate any constraints, like unique constraints, on the table. You can also use `insert` which panics on constraint violations if you know for sure that you will not violate any constraints.

Now that we've ensured that our database always has a valid `world_size` let's spawn some food into the map. Add the following code to the end of the file.

```rust
const FOOD_MASS_MIN: i32 = 2;
const FOOD_MASS_MAX: i32 = 4;
const TARGET_FOOD_COUNT: usize = 600;

fn mass_to_radius(mass: i32) -> f32 {
    (mass as f32).sqrt()
}

#[spacetimedb::reducer]
pub fn spawn_food(ctx: &ReducerContext) -> Result<(), String> {
    if ctx.db.player().count() == 0 {
        // Are there no logged in players? Skip food spawn.
        return Ok(());
    }

    let world_size = ctx
        .db
        .config()
        .id()
        .find(0)
        .ok_or("Config not found")?
        .world_size;

    let mut rng = ctx.rng();
    let mut food_count = ctx.db.food().count();
    while food_count < TARGET_FOOD_COUNT as u64 {
        let food_mass = rng.gen_range(FOOD_MASS_MIN..FOOD_MASS_MAX);
        let food_radius = mass_to_radius(food_mass);
        let x = rng.gen_range(food_radius..world_size as f32 - food_radius);
        let y = rng.gen_range(food_radius..world_size as f32 - food_radius);
        let entity = ctx.db.entity().try_insert(Entity {
            entity_id: 0,
            position: DbVector2 { x, y },
            mass: food_mass,
        })?;
        ctx.db.food().try_insert(Food {
            entity_id: entity.entity_id,
        })?;
        food_count += 1;
        log::info!("Spawned food! {}", entity.entity_id);
    }

    Ok(())
}
```

In this reducer, we are using the `world_size` we configured along with the `ReducerContext`'s random number generator `.rng()` function to place 600 food uniformly randomly throughout the map. We've also chosen the `mass` of the food to be a random number between 2 and 4 inclusive.

</TabItem>
<TabItem value="csharp" label="C#">
Let's start by spawning food into the map. The first thing we need to do is create a new, special reducer called the `Init` reducer. SpacetimeDB calls the `Init` reducer automatically when you first publish your module, and also after any time you run with `publish --delete-data`. It gives you an opportunity to initialize the state of your database before any clients connect.

Add this new reducer above our `Connect` reducer.

```csharp
// Note the `init` parameter passed to the reducer macro.
// That indicates to SpacetimeDB that it should be called
// once upon database creation.
[Reducer(ReducerKind.Init)]
public static void Init(ReducerContext ctx)
{
    Log.Info($"Initializing...");
    ctx.Db.config.Insert(new Config { world_size = 1000 });
}
```

This reducer also demonstrates how to insert new rows into a table. Here we are adding a single `Config` row to the `config` table with the `Insert` function.

Now that we've ensured that our database always has a valid `world_size` let's spawn some food into the map. Add the following code to the end of the `Module` class.

```csharp
const int FOOD_MASS_MIN = 2;
const int FOOD_MASS_MAX = 4;
const int TARGET_FOOD_COUNT = 600;

public static float MassToRadius(int mass) => MathF.Sqrt(mass);

[Reducer]
public static void SpawnFood(ReducerContext ctx)
{
    if (ctx.Db.player.Count == 0) //Are there no players yet?
    {
        return;
    }

    var world_size = (ctx.Db.config.id.Find(0) ?? throw new Exception("Config not found")).world_size;
    var rng = ctx.Rng;
    var food_count = ctx.Db.food.Count;
    while (food_count < TARGET_FOOD_COUNT)
    {
        var food_mass = rng.Range(FOOD_MASS_MIN, FOOD_MASS_MAX);
        var food_radius = MassToRadius(food_mass);
        var x = rng.Range(food_radius, world_size - food_radius);
        var y = rng.Range(food_radius, world_size - food_radius);
        var entity = ctx.Db.entity.Insert(new Entity()
        {
            position = new DbVector2(x, y),
            mass = food_mass,
        });
        ctx.Db.food.Insert(new Food
        {
            entity_id = entity.entity_id,
        });
        food_count++;
        Log.Info($"Spawned food! {entity.entity_id}");
    }
}

public static float Range(this Random rng, float min, float max) => rng.NextSingle() * (max - min) + min;

public static int Range(this Random rng, int min, int max) => (int)rng.NextInt64(min, max);
```

In this reducer, we are using the `world_size` we configured along with the `ReducerContext`'s random number generator `.rng()` function to place 600 food uniformly randomly throughout the map. We've also chosen the `mass` of the food to be a random number between 2 and 4 inclusive.

We also added two helper functions so we can get a random range as either a `int` or a `float`.

</TabItem>
</Tabs>

Although, we've written the reducer to spawn food, no food will actually be spawned until we call the function while players are logged in. This raises the question, who should call this function and when?

We would like for this function to be called periodically to "top up" the amount of food on the map so that it never falls very far below our target amount of food. SpacetimeDB has built in functionality for exactly this. With SpacetimeDB you can schedule your module to call itself in the future or repeatedly with reducers.

<Tabs groupId="server-language" defaultValue="rust">
<TabItem value="rust" label="Rust">
In order to schedule a reducer to be called we have to create a new table which specifies when and how a reducer should be called. Add this new table to the top of the file, below your imports.

```rust
#[spacetimedb::table(name = spawn_food_timer, scheduled(spawn_food))]
pub struct SpawnFoodTimer {
    #[primary_key]
    #[auto_inc]
    scheduled_id: u64,
    scheduled_at: spacetimedb::ScheduleAt,
}
```

Note the `scheduled(spawn_food)` parameter in the table macro. This tells SpacetimeDB that the rows in this table specify a schedule for when the `spawn_food` reducer should be called. Each scheduled table requires a `scheduled_id` and a `scheduled_at` field so that SpacetimeDB can call your reducer, however you can also add your own fields to these rows as well.
</TabItem>
<TabItem value="csharp" label="C#">
In order to schedule a reducer to be called we have to create a new table which specifies when an how a reducer should be called. Add this new table to the top of the `Module` class.

```csharp
[Table(Name = "spawn_food_timer", Scheduled = nameof(SpawnFood), ScheduledAt = nameof(scheduled_at))]
public partial struct SpawnFoodTimer
{
    [PrimaryKey, AutoInc]
    public ulong scheduled_id;
    public ScheduleAt scheduled_at;
}
```

Note the `Scheduled = nameof(SpawnFood)` parameter in the table macro. This tells SpacetimeDB that the rows in this table specify a schedule for when the `SpawnFood` reducer should be called. Each scheduled table requires a `scheduled_id` and a `scheduled_at` field so that SpacetimeDB can call your reducer, however you can also add your own fields to these rows as well.
</TabItem>
</Tabs>

You can create, delete, or change a schedule by inserting, deleting, or updating rows in this table.

You will see an error telling you that the `spawn_food` reducer needs to take two arguments, but currently only takes one. This is because the schedule row must be passed in to all scheduled reducers. Modify your `spawn_food` reducer to take the scheduled row as an argument.

<Tabs groupId="server-language" defaultValue="rust">
<TabItem value="rust" label="Rust">

```rust
#[spacetimedb::reducer]
pub fn spawn_food(ctx: &ReducerContext, _timer: SpawnFoodTimer) -> Result<(), String> {
    // ...
}
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
[Reducer]
public static void SpawnFood(ReducerContext ctx, SpawnFoodTimer _timer)
{
    // ...
}
```

</TabItem>
</Tabs>

In our case we aren't interested in the data on the row, so we name the argument `_timer`.

<Tabs groupId="server-language" defaultValue="rust">
<TabItem value="rust" label="Rust">
Let's modify our `init` reducer to schedule our `spawn_food` reducer to be called every 500 milliseconds.

```rust
#[spacetimedb::reducer(init)]
pub fn init(ctx: &ReducerContext) -> Result<(), String> {
    log::info!("Initializing...");
    ctx.db.config().try_insert(Config {
        id: 0,
        world_size: 1000,
    })?;
    ctx.db.spawn_food_timer().try_insert(SpawnFoodTimer {
        scheduled_id: 0,
        scheduled_at: ScheduleAt::Interval(Duration::from_millis(500).into()),
    })?;
    Ok(())
}
```

:::note
You can use `ScheduleAt::Interval` to schedule a reducer call at an interval like we're doing here. SpacetimeDB will continue to call the reducer at this interval until you remove the row. You can also use `ScheduleAt::Time()` to specify a specific at which to call a reducer once. SpacetimeDB will remove that row automatically after the reducer has been called.
:::

</TabItem>
<TabItem value="csharp" label="C#">
Let's modify our `Init` reducer to schedule our `SpawnFood` reducer to be called every 500 milliseconds.

```csharp
[Reducer(ReducerKind.Init)]
public static void Init(ReducerContext ctx)
{
    Log.Info($"Initializing...");
    ctx.Db.config.Insert(new Config { world_size = 1000 });
    ctx.Db.spawn_food_timer.Insert(new SpawnFoodTimer
    {
        scheduled_at = new ScheduleAt.Interval(TimeSpan.FromMilliseconds(500))
    });
}
```

:::note
You can use `ScheduleAt.Interval` to schedule a reducer call at an interval like we're doing here. SpacetimeDB will continue to call the reducer at this interval until you remove the row. You can also use `ScheduleAt.Time()` to specify a specific at which to call a reducer once. SpacetimeDB will remove that row automatically after the reducer has been called.
:::

</TabItem>
</Tabs>

### Logging Players In

Let's continue building out our server module by modifying it to log in a player when they connect to the database, or to create a new player if they've never connected before.

Let's add a second table to our `Player` struct. Modify the `Player` struct by adding this above the struct:

<Tabs groupId="server-language" defaultValue="rust">
<TabItem value="rust" label="Rust">

```rust
#[spacetimedb::table(name = logged_out_player)]
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
[Table(Name = "logged_out_player")]
```

</TabItem>
</Tabs>

Your struct should now look like this:

<Tabs groupId="server-language" defaultValue="rust">
<TabItem value="rust" label="Rust">

```rust
#[spacetimedb::table(name = player, public)]
#[spacetimedb::table(name = logged_out_player)]
#[derive(Debug, Clone)]
pub struct Player {
    #[primary_key]
    identity: Identity,
    #[unique]
    #[auto_inc]
    player_id: i32,
    name: String,
}
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
[Table(Name = "player", Public = true)]
[Table(Name = "logged_out_player")]
public partial struct Player
{
    [PrimaryKey]
    public Identity identity;
    [Unique, AutoInc]
    public int player_id;
    public string name;
}
```

</TabItem>
</Tabs>

This line creates an additional tabled called `logged_out_player` whose rows share the same `Player` type as in the `player` table.

:::danger
Note that this new table is not marked `public`. This means that it can only be accessed by the database owner (which is almost always the database creator). In order to prevent any unintended data access, all SpacetimeDB tables are private by default.

If your client isn't syncing rows from the server, check that your table is not accidentally marked private.
:::

<Tabs groupId="server-language" defaultValue="rust">
<TabItem value="rust" label="Rust">
Next, modify your `connect` reducer and add a new `disconnect` reducer below it:

```rust
#[spacetimedb::reducer(client_connected)]
pub fn connect(ctx: &ReducerContext) -> Result<(), String> {
    if let Some(player) = ctx.db.logged_out_player().identity().find(&ctx.sender) {
        ctx.db.player().insert(player.clone());
        ctx.db
            .logged_out_player()
            .identity()
            .delete(&player.identity);
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
    let player_id = player.player_id;
    ctx.db.logged_out_player().insert(player);
    ctx.db.player().identity().delete(&ctx.sender);

    Ok(())
}
```

</TabItem>
<TabItem value="csharp" label="C#">

Next, modify your `Connect` reducer and add a new `Disconnect` reducer below it:

```csharp
[Reducer(ReducerKind.ClientConnected)]
public static void Connect(ReducerContext ctx)
{
    var player = ctx.Db.logged_out_player.identity.Find(ctx.Sender);
    if (player != null)
    {
        ctx.Db.player.Insert(player.Value);
        ctx.Db.logged_out_player.identity.Delete(player.Value.identity);
    }
    else
    {
        ctx.Db.player.Insert(new Player
        {
            identity = ctx.Sender,
            name = "",
        });
    }
}

[Reducer(ReducerKind.ClientDisconnected)]
public static void Disconnect(ReducerContext ctx)
{
    var player = ctx.Db.player.identity.Find(ctx.Sender) ?? throw new Exception("Player not found");
    ctx.Db.logged_out_player.Insert(player);
    ctx.Db.player.identity.Delete(player.identity);
}
```

</TabItem>
</Tabs>

Now when a client connects, if the player corresponding to the client is in the `logged_out_player` table, we will move them into the `player` table, thus indicating that they are logged in and connected. For any new unrecognized client connects we will create a `Player` and insert it into the `player` table.

When a player disconnects, we will transfer their player row from the `player` table to the `logged_out_player` table to indicate they're offline.

> Note that we could have added a `logged_in` boolean to the `Player` type to indicated whether the player is logged in. There's nothing incorrect about that approach, however for several reasons we recommend this two table approach:
>
> - We can iterate over all logged in players without any `if` statements or branching
> - The `Player` type now uses less program memory improving cache efficiency
> - We can easily check whether a player is logged in, based on whether their row exists in the `player` table
>
> This approach is more generally referred to as [existence based processing](https://www.dataorienteddesign.com/dodmain/node4.html) and it is a common technique in data-oriented design.

### Spawning Player Circles

Now that we've got our food spawning and our players set up, let's create a match and spawn player circle entities into it. The first thing we should do before spawning a player into a match is give them a name.

<Tabs groupId="server-language" defaultValue="rust">
<TabItem value="rust" label="Rust">
Add the following to the bottom of your file.

```rust
const START_PLAYER_MASS: i32 = 15;

#[spacetimedb::reducer]
pub fn enter_game(ctx: &ReducerContext, name: String) -> Result<(), String> {
    log::info!("Creating player with name {}", name);
    let mut player: Player = ctx.db.player().identity().find(ctx.sender).ok_or("")?;
    let player_id = player.player_id;
    player.name = name;
    ctx.db.player().identity().update(player);
    spawn_player_initial_circle(ctx, player_id)?;

    Ok(())
}

fn spawn_player_initial_circle(ctx: &ReducerContext, player_id: i32) -> Result<Entity, String> {
    let mut rng = ctx.rng();
    let world_size = ctx
        .db
        .config()
        .id()
        .find(&0)
        .ok_or("Config not found")?
        .world_size;
    let player_start_radius = mass_to_radius(START_PLAYER_MASS);
    let x = rng.gen_range(player_start_radius..(world_size as f32 - player_start_radius));
    let y = rng.gen_range(player_start_radius..(world_size as f32 - player_start_radius));
    spawn_circle_at(
        ctx,
        player_id,
        START_PLAYER_MASS,
        DbVector2 { x, y },
        ctx.timestamp,
    )
}

fn spawn_circle_at(
    ctx: &ReducerContext,
    player_id: i32,
    mass: i32,
    position: DbVector2,
    timestamp: Timestamp,
) -> Result<Entity, String> {
    let entity = ctx.db.entity().try_insert(Entity {
        entity_id: 0,
        position,
        mass,
    })?;

    ctx.db.circle().try_insert(Circle {
        entity_id: entity.entity_id,
        player_id,
        direction: DbVector2 { x: 0.0, y: 1.0 },
        speed: 0.0,
        last_split_time: timestamp,
    })?;
    Ok(entity)
}
```

The `enter_game` reducer takes one argument, the player's `name`. We can use this name to display as a label for the player in the match, by storing the name on the player's row. We are also spawning some circles for the player to control now that they are entering the game. To do this, we choose a random position within the bounds of the arena and create a new entity and corresponding circle row.
</TabItem>
<TabItem value="csharp" label="C#">
Add the following to the end of the `Module` class.

```csharp
const int START_PLAYER_MASS = 15;

[Reducer]
public static void EnterGame(ReducerContext ctx, string name)
{
    Log.Info($"Creating player with name {name}");
    var player = ctx.Db.player.identity.Find(ctx.Sender) ?? throw new Exception("Player not found");
    player.name = name;
    ctx.Db.player.identity.Update(player);
    SpawnPlayerInitialCircle(ctx, player.player_id);
}

public static Entity SpawnPlayerInitialCircle(ReducerContext ctx, int player_id)
{
    var rng = ctx.Rng;
    var world_size = (ctx.Db.config.id.Find(0) ?? throw new Exception("Config not found")).world_size;
    var player_start_radius = MassToRadius(START_PLAYER_MASS);
    var x = rng.Range(player_start_radius, world_size - player_start_radius);
    var y = rng.Range(player_start_radius, world_size - player_start_radius);
    return SpawnCircleAt(
        ctx,
        player_id,
        START_PLAYER_MASS,
        new DbVector2(x, y),
        ctx.Timestamp
    );
}

public static Entity SpawnCircleAt(ReducerContext ctx, int player_id, int mass, DbVector2 position, SpacetimeDB.Timestamp timestamp)
{
    var entity = ctx.Db.entity.Insert(new Entity
    {
        position = position,
        mass = mass,
    });

    ctx.Db.circle.Insert(new Circle
    {
        entity_id = entity.entity_id,
        player_id = player_id,
        direction = new DbVector2(0, 1),
        speed = 0f,
        last_split_time = timestamp,
    });
    return entity;
}
```

The `EnterGame` reducer takes one argument, the player's `name`. We can use this name to display as a label for the player in the match, by storing the name on the player's row. We are also spawning some circles for the player to control now that they are entering the game. To do this, we choose a random position within the bounds of the arena and create a new entity and corresponding circle row.
</TabItem>
</Tabs>

Let's also modify our `disconnect` reducer to remove the circles from the arena when the player disconnects from the database server.

<Tabs groupId="server-language" defaultValue="rust">
<TabItem value="rust" label="Rust">

```rust
#[spacetimedb::reducer(client_disconnected)]
pub fn disconnect(ctx: &ReducerContext) -> Result<(), String> {
    let player = ctx
        .db
        .player()
        .identity()
        .find(&ctx.sender)
        .ok_or("Player not found")?;
    let player_id = player.player_id;
    ctx.db.logged_out_player().insert(player);
    ctx.db.player().identity().delete(&ctx.sender);

    // Remove any circles from the arena
    for circle in ctx.db.circle().player_id().filter(&player_id) {
        ctx.db.entity().entity_id().delete(&circle.entity_id);
        ctx.db.circle().entity_id().delete(&circle.entity_id);
    }

    Ok(())
}
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
[Reducer(ReducerKind.ClientDisconnected)]
public static void Disconnect(ReducerContext ctx)
{
    var player = ctx.Db.player.identity.Find(ctx.Sender) ?? throw new Exception("Player not found");
    // Remove any circles from the arena
    foreach (var circle in ctx.Db.circle.player_id.Filter(player.player_id))
    {
        var entity = ctx.Db.entity.entity_id.Find(circle.entity_id) ?? throw new Exception("Could not find circle");
        ctx.Db.entity.entity_id.Delete(entity.entity_id);
        ctx.Db.circle.entity_id.Delete(entity.entity_id);
    }
    ctx.Db.logged_out_player.Insert(player);
    ctx.Db.player.identity.Delete(player.identity);
}
```

</TabItem>
</Tabs>

Finally, publish the new module to SpacetimeDB with this command:

```sh
spacetime publish --server local blackholio --delete-data
```

Deleting the data is optional in this case, but in case you've been messing around with the module we can just start fresh.

:::note
When using `--delete-data`, SpacetimeDB will prompt you to confirm the deletion. Enter **y** and press **Enter** to proceed.
:::

### Creating the Arena

With the server logic in place to spawn food and players, extend the Unreal client to display the current state.

<Tabs groupId="client-language" defaultValue="cpp">
<TabItem value="cpp" label="C++">
Add the `SetupArena` and `CreateBorderCube` methods and properties to your `GameManager.h` class. Place them below the `Handle{}` functions in the private block:

```cpp
    /* Border */
    UFUNCTION()
    void SetupArena(int64 WorldSizeMeters);
    UFUNCTION()
    void CreateBorderCube(const FVector2f Position, const FVector2f Size) const;

    UPROPERTY(VisibleAnywhere, Category="Arena")
    UInstancedStaticMeshComponent* BorderISM;
    UPROPERTY(EditDefaultsOnly, Category="Arena", meta=(ClampMin="1.0"))
    float BorderThickness = 50.0f;
    UPROPERTY(EditDefaultsOnly, Category="Arena", meta=(ClampMin="1.0"))
    float BorderHeight = 100.0f;
    UPROPERTY(EditDefaultsOnly, Category="Arena")
    UMaterialInterface* BorderMaterial = nullptr;
    UPROPERTY(EditDefaultsOnly, Category="Arena")
    UStaticMesh* CubeMesh = nullptr;        // defaults as /Engine/BasicShapes/Cube.Cube
    /* Border */
```

Next, we'll need to make a few updates in `GameManager.cpp`.

First, update the includes:

```cpp
#include "GameManager.h"
#include "Components/InstancedStaticMeshComponent.h"
#include "Connection/Credentials.h"
#include "ModuleBindings/Tables/ConfigTable.g.h"
```

The `AGameManager()` constructor in `GameManager.cpp` includes an `InstancedStaticMeshComponent` to set up the cube. Update the constructor as follows:

```cpp
AGameManager::AGameManager()
{
    PrimaryActorTick.bCanEverTick = true;
    PrimaryActorTick.bStartWithTickEnabled = true;

    BorderISM = CreateDefaultSubobject<UInstancedStaticMeshComponent>(TEXT("BorderISM"));
    SetRootComponent(BorderISM);

    if (CubeMesh != nullptr)
        return;

    static ConstructorHelpers::FObjectFinder<UStaticMesh> CubeAsset(TEXT("/Engine/BasicShapes/Cube.Cube"));
    if (CubeAsset.Succeeded())
    {
        CubeMesh = CubeAsset.Object;
    }
}
```

Add the implementations of `SetupArena` and `CreateBorderCube` to the end of `GameManager.cpp`:

```cpp
void AGameManager::SetupArena(int64 WorldSizeMeters)
{
    if (!BorderISM || !CubeMesh) return;

    BorderISM->ClearInstances();
    BorderISM->SetStaticMesh(CubeMesh);
    if (BorderMaterial)
    {
        BorderISM->SetMaterial(0, BorderMaterial);
    }

    // Convert from meters (int64) → centimeters (double for precision)
    const double worldSizeCmDouble = static_cast<double>(WorldSizeMeters) * 100.0;

    // Clamp to avoid float overflow in transforms
    const double clampedWorldSizeCmDouble = FMath::Clamp(
        worldSizeCmDouble,
        0.0,
        FLT_MAX * 0.25 // safe margin
    );

    // Convert to float for actual Unreal math
    const float worldSizeCm = static_cast<float>(clampedWorldSizeCmDouble);

    const float borderThicknessCm = BorderThickness; // already cm

    // Create four borders
    CreateBorderCube(
        FVector2f(worldSizeCm * 0.5f, worldSizeCm + borderThicknessCm * 0.5f), // North
        FVector2f(worldSizeCm + borderThicknessCm * 2.0f, borderThicknessCm)
    );

    CreateBorderCube(
        FVector2f(worldSizeCm * 0.5f, -borderThicknessCm * 0.5f), // South
        FVector2f(worldSizeCm + borderThicknessCm * 2.0f, borderThicknessCm)
    );

    CreateBorderCube(
        FVector2f(worldSizeCm + borderThicknessCm * 0.5f, worldSizeCm * 0.5f), // East
        FVector2f(borderThicknessCm, worldSizeCm + borderThicknessCm * 2.0f)
    );

    CreateBorderCube(
        FVector2f(-borderThicknessCm * 0.5f, worldSizeCm * 0.5f), // West
        FVector2f(borderThicknessCm, worldSizeCm + borderThicknessCm * 2.0f)
    );
}

void AGameManager::CreateBorderCube(const FVector2f Position, const FVector2f Size) const
{
    // Scale from the 100cm default cube to desired size (in cm)
    const FVector Scale(Size.X / 100.0f, BorderHeight / 100.0f, Size.Y / 100.0f);

    // Place so the bottom sits on Z=0 (cube is centered)
    const FVector Location(Position.X, BorderHeight * 0.5f, Position.Y);

    const FTransform Transform(FRotator::ZeroRotator, Location, Scale);
    BorderISM->AddInstance(Transform);
}
```

In `HandleSubscriptionApplied`, call the `SetupArena` method. Update `HandleSubscriptionApplied` as follows:

```cpp
void AGameManager::HandleSubscriptionApplied(FSubscriptionEventContext& Context)
{
    UE_LOG(LogTemp, Log, TEXT("Subscription applied!"));

    // Once we have the initial subscription sync'd to the client cache
    // Get the world size from the config table and set up the arena
    int64 WorldSize = Conn->Db->Config->Id->Find(0).WorldSize;
    SetupArena(WorldSize);
}
```
</TabItem>
<TabItem value="blueprint" label="Blueprint">
Open `BP_GameManager` and update to the following:

1. Add a **Variable**
    - Change **Variable Name** to `BorderThickness`
    - Change **Variable Type** to **Float**
    - Change **Default Value** to `50.0`
    - Change **Category** to `Arena`
2. Add a **Variable**
    - Change **Variable Name** to `BorderHeight`
    - Change **Variable Type** to **Float**
    - Change **Default Value** to `100.0`
    - Change **Category** to `Arena`
3. Add a **Variable**
    - Change **Variable Name** to `BorderMaterial`
    - Change **Variable Type** to **Material Instance > Object Reference**
    - Change **Default Value** to `BasicShapeMaterial_Inst`
    - Change **Category** to `Arena`
4. Add a **Component**
    - Click **Add** button
    - Select **Instanced Static Mesh**
    - Rename to `BorderISM`

Add the `CreateBorderCube` and `SetupArena` functions and properties to `BP_GameManager`:

Add **Function** named `CreateBorderCube` as follows:
![Add CreateBorderCube](/images/unreal/part-3-01-blueprint-setup-arena-1.png)

- Add **Input** as `Position` with **Vector 2D** as the type.
- Add **Input** as `Size` with **Vector 2D** as the type.

Add **Function** named `SetupArena` as follows:
![Add SetupArena](/images/unreal/part-3-01-blueprint-setup-arena-2.png)

![Continue SetupArena](/images/unreal/part-3-01-blueprint-setup-arena-3.png)

- Add **Input** as `WorldSizeMeters` with **Integer 64** as the type.
- Add **Local Variable** as `WorldSizeCm` with **Float** as the type.
- Add **Local Variable** as `HalfWorldSize` with **Float** as the type.
- Add **Local Variable** as `BorderWidth` with **Float** as the type.
- Add **Local Variable** as `HalfBorder` with **Float** as the type.

Add **Function** named `IsConnected` as follows:
![Add IsConnected](/images/unreal/part-3-03-blueprint-gamemanager-2.png)

- Add **Output** as `Result` with **Boolean** as the type.
- Check **Pure**

In `OnApplied_Event`, call the `SetupArena` function. Update `OnApplied_Event` as follows:

![Call SetupArena](/images/unreal/part-3-01-blueprint-setup-arena-4.png)
</TabItem>
</Tabs>
	
The `OnApplied` callback is called after the server synchronizes the initial state of your tables with the client. After the sync, look up the world size from the `config` table and use it to set up the arena.

### Create Entity Blueprints

With the arena set up, use the row data that SpacetimeDB syncs with the client to create and display **Blueprints** on the screen.

<Tabs groupId="client-language" defaultValue="cpp">
<TabItem value="cpp" label="C++">
Start by making a C++ class for each entity you want in the scene. If the Unreal project is not running, start it now. From the top menu, choose **Tools -> New C++ Class...** to create the following classes (you’ll modify these later):

:::note
After creating the first class, wait for **Live Coding** to finish before creating the next classes.
:::

1. **Parent:** **Actor** · **Class Type:** **Public** · **Class Name:** `Entity`
2. **Parent:** **All Classes -> Entity** · **Class Type:** **Public** · **Class Name:** `Circle`
3. **Parent:** **All Classes -> Entity** · **Class Type:** **Public** · **Class Name:** `Food`
4. **Parent:** **Pawn** · **Class Type:** **Public** · **Class Name:** `PlayerPawn`
5. **Parent:** **Player Controller** · **Class Type:** **Public** · **Class Name:** `BlackholioPlayerController`
6. **Parent:** **None** · **Class Type:** **Public** · **Class Name:** `DbVector2`

Next add blueprints for our these classes:

![Add Circle](/images/unreal/part-3-01-create-blueprint.png)

1. **Circle Blueprint**
   - In the **Content Drawer**, right-click and choose **Blueprint -> Blueprint Class**.
   - Expand **All Classes**, search for `Circle`, highlight `Circle`, and click **Select**.
   - Rename the new Blueprint to `BP_Circle`.

2. **Food Blueprint**
   - In the **Content Drawer**, right-click and choose **Blueprint -> Blueprint Class**.
   - Expand **All Classes**, search for `Food`, highlight `Food`, and click **Select**.
   - Rename the new Blueprint to `BP_Food`.

3. **Player Blueprint**
   - In the **Content Drawer**, right-click and choose **Blueprint -> Blueprint Class**.
   - Expand **All Classes**, search for `PlayerPawn`, highlight `PlayerPawn`, and click **Select**.
   - Rename the new Blueprint to `BP_PlayerPawn`.

4. **Player Controller Blueprint**
   - In the **Content Drawer**, right-click and choose **Blueprint -> Blueprint Class**.
   - Expand **All Classes**, search for `BlackholioPlayerController`, highlight `BlackholioPlayerController`, and click **Select**.
   - Rename the new Blueprint to `BP_BlackholioPlayerController`.
   - Open **Window -> World Settings** in the top menu.
   - Change **Player Controller Class** from **PlayerController** to `BP_BlackholioPlayerController`.
   - Save the level.
</TabItem>
<TabItem value="blueprint" label="Blueprint">
Add blueprints for our entities:

1. **Entity Blueprint**
    - In the **Content Drawer**, right-click and choose **Blueprint -> Blueprint Class**.
    - Click **Actor**.
    - Rename the new Blueprint to `BP_Entity`.

2. **Circle Blueprint**  
   - In the **Content Drawer**, find and right-click **BP_Entity** and choose **Create Child Blueprint Class**.
   - Rename the new Blueprint to `BP_Circle`.

3. **Food Blueprint**  
   - In the **Content Drawer**, find and right-click **BP_Entity** and choose **Create Child Blueprint Class**.
   - Rename the new Blueprint to `BP_Food`.

4. **Player Blueprint**  
   - In the **Content Drawer**, right-click and choose **Blueprint -> Blueprint Class**.
   - Click **Pawn**.
   - Rename the new Blueprint to `BP_PlayerPawn`.

5. **Player Controller Blueprint**  
   - In the **Content Drawer**, right-click and choose **Blueprint -> Blueprint Class**.
   - Click **Player Controller**.
   - Rename the new Blueprint to `BP_PlayerController`.
   - Open **Window -> World Settings** in the top menu.  
   - Change **Player Controller Class** from **PlayerController** to `BP_PlayerController`.  
   - Save the level.
</TabItem>
</Tabs>

### Set Up the Nameplate Blueprint

Create a widget Blueprint for the player nameplate:

- In the **Content Drawer**, right-click and choose **Blueprint -> Blueprint Class**.
- Expand **All Classes**, search for **UserWidget**, highlight **UserWidget**, and click **Select**.
- Name the new Blueprint `WBP_Nameplate`.

Double-click `WBP_Nameplate` to open it, then make the following changes:

1. In the **Palette** on the left, search for **Size Box** and drag it into the **Hierarchy** under `WBP_Nameplate`.
2. Search the **Palette** for **Text** and drag it under the **Size Box**.
3. Select the **Text** widget and update its details:
   - Rename it to `TextBlock`.
   - Check **Is Variable**.
   - Set **Font -> Size** to `24`.
   - Set **Font -> Justification** to **Align Text Center**.

![WBP_Nameplate](/images/unreal/part-3-02-create-nameplate.png)

Finally, add Blueprint logic so the circle can update its nameplate:

1. In the `WBP_Nameplate` editor, open the **Graph** tab (top right).
2. Click the **+** button next to **My Blueprint -> Functions** and name the new function `UpdateText`.
3. Select `UpdateText` in the editor, then in **Details -> Inputs**, add a variable named `Text` of type `String`.
4. Drag **TextBlock** into the graph and choose **Get TextBlock**.
5. Drag off **TextBlock** and search for **Set Text**.
6. Connect **UpdateText** to **Set Text**, then connect **UpdateText -> Text** to **Set Text -> Text**.
   - A conversion from `String` to `Text` is added automatically; this is expected.
7. Click **Save** and **Compile**.

![UpdateText Function](/images/unreal/part-3-03-update-text-function.png)

### Set Up Circle Entity Blueprint

Import and set up the circle sprite:

1. Right-click the image below and save it locally:  
   ![Circle](/images/unreal/circle.png)

2. In the **Content Drawer**, right-click and select **Import to Current Folder**, then choose the saved image.
   - This imports the Circle as a texture.
   - Right-click the imported texture, select **Sprite Actions -> Create Sprite**, and rename it `Circle_Sprite`.

Next, open `BP_Circle` and configure it:

1. Select **DefaultSceneRoot**, add a **Components -> Paper Sprite** component, and rename it `Circle`.
   - In the **Details** panel, set **Scale** to `0.4` for all three axes.
   - Set **Source Sprite** to `Circle_Sprite`.

2. Select **DefaultSceneRoot**, add a **Components -> Widget** component, and rename it `NameplateWidget`.
   - In the **Details** panel, set **Location** to `0, 10, -45`.
   - Set **Rotation** to `0, 0, 90`.
   - Under **User Interface**, update:
     - **Widget Class** to `WBP_Nameplate`
     - **Draw Size** to `300, 60`
     - **Pivot** to `0.5, 1.0`
3. Click **Save** and **Compile**.

### Set Up the Food Entity Blueprint

The food entity is a simple collectible. Open `BP_Food` and configure it as follows:

1. Select **DefaultSceneRoot**, add a **Components -> Paper Sprite** component, and rename it `Circle`.
   - In the **Details** panel, set **Scale** to `0.4` for all three axes.
   - Set **Source Sprite** to `Circle_Sprite`.
2. Click **Save** and **Compile**.

### Set Up the PlayerPawn Blueprint

The PlayerPawn owns the circles and controls the camera by following the center of mass. This setup provides the initial functionality; additional behavior will be added in the C++ class.

Open `BP_PlayerPawn` and make the following changes:

1. Select **DefaultSceneRoot**, add a **Components -> Spring Arm** component.
2. Select **SpringArm**, add a **Components -> Camera** component.
3. Select **SpringArm**
   - In the **Details** panel, set:
     - **Location** to `0, 15000, 0`
     - **Rotation** to `0, 0, -90`
     - **Target Arm Length** to `200`
4. Click **Save** and **Compile**.

:::note
Make sure the **Camera** component's **Location** and **Rotation** are `0, 0, 0`
:::

### Update Classes

<Tabs groupId="client-language" defaultValue="cpp">
<TabItem value="cpp" label="C++">
With the Blueprints set up, return to the source code behind the entities. First, add helper functions to translate server-side vectors to Unreal vectors.

Open `DbVector2.h` and update it as follows:

```cpp
#pragma once

#include "ModuleBindings/Types/DbVector2Type.g.h"

FORCEINLINE FDbVector2Type ToDbVector(const FVector2D& Vec)
{
    FDbVector2Type Out;
    Out.X = Vec.X;
    Out.Y = Vec.Y;
    return Out;
}

FORCEINLINE FDbVector2Type ToDbVector(const FVector& Vec)
{
    FDbVector2Type Out;
    Out.X = Vec.X;
    Out.Y = Vec.Y;
    return Out;
}

FORCEINLINE FVector2D ToFVector2D(const FDbVector2Type& Vec)
{
    return FVector2D(Vec.X * 100.f, Vec.Y * 100.f);
}

FORCEINLINE FVector ToFVector(const FDbVector2Type& Vec, float Z = 0.f)
{
    return FVector(Vec.X * 100.f, Z, Vec.Y * 100.f);
}
```

:::warning
Delete `DbVector2.cpp` (not needed), or clear its contents so compilation succeeds.
::::

#### Entity Class

With the foundation in place, implement the core entity class. Edit `Entity.h` as follows:

```cpp
#pragma once

#include "CoreMinimal.h"
#include "GameFramework/Actor.h"
#include "Entity.generated.h"

struct FEventContext;
struct FEntityType;

UCLASS()
class CLIENT_UNREAL_API AEntity : public AActor
{
    GENERATED_BODY()

public:
    AEntity();

protected:
    UPROPERTY(EditDefaultsOnly, Category="BH|Entity")
    float LerpTime = 0.f;
    UPROPERTY(EditDefaultsOnly, Category="BH|Entity")
    float LerpDuration = 0.10f;

    FVector LerpStartPosition = FVector::ZeroVector;
    FVector LerpTargetPosition = FVector::ZeroVector;
    float TargetScale = 1.f;

public:
    int32 EntityId = 0;
    virtual void Tick(float DeltaTime) override;

    void Spawn(int32 InEntityId);
    virtual void OnEntityUpdated(const FEntityType& NewVal);
    virtual void OnDelete(const FEventContext& Context);

    void SetColor(const FLinearColor& Color) const;

    static float MassToRadius(int32 Mass) { return FMath::Sqrt(static_cast<float>(Mass)); }
    static float MassToDiameter(int32 Mass) { return MassToRadius(Mass) * 2.f; }
};
```

Update `Entity.cpp` as follows:

```cpp
#include "Entity.h"
#include "DbVector2.h"
#include "GameManager.h"
#include "PaperSpriteComponent.h"
#include "ModuleBindings/Tables/EntityTable.g.h"

AEntity::AEntity()
{
    PrimaryActorTick.bCanEverTick = true;
    LerpTime = 0.f;
}

void AEntity::Tick(float DeltaTime)
{
    Super::Tick(DeltaTime);

    // Interpolate the position and scale
    LerpTime = FMath::Min(LerpTime + DeltaTime, LerpDuration);
    const float Alpha = (LerpDuration > 0.f) ? (LerpTime / LerpDuration) : 1.f;
    SetActorLocation(FMath::Lerp(LerpStartPosition, LerpTargetPosition, Alpha));
    const float NewScale = FMath::FInterpTo(GetActorScale3D().X, TargetScale, DeltaTime, 8.f);
    SetActorScale3D(FVector(NewScale));
}

void AEntity::Spawn(int32 InEntityId)
{
    EntityId = InEntityId;

    const FEntityType EntityRow = AGameManager::Instance->Conn->Db->Entity->EntityId->Find(InEntityId);

    LerpStartPosition = LerpTargetPosition = ToFVector(EntityRow.Position);
    TargetScale = MassToDiameter(EntityRow.Mass);
    SetActorScale3D(FVector::OneVector);
}

void AEntity::OnEntityUpdated(const FEntityType& NewVal)
{
    LerpStartPosition = GetActorLocation();
    LerpTargetPosition = ToFVector(NewVal.Position);
    TargetScale = MassToDiameter(NewVal.Mass);
    LerpTime = 0.f;
}

void AEntity::OnDelete(const FEventContext& Context)
{
    Destroy();
}

void AEntity::SetColor(const FLinearColor& Color) const
{
    if (UPaperSpriteComponent* SpriteComponent = FindComponentByClass<UPaperSpriteComponent>())
    {
        SpriteComponent->SetSpriteColor(Color);
    }
}
```

The `Entity` class provides helper functions and basic functionality to manage game objects based on entity updates.

:::note
One notable feature is linear interpolation (lerp) between the server-reported entity position and the client-drawn position. This technique produces smoother movement.

If you're interested in learning more checkout [this demo](https://gabrielgambetta.com/client-side-prediction-live-demo.html) from Gabriel Gambetta.
:::

#### Circle Class

Open `Circle.h` and update it as follows:

```cpp
#pragma once

#include "CoreMinimal.h"
#include "Entity.h"
#include "Circle.generated.h"

struct FCircleType;
class APlayerPawn;

UCLASS()
class CLIENT_UNREAL_API ACircle : public AEntity
{
    GENERATED_BODY()

public:
    ACircle();

    int32 OwnerPlayerId = 0;
    UPROPERTY(BlueprintReadOnly, Category="BH|Circle")
    FString Username;

    void Spawn(const FCircleType& Circle, APlayerPawn* InOwner);
    virtual void OnDelete(const FEventContext& Context) override;

    UFUNCTION(BlueprintCallable, Category="BH|Circle")
    void SetUsername(const FString& InUsername);

    DECLARE_DYNAMIC_MULTICAST_DELEGATE_OneParam(FOnUsernameChanged, const FString&, NewUsername);
    UPROPERTY(BlueprintAssignable, Category="BH|Circle")
    FOnUsernameChanged OnUsernameChanged;

protected:
    UPROPERTY(EditDefaultsOnly, Category="BH|Circle")
    TArray<FLinearColor> ColorPalette;

private:
    TWeakObjectPtr<APlayerPawn> Owner;
};
```

Update `Circle.cpp` as follows:

```cpp
#include "Circle.h"
#include "PlayerPawn.h"
#include "ModuleBindings/Types/CircleType.g.h"

ACircle::ACircle()
{
    ColorPalette = {
        // Yellow
        FLinearColor::FromSRGBColor(FColor(175, 159, 49, 255)),
        FLinearColor::FromSRGBColor(FColor(175, 116, 49, 255)),

        // Purple
        FLinearColor::FromSRGBColor(FColor(112, 47, 252, 255)),
        FLinearColor::FromSRGBColor(FColor(51,  91, 252, 255)),

        // Red
        FLinearColor::FromSRGBColor(FColor(176, 54, 54, 255)),
        FLinearColor::FromSRGBColor(FColor(176, 109, 54, 255)),
        FLinearColor::FromSRGBColor(FColor(141, 43, 99, 255)),

        // Blue
        FLinearColor::FromSRGBColor(FColor(2,   188, 250, 255)),
        FLinearColor::FromSRGBColor(FColor(7,   50,  251, 255)),
        FLinearColor::FromSRGBColor(FColor(2,   28,  146, 255)),
    };
}

void ACircle::Spawn(const FCircleType& Circle, APlayerPawn* InOwner)
{
    Super::Spawn(Circle.EntityId);

    const int32 Index = ColorPalette.Num() ? static_cast<int32>(InOwner->PlayerId % ColorPalette.Num()) : 0;
    const FLinearColor Color = ColorPalette.IsValidIndex(Index) ? ColorPalette[Index] : FLinearColor::Green;
    SetColor(Color);

    this->Owner = InOwner;
    SetUsername(InOwner->Username);
}

void ACircle::OnDelete(const FEventContext& Context)
{
    Super::OnDelete(Context);
    Owner->OnCircleDeleted(this);
}

void ACircle::SetUsername(const FString& InUsername)
{
    if (Username.Equals(InUsername, ESearchCase::CaseSensitive))
        return;

    Username = InUsername;
    OnUsernameChanged.Broadcast(Username);
}
```

At the top of the file, define possible colors for the circle. A spawn function creates an `ACircle` (the same type stored in the `circle` table) and an `APlayerPawn`. The function sets the circle’s color based on the player ID and updates the circle’s text with the player’s username.

:::note
`ACircle` inherits from `AEntity`, not `AActor`. Compilation will fail until `APlayerPawn` is implemented.
:::

#### Food Class

Open `Food.h` and update it as follows:

```cpp
#pragma once

#include "CoreMinimal.h"
#include "Entity.h"
#include "Food.generated.h"

struct FFoodType;

UCLASS()
class CLIENT_UNREAL_API AFood : public AEntity
{
    GENERATED_BODY()

public:
    AFood();
    void Spawn(const FFoodType& FoodEntity);
protected:
    UPROPERTY(EditDefaultsOnly, Category="BH|Food")
    TArray<FLinearColor> ColorPalette;
};
```

Update `Food.cpp` as follows:

```cpp
#include "Food.h"
#include "ModuleBindings/Types/FoodType.g.h"

AFood::AFood()
{
    ColorPalette = {
        // Greenish
        FLinearColor::FromSRGBColor(FColor(119, 252, 173, 255)),
        FLinearColor::FromSRGBColor(FColor(76,  250, 146, 255)),
        FLinearColor::FromSRGBColor(FColor(35,  246, 120, 255)),

        // Aqua / Teal
        FLinearColor::FromSRGBColor(FColor(119, 251, 201, 255)),
        FLinearColor::FromSRGBColor(FColor(76,  249, 184, 255)),
        FLinearColor::FromSRGBColor(FColor(35,  245, 165, 255)),
    };
}

void AFood::Spawn(const FFoodType& FoodEntity)
{
    Super::Spawn(FoodEntity.EntityId);

    const int32 Index = ColorPalette.Num() ? static_cast<int32>(EntityId % ColorPalette.Num()) : 0;
    const FLinearColor Color = ColorPalette.IsValidIndex(Index) ? ColorPalette[Index] : FLinearColor::Green;
    SetColor(Color);
}
```

#### PlayerPawn Class

Open `PlayerPawn.h` and update it as follows:

```cpp
#pragma once

#include "CoreMinimal.h"
#include "GameFramework/Pawn.h"
#include "PlayerPawn.generated.h"

class ACircle;
struct FPlayerType;

UCLASS()
class CLIENT_UNREAL_API APlayerPawn : public APawn
{
    GENERATED_BODY()

public:
    APlayerPawn();
    void Initialize(FPlayerType Player);

    int32 PlayerId = 0;
    UPROPERTY(BlueprintReadOnly, Category="BH|Player")
    FString Username;
    UPROPERTY(BlueprintReadWrite, Category="BH|Player")
    bool bIsLocalPlayer = false;

    UPROPERTY()
    TArray<TWeakObjectPtr<ACircle>> OwnedCircles;

    UFUNCTION()
    void OnCircleSpawned(ACircle* Circle);
    UFUNCTION()
    void OnCircleDeleted(ACircle* Circle);

    int32 TotalMass() const;
    UFUNCTION(BlueprintPure, Category="BH|Player")
    FVector CenterOfMass() const;

protected:
    virtual void Destroyed() override;

public:
    virtual void Tick(float DeltaTime) override;

private:
    UPROPERTY(EditDefaultsOnly, Category="BH|Net")
    float SendUpdatesFrequency = 0.0333f;
};
```

Next, add the implementation to `PlayerPawn.cpp`.  
In the Blueprint we've set the `PlayerPawn` with a spring arm and camera, simplifying camera controls since the camera automatically follows the pawn.  
You can see this behavior in the `Tick` function below:

```cpp
#include "PlayerPawn.h"
#include "Circle.h"
#include "GameManager.h"
#include "Kismet/GameplayStatics.h"
#include "ModuleBindings/Tables/EntityTable.g.h"
#include "ModuleBindings/Types/EntityType.g.h"
#include "ModuleBindings/Types/PlayerType.g.h"

APlayerPawn::APlayerPawn()
{
    PrimaryActorTick.bCanEverTick = true;
}

void APlayerPawn::Initialize(FPlayerType Player)
{
    PlayerId = Player.PlayerId;
    Username = Player.Name;

    if (Player.Identity == AGameManager::Instance->LocalIdentity)
    {
        bIsLocalPlayer = true;
        if (APlayerController* PC = UGameplayStatics::GetPlayerController(GetWorld(), 0))
        {
            PC->Possess(this);
        }
    }
}

void APlayerPawn::OnCircleSpawned(ACircle* Circle)
{
    if (ensure(Circle))
    {
        OwnedCircles.AddUnique(Circle);
    }
}

void APlayerPawn::OnCircleDeleted(ACircle* Circle)
{
    if (Circle)
    {
        for (int32 i = OwnedCircles.Num() - 1; i >= 0; --i)
        {
            if (!OwnedCircles[i].IsValid() || OwnedCircles[i].Get() == Circle)
            {
                OwnedCircles.RemoveAt(i);
            }
        }
    }

    if (OwnedCircles.Num() == 0 && bIsLocalPlayer)
    {
        UE_LOG(LogTemp, Log, TEXT("Player has died!"));
    }
}

int32 APlayerPawn::TotalMass() const
{
    int32 Total = 0;
    for (int32 Index = 0; Index < OwnedCircles.Num(); ++Index)
    {
        const TWeakObjectPtr<ACircle>& Weak = OwnedCircles[Index];
        if (!Weak.IsValid()) continue;

        const ACircle* Circle = Weak.Get();
        const int32 Id = Circle->EntityId;

        const FEntityType Entity = AGameManager::Instance->Conn->Db->Entity->EntityId->Find(Id);
        Total += Entity.Mass;
    }
    return Total;
}

FVector APlayerPawn::CenterOfMass() const
{
    if (OwnedCircles.Num() == 0)
    {
        return FVector::ZeroVector;
    }

    FVector WeightedPosition = FVector::ZeroVector; // Σ (pos * mass)
    double  TotalMass        = 0.0;                 // Σ mass

    const int32 Count = OwnedCircles.Num();

    for (int32 Index = 0; Index < Count; ++Index)
    {
        const TWeakObjectPtr<ACircle>& Weak = OwnedCircles[Index];
        if (!Weak.IsValid()) continue;

        const ACircle* Circle = Weak.Get();
        const int32 Id = Circle->EntityId;

        const FEntityType Entity = AGameManager::Instance->Conn->Db->Entity->EntityId->Find(Id);
        const double Mass = Entity.Mass;

        const FVector Loc = Circle->GetActorLocation();

        if (Mass <= 0.0) continue;

        WeightedPosition += (Loc * Mass);
        TotalMass += Mass;
    }

    const FVector ActorLoc = GetActorLocation();

    FVector Result = FVector::ZeroVector;
    if (TotalMass > 0.0)
    {
        const FVector CalculatedCenter = WeightedPosition / TotalMass;
        // Keep Z at the player's Z, per your original intent
        Result = FVector(CalculatedCenter.X, ActorLoc.Y, CalculatedCenter.Z);
    }

    return Result;
}

void APlayerPawn::Destroyed()
{
    Super::Destroyed();
    for (TWeakObjectPtr<ACircle>& CirclePtr : OwnedCircles)
    {
        if (ACircle* Circle = CirclePtr.Get())
        {
            Circle->Destroy();
        }
    }
    OwnedCircles.Empty();
}

void APlayerPawn::Tick(float DeltaTime)
{
    Super::Tick(DeltaTime);

    if (!bIsLocalPlayer || OwnedCircles.Num() == 0)
        return;

    const FVector ArenaCenter(0.f, 1.f, 0.f);
    FVector Target = ArenaCenter;
    if (AGameManager::Instance->IsConnected())
    {
        const FVector CoM = CenterOfMass();
        if (!CoM.ContainsNaN())
        {
            Target = { CoM.X, 1.f, CoM.Z };
        }
    }
    const FVector NewLoc = FMath::VInterpTo(GetActorLocation(), Target, DeltaTime, 120.f);
    SetActorLocation(NewLoc);
}
```
</TabItem>
<TabItem value="blueprint" label="Blueprint">
#### Entity Blueprint

With the foundation in place, implement the core entity class. Edit `BP_Entity` add the following **Variables**:

1. Add `LerpStartPosition`
    - Change **Variable Type** to **Vector**
2. Add `LerpTargetPosition`
    - Change **Variable Type** to **Vector**
3. Add `TargetScale`
    - Change **Variable Type** to **Float**
    - Change **Default Value** to `1.0`
4. Add `LerpTime`
    - Change **Variable Type** to **Float**
5. Add `LerpDuration`
    - Change **Variable Type** to **Float**
    - Change **Default Value** to `0.1`
6. Add `EntityId`
    - Change **Variable Type** to **Integer**
7. Add `Alpha`
    - Change **Variable Type** to **Float**

![Add Variables](/images/unreal/part-3-02-blueprint-entity-1.png)

Add the following to **Event Tick**:

![Update Event Tick](/images/unreal/part-3-02-blueprint-entity-2.png)

Add **Function** named `MassToRadius` as follows:

![Add MassToRadius](/images/unreal/part-3-02-blueprint-entity-7.png)

- Add **Input** as `Mass` with **Integer** as the type.
- Add **Output** as `Radius` with **Float** as the type.

Add **Function** named `MassToDiameter` as follows:

![Add MassToDiameter](/images/unreal/part-3-02-blueprint-entity-8.png)

- Add **Input** as `Mass` with **Integer** as the type.
- Add **Output** as `Diameter` with **Float** as the type.

Add **Function** named `OnUpdated` as follows:

![Add OnUpdated](/images/unreal/part-3-02-blueprint-entity-3.png)

- Add **Input** as `NewRow` with **Entity Type** as the type.

Add **Function** named `OnDeleted` as follows:

![Add OnDeleted](/images/unreal/part-3-02-blueprint-entity-4.png)

- Add **Input** as `Context` with **Event Context** as the type.

Add **Function** named `Spawn` as follows:

![Add Spawn](/images/unreal/part-3-02-blueprint-entity-5.png)

- Add **Input** as `In Entity Id` with **Integer** as the type.

Add **Function** named `SetColor` as follows:

![Add SetColor](/images/unreal/part-3-02-blueprint-entity-6.png)

- Add **Input** as `Color` with **Linear Color** as the type.

The `Entity` class provides helper functions and basic functionality to manage game objects based on entity updates.

> **Note:** One notable feature is linear interpolation (lerp) between the server-reported entity position and the client-drawn position. This technique produces smoother movement.
>
> If you're interested in learning more checkout [this demo](https://gabrielgambetta.com/client-side-prediction-live-demo.html) from Gabriel Gambetta.

#### PlayerPawn Blueprint

Open `BP_PlayerPawn` and add the following **Variables**:

1. Add `Username`
    - Change **Variable Type** to **String**
    - Check **Instance Editable**
2. Add `PlayerId`
    - Change **Variable Type** to an **Integer**
3. Add `IsLocalPlayer`
    - Change **Variable Type** to an **Boolean**
4. Add `OwnedCircles`
    - Change **Variable Type** to an **Array** **BP Circle -> Object References**
5. Add `GameManager`
    - Change **Variable Type** to an **BP GameManager -> Object References**
6. Add `Target`
    - Change **Variable Type** to an **Vector**

![Add Variables](/images/unreal/part-3-02-blueprint-player-1.png)

Add **Function** named `GetGameManager` as follows:

![Add GetGameManager](/images/unreal/part-3-02-blueprint-player-2.png)

- Add **Output** as `GameManager` with **BP Game Manager** as the type.

Add **Function** named `Initialize` as follows:

![Add Initialize](/images/unreal/part-3-02-blueprint-player-3.png)

- Add **Input** as `PlayerRow` with **Player Type** as the type.

Add **Function** named `OnCircleSpawned` as follows:

![Add OnCircleSpawned](/images/unreal/part-3-02-blueprint-player-4.png)

- Add **Input** as `Circle` with **BP Circle -> Object Reference** as the type.

Add **Function** named `OnCircleDeleted` as follows:

![Add OnCircleDeleted](/images/unreal/part-3-02-blueprint-player-5.png)

- Add **Input** as `Circle` with **BP Circle -> Object Reference** as the type.

Add **Function** named `CenterOfMass` as follows:

![Add CenterOfMass](/images/unreal/part-3-02-blueprint-player-6.png)

- Add **Output** as `Center` with **Vector** as the type.
- Add **Local Variable** as `WeightedPosition` with **Vector** as the type.
- Add **Local Variable** as `TotalMass` with **Float** as the type.

Add **Function** named `UpdateTargetLocation` as follows:

![Add UpdateTargetLocation](/images/unreal/part-3-02-blueprint-player-7.png)

Add **Function** named `GetUsername` as follows:

![Add GetUsername](/images/unreal/part-3-02-blueprint-player-10.png)

- Add **Output** as `Output` with **String** as the type.
- Check **Pure**

Update **Event Tick** to:
![Update Event Tick](/images/unreal/part-3-02-blueprint-player-8.png)

Update **Event Destroyed** to:
![Update Event Destroyed](/images/unreal/part-3-02-blueprint-player-9.png)

#### Circle Blueprint

Open `BP_Circle` and add the following **Variables**:

1. Add `OwningPlayer`
    - Change **Variable Type** to **BP Player Pawn -> Object Reference**
2. Add `ColorPalette`
    - Change **Variable Type** to an **Array** of **Linear Color**

![Color Palette](/images/unreal/part-3-02-blueprint-circle-1.png)

Override **Function** `OnDeleted` as follows:

![Override OnDeleted](/images/unreal/part-3-02-blueprint-circle-2.png)

- Add **Input** as `Context` with **Entity Context** as the type.

Add **Function** named `SpawnCircle` as follows:

![Add SpawnCircle](/images/unreal/part-3-02-blueprint-circle-3.png)

- Add **Input** as `Circle` with **Circle Type** as the type.
- Add **Input** as `InOwner` with **BP Player Pawn -> Object Reference** as the type

#### Food Blueprint

Open `BP_Food` and add the following **Variables**:

1. Add `ColorPalette`
    - Change **Variable Type** to an **Array** of **Linear Color**

![Color Palette](/images/unreal/part-3-02-blueprint-food-1.png)

Add **Function** named `SpawnFood` as follows:

![Add SpawnFood](/images/unreal/part-3-02-blueprint-food-2.png)

- Add **Input** as `Food Entity` with **Food Type** as the type.
</TabItem>
</Tabs>

### Spawning Blueprints

<Tabs groupId="client-language" defaultValue="cpp">
<TabItem value="cpp" label="C++">
Update `GameManager.h` to support spawning Blueprints.  
Make the following edits to the file:

Add the code below after the `UDbConnection` forward declaration:

```cpp
// ...
class UDbConnection;
class AEntity;
class ACircle;
class AFood;
class APlayerPawn;

UCLASS()
class CLIENT_UNREAL_API AGameManager : public AActor
// ...
```

Add in public below the TokenFilePath:

```cpp
class CLIENT_UNREAL_API AGameManager : public AActor
{
    GENERATED_BODY()

public:
    // ...

    UPROPERTY(EditAnywhere, Category="BH|Classes")
    TSubclassOf<ACircle> CircleClass;
    UPROPERTY(EditAnywhere, Category="BH|Classes")
    TSubclassOf<AFood> FoodClass;
    UPROPERTY(EditAnywhere, Category="BH|Classes")
    TSubclassOf<APlayerPawn> PlayerClass;

    // ...
```

Below the `/* Border */` section, add code to link the SpacetimeDB tables to the `GameManager` and handle entity spawning:

```cpp
    // ...
    /* Border */

    /* Data Bindings */
    UPROPERTY()
    TMap<int32, TWeakObjectPtr<AEntity>> EntityMap;
    UPROPERTY()
    TMap<int32, TWeakObjectPtr<APlayerPawn>> PlayerMap;

    APlayerPawn* SpawnOrGetPlayer(const FPlayerType& PlayerRow);
    ACircle* SpawnCircle(const FCircleType& CircleRow);
    AFood* SpawnFood(const FFoodType& Food);

    UFUNCTION()
    void OnCircleInsert(const FEventContext& Context, const FCircleType& NewRow);
    UFUNCTION()
    void OnEntityUpdate(const FEventContext& Context, const FEntityType& OldRow, const FEntityType& NewRow);
    UFUNCTION()
    void OnEntityDelete(const FEventContext& Context, const FEntityType& RemovedRow);
    UFUNCTION()
    void OnFoodInsert(const FEventContext& Context, const FFoodType& NewFood);
    UFUNCTION()
    void OnPlayerInsert(const FEventContext& Context, const FPlayerType& NewRow);
    UFUNCTION()
    void OnPlayerDelete(const FEventContext& Context, const FPlayerType& RemovedRow);
    /* Data Bindings */

    // ...
```

With the header updated, add the wiring for spawning entities with data from SpacetimeDB in `GameManager.cpp`.  
As with the header, edit only the relevant parts of the file.

First, update the includes:

```cpp
#include "GameManager.h"
#include "Circle.h"
#include "Entity.h"
#include "Food.h"
#include "PlayerPawn.h"
#include "Components/InstancedStaticMeshComponent.h"
#include "Connection/Credentials.h"
#include "ModuleBindings/Tables/CircleTable.g.h"
#include "ModuleBindings/Tables/ConfigTable.g.h"
#include "ModuleBindings/Tables/EntityTable.g.h"
#include "ModuleBindings/Tables/FoodTable.g.h"
#include "ModuleBindings/Tables/PlayerTable.g.h"
```

Next, update `HandleConnect` to register the table-change handlers:

```cpp
void AGameManager::HandleConnect(UDbConnection* InConn, FSpacetimeDBIdentity Identity, const FString& Token)
{
    UE_LOG(LogTemp, Log, TEXT("Connected."));
    UCredentials::SaveToken(Token);
    LocalIdentity = Identity;

    Conn->Db->Circle->OnInsert.AddDynamic(this, &AGameManager::OnCircleInsert);
    Conn->Db->Entity->OnUpdate.AddDynamic(this, &AGameManager::OnEntityUpdate);
    Conn->Db->Entity->OnDelete.AddDynamic(this, &AGameManager::OnEntityDelete);
    Conn->Db->Food->OnInsert.AddDynamic(this, &AGameManager::OnFoodInsert);
    Conn->Db->Player->OnInsert.AddDynamic(this, &AGameManager::OnPlayerInsert);
    Conn->Db->Player->OnDelete.AddDynamic(this, &AGameManager::OnPlayerDelete);

    FOnSubscriptionApplied AppliedDelegate;
    BIND_DELEGATE_SAFE(AppliedDelegate, this, AGameManager, HandleSubscriptionApplied);
    Conn->SubscriptionBuilder()
        ->OnApplied(AppliedDelegate)
        ->SubscribeToAllTables();
}
```

Finally, add the new functions at the end of `GameManager.cpp` to handle entity spawning:

```cpp
void AGameManager::OnCircleInsert(const FEventContext& Context, const FCircleType& NewRow)
{
    if (EntityMap.Contains(NewRow.EntityId)) return;
    SpawnCircle(NewRow);
}

void AGameManager::OnEntityUpdate(const FEventContext& Context, const FEntityType& OldRow, const FEntityType& NewRow)
{
    if (TWeakObjectPtr<AEntity>* WeakEntity = EntityMap.Find(NewRow.EntityId))
    {
        if (!WeakEntity->IsValid())
        {
            return;
        }
        if (AEntity* Entity = WeakEntity->Get())
        {
            Entity->OnEntityUpdated(NewRow);
        }
    }
}

void AGameManager::OnEntityDelete(const FEventContext& Context, const FEntityType& RemovedRow)
{
    TWeakObjectPtr<AEntity> EntityPtr;
    const bool bHadEntry = EntityMap.RemoveAndCopyValue(RemovedRow.EntityId, EntityPtr);
    const bool bIsValid =EntityPtr.IsValid();
    if (!bHadEntry || !bIsValid)
    {
        return;
    }

    if (AEntity* Entity = EntityPtr.Get())
    {
        Entity->OnDelete(Context);
    }
}

void AGameManager::OnFoodInsert(const FEventContext& Context, const FFoodType& NewRow)
{
    if (EntityMap.Contains(NewRow.EntityId)) return;
    SpawnFood(NewRow);
}

void AGameManager::OnPlayerInsert(const FEventContext& Context, const FPlayerType& NewRow)
{
    SpawnOrGetPlayer(NewRow);
}

void AGameManager::OnPlayerDelete(const FEventContext& Context, const FPlayerType& RemovedRow)
{
    TWeakObjectPtr<APlayerPawn> PlayerPtr;
    const bool bHadEntry = PlayerMap.RemoveAndCopyValue(RemovedRow.PlayerId, PlayerPtr);

    if (!bHadEntry || !PlayerPtr.IsValid())
    {
        return;
    }

    if (APlayerPawn* Player = PlayerPtr.Get())
    {
        Player->Destroy();
    }
}

APlayerPawn* AGameManager::SpawnOrGetPlayer(const FPlayerType& PlayerRow)
{
    TWeakObjectPtr<APlayerPawn> WeakPlayer = PlayerMap.FindRef(PlayerRow.PlayerId);
    if (WeakPlayer.IsValid())
    {
        return WeakPlayer.Get();
    }

    if (!PlayerClass)
    {
        UE_LOG(LogTemp, Error, TEXT("GameManager - PlayerClass not set."));
        return nullptr;
    }
    FActorSpawnParameters Params;
    Params.SpawnCollisionHandlingOverride = ESpawnActorCollisionHandlingMethod::AlwaysSpawn;
    APlayerPawn* Player = GetWorld()->SpawnActor<APlayerPawn>(PlayerClass, FVector::ZeroVector, FRotator::ZeroRotator, Params);
    if (Player)
    {
        Player->Initialize(PlayerRow);
        PlayerMap.Add(PlayerRow.PlayerId, Player);
    }
    return Player;
}

ACircle* AGameManager::SpawnCircle(const FCircleType& CircleRow)
{
    if (!CircleClass)
    {
        UE_LOG(LogTemp, Error, TEXT("GameManager - CircleClass not set."));
        return nullptr;
    }
    // Need player row for username
    const FPlayerType PlayerRow = Conn->Db->Player->PlayerId->Find(CircleRow.PlayerId);
    APlayerPawn* OwningPlayer = SpawnOrGetPlayer(PlayerRow);

    FActorSpawnParameters Params;
    auto* Circle = GetWorld()->SpawnActor<ACircle>(CircleClass, FVector::ZeroVector, FRotator::ZeroRotator, Params);
    if (Circle)
    {
        Circle->Spawn(CircleRow, OwningPlayer);
        EntityMap.Add(CircleRow.EntityId, Circle);
        if (OwningPlayer)
            OwningPlayer->OnCircleSpawned(Circle);
    }
    return Circle;
}

AFood* AGameManager::SpawnFood(const FFoodType& FoodEntity)
{
    if (!FoodClass)
    {
        UE_LOG(LogTemp, Error, TEXT("GameManager - FoodClass not set."));
        return nullptr;
    }

    FActorSpawnParameters Params;
    AFood* Food = GetWorld()->SpawnActor<AFood>(FoodClass, FVector::ZeroVector, FRotator::ZeroRotator, Params);
    if (Food)
    {
        Food->Spawn(FoodEntity);
        EntityMap.Add(FoodEntity.EntityId, Food);
    }
    return Food;
}
```
</TabItem>
<TabItem value="blueprint" label="Blueprint">
Open `BP_GameManager` and add the following **Variables**:

1. Add `CircleClass`
    - Change **Variable Type** to **BP Circle -> Class Reference**
    - Check **Instance Editable**
    - Change **Category** to `Classes`
    - Change **Default Value** to `BP_Circle`
2. Add `FoodClass`
    - Change **Variable Type** to **BP Food -> Class Reference**
    - Check **Instance Editable**
    - Change **Category** to `Classes`
    - Change **Default Value** to `BP_Food`
3. Add `PlayerClass`
    - Change **Variable Type** to **BP Player Pawn -> Class Reference**
    - Check **Instance Editable**
    - Change **Category** to `Classes`
    - Change **Default Value** to `BP_PlayerPawn`
4. Add `EntityMap`
    - Change **Variable Type** to an **Integer**
    - Change **Variable Type** to **Map** and set value type to **BP Entity -> Object Reference**
5. Add `PlayerMap`
    - Change **Variable Type** to an **Integer**
    - Change **Variable Type** to **Map** and set value type to **BP Player Pawn -> Object Reference**

![Add Variables](/images/unreal/part-3-03-blueprint-gamemanager-1.png)

Add **Function** named `SpawnOrGetPlayer` as follows:
![Add SpawnOrGetPlayer](/images/unreal/part-3-03-blueprint-gamemanager-3.png)

- Add **Input** as `PlayerRow` with **Player Type** as the type.
- Add **Output** as `PlayerPawn` with **BP Player Pawn -> Object Reference** as the type.

With the functions and variables in place next we'll expand the **EventGraph**:

Extened **OnConnect_Event** as follows:
![Update OnConnect_Event](/images/unreal/part-3-03-blueprint-gamemanager-4.png)
![Update OnConnect_Event](/images/unreal/part-3-03-blueprint-gamemanager-5.png)

> **Note:** For the events the naming scheme for this tutorial is `<Type>_<Event>_Event` for example `Circle_OnInsert_Event`.

Update **Circle_OnInsert_Event** as follows:
![Update Circle_OnInsert_Event](/images/unreal/part-3-03-blueprint-gamemanager-6.png)

Update **Entity_OnUpdate_Event** as follows:
![Update Entity_OnUpdate_Event](/images/unreal/part-3-03-blueprint-gamemanager-7.png)

Update **Entity_OnDelete_Event** as follows:
![Update Entity_OnDelete_Event](/images/unreal/part-3-03-blueprint-gamemanager-8.png)

Update **Food_OnInsert_Event** as follows:
![Update Food_OnInsert_Event](/images/unreal/part-3-03-blueprint-gamemanager-9.png)

Update **Player_OnInsert_Event** and **Player_OnDelete_Event** as follows:
![Update Player Events](/images/unreal/part-3-03-blueprint-gamemanager-10.png)
</TabItem>
</Tabs>

### Player Controller

In most Unreal projects, proper input handling depends on setting up the PlayerController.  
We’ll finish that setup in the next part of the tutorial. For now, add the possession logic.

<Tabs groupId="client-language" defaultValue="cpp">
<TabItem value="cpp" label="C++">

Edit `BlackholioPlayerController.h` as follows:

```cpp
#pragma once

#include "CoreMinimal.h"
#include "PlayerPawn.h"
#include "GameFramework/PlayerController.h"
#include "BlackholioPlayerController.generated.h"

class APlayerPawn;

UCLASS()
class CLIENT_UNREAL_API ABlackholioPlayerController : public APlayerController
{
    GENERATED_BODY()

public:
    ABlackholioPlayerController();

protected:
    virtual void Tick(float DeltaSeconds) override;
    virtual void OnPossess(APawn* InPawn) override;
    FVector2D ComputeDesiredDirection() const;

private:
    UPROPERTY()
    TObjectPtr<APlayerPawn> LocalPlayer;

    UPROPERTY()
    float SendUpdatesFrequency = 0.0333f;
    float LastMovementSendTimestamp = 0.f;

    TOptional<FVector2D> LockInputPosition;
};
```

Update `BlackholioPlayerController.cpp` (the movement logic will be added in the next part):

```cpp
#include "BlackholioPlayerController.h"
#include "DbVector2.h"
#include "GameManager.h"
#include "PlayerPawn.h"

ABlackholioPlayerController::ABlackholioPlayerController()
{
    bShowMouseCursor = true;
    bEnableClickEvents = true;
    bEnableMouseOverEvents = true;
    PrimaryActorTick.bCanEverTick = true;
}

void ABlackholioPlayerController::Tick(float DeltaSeconds)
{
    Super::Tick(DeltaSeconds);
}

void ABlackholioPlayerController::OnPossess(APawn* InPawn)
{
    Super::OnPossess(InPawn);
    LocalPlayer = Cast<APlayerPawn>(InPawn);
}

FVector2D ABlackholioPlayerController::ComputeDesiredDirection() const
{
    return FVector2D::ZeroVector;
}
```
</TabItem>
<TabItem value="blueprint" label="Blueprint">

Last update `BP_PlayerController` for the basics by adding the following **Variables**:

1. Add `GameManger`
    - Change **Variable Type** to **BP Game Manager -> Object Reference**
2. Add `LocalPlayer`
    - Change **Variable Type** to **BP Player Pawn -> Object Reference**
3. Add `LastMovementSendTime`
    - Change **Variable Type** to **Float**
4. Add `SendUpdateFrequency`
    - Change **Variable Type** to **Float**
    - Change **Default Value** to `0.0333`

Add **Function** named `GetGameManager` as follows:
![Add GetGameManager](/images/unreal/part-3-04-blueprint-playercontroller-1.png)

- Add **Output** as `GameManager` with **BP Game Manager** as the type.

Override **Function -> On Possess** as follows:
![Add GetGameManager](/images/unreal/part-3-04-blueprint-playercontroller-2.png)

Update **Event BeginPlay** as follows:
![Update BeginPlay](/images/unreal/part-3-04-blueprint-playercontroller-3.png)

</TabItem>
</Tabs>

### Entering the Game

At this point, you may need to regenerate your bindings the following command from the `blackholio/spacetimedb` directory.

```sh
spacetime generate --lang unrealcpp --uproject-dir .. --module-name blackholio
```

<Tabs groupId="client-language" defaultValue="cpp">
<TabItem value="cpp" label="C++">
The last step is to call the `enter_game` reducer on the server, passing in a username for the player.
For simplicity, call `enter_game` from the `HandleSubscriptionApplied` callback with the name `TestPlayer`.

Open up `GameManager.cpp` and edit `HandleSubscriptionApplied` to match the following:

```cpp
void AGameManager::HandleSubscriptionApplied(FSubscriptionEventContext& Context)
{
    UE_LOG(LogTemp, Log, TEXT("Subscription applied!"));

    // Once we have the initial subscription sync'd to the client cache
    // Get the world size from the config table and set up the arena
    int64 WorldSize = Conn->Db->Config->Id->Find(0).WorldSize;
    SetupArena(WorldSize);

    Context.Reducers->EnterGame("TestPlayer");
}
```

:::warning
Be sure to rebuild your project after making changes to the code.
:::

</TabItem>
<TabItem value="blueprint" label="Blueprint">

The last step is to call the `enter_game` reducer on the server, passing in a username for the player.
For simplicity, call `enter_game` from the `OnApplied_Event` callback with the name `TestPlayer`.

Open up `BP_GameManager` and edit `OnApplied_Event` to match the following:

![Update OnApplied_Event](/images/unreal/part-3-05-blueprint-gamemanager-1.png)
</TabItem>
</Tabs>

### Trying It Out

<Tabs groupId="client-language" defaultValue="cpp">
<TabItem value="cpp" label="C++">

Almost everything is ready to play. Before launching, set up the spawning classes:

1. Open `BP_GameManager`.
2. Update the spawning classes:
   - **Circle Class** → `BP_Circle`
   - **Food Class** → `BP_Food`
   - **Player Class** → `BP_PlayerPawn`

:::warning
Compile and save your changes.
:::

Next, wire up `SetUsername` to update the Nameplate:

1. Open `BP_Circle`.
2. In **Event BeginPlay**, add the following:

![Nameplate Update](/images/unreal/part-3-04-nameplate-change.png)
</TabItem>
<TabItem value="blueprint" label="Blueprint">
Almost everything is ready to play. Before launching, set up the spawning classes:

1. Open `BP_GameManager`.
2. Update the spawning classes:
   - **Circle Class** → `BP_Circle`
   - **Food Class** → `BP_Food`
   - **Player Class** → `BP_PlayerPawn`

:::warning
Compile and save your changes.
:::
</TabItem>
</Tabs>
---

After publishing the module, press **Play** to see it in action.  
You should see your player’s circle with its username label, surrounded by food.

### Next Steps

It's pretty cool to see our player in game surrounded by food, but there's a problem! We can't move yet. In the next part, we'll explore how to get your player moving and interacting with food and other objects.

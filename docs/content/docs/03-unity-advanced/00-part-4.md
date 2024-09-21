---
title: Unity Tutorial - Advanced - Part 4 - Resources and Scheduling
navTitle: 4 - Resources & Scheduling
---

Need help with the tutorial? [Join our Discord server](https://discord.gg/spacetimedb)!

This progressive tutorial is continued from the [Part 3](/docs/unity-tutorial/part-3) Tutorial.

**Oct 14th, 2023: This tutorial has not yet been updated for the recent 0.7.0 release, it will be updated asap!**

In this second part of the lesson, we'll add resource nodes to our project and learn about scheduled reducers. Then we will spawn the nodes on the client so they are visible to the player.

## Add Resource Node Spawner

In this section we will add functionality to our server to spawn the resource nodes.

### Step 1: Add the SpacetimeDB Tables for Resource Nodes

1. Before we start adding code to the server, we need to add the ability to use the rand crate in our SpacetimeDB module so we can generate random numbers. Open the `Cargo.toml` file in the `Server` directory and add the following line to the `[dependencies]` section.

```toml
rand = "0.8.5"
```

We also need to add the `getrandom` feature to our SpacetimeDB crate. Update the `spacetimedb` line to:

```toml
spacetimedb = { "0.5", features = ["getrandom"] }
```

2. The first entity component we are adding, `ResourceNodeComponent`, stores the resource type. We'll define an enum to describe a `ResourceNodeComponent`'s type. For now, we'll just have one resource type: Iron. In the future, though, we'll add more resources by adding variants to the `ResourceNodeType` enum. Since we are using a custom enum, we need to mark it with the `SpacetimeType` attribute. Add the following code to lib.rs.

```rust
#[derive(SpacetimeType, Clone)]
pub enum ResourceNodeType {
    Iron,
}

#[spacetimedb(table(public))]
#[derive(Clone)]
pub struct ResourceNodeComponent {
    #[primarykey]
    pub entity_id: u64,

    // Resource type of this resource node
    pub resource_type: ResourceNodeType,
}
```

Because resource nodes never move, the `MobileEntityComponent` is overkill. Instead, we will add a new entity component named `StaticLocationComponent` that only stores the position and rotation.

```rust
#[spacetimedb(table(public))]
#[derive(Clone)]
pub struct StaticLocationComponent {
    #[primarykey]
    pub entity_id: u64,

    pub location: StdbVector2,
    pub rotation: f32,
}
```

3. We are also going to add a couple of additional column to our Config table. `map_extents` let's our spawner know where it can spawn the nodes. `num_resource_nodes` is the maximum number of nodes to spawn on the map. Update the config table in lib.rs.

```rust
#[spacetimedb(table(public))]
pub struct Config {
    // Config is a global table with a single row. This table will be used to
    // store configuration or global variables

    #[primarykey]
    // always 0
    // having a table with a primarykey field which is always zero is a way to store singleton global state
    pub version: u32,

    pub message_of_the_day: String,

    // new variables for resource node spawner
    // X and Z range of the map (-map_extents to map_extents)
    pub map_extents: u32,
    // maximum number of resource nodes to spawn on the map
    pub num_resource_nodes: u32,
}
```

4. In the `init` reducer, we need to set the initial values of our two new variables. Update the following code:

```rust
    Config::insert(Config {
        version: 0,
        message_of_the_day: "Hello, World!".to_string(),

        // new variables for resource node spawner
        map_extents: 25,
        num_resource_nodes: 10,
    })
    .expect("Failed to insert config.");
```

### Step 2: Write our Resource Spawner Repeating Reducer

1. Add the following code to lib.rs. As we want to schedule `resource_spawn_agent` to run later, It will require to implement a scheduler table.

```rust
#[spacetimedb(table, scheduled(resource_spawner_agent))]
struct ResouceSpawnAgentSchedueler {
    _prev_time: Timestamp,
}

#[spacetimedb(reducer)
pub fn resource_spawner_agent(_ctx: ReducerContext, _arg: ResourceSpawnAgentScheduler) -> Result<(), String> {
    let config = Config::find_by_version(&0).unwrap();

    // Retrieve the maximum number of nodes we want to spawn from the Config table
    let num_resource_nodes = config.num_resource_nodes as usize;

    // Count the number of nodes currently spawned and exit if we have reached num_resource_nodes
    let num_resource_nodes_spawned = ResourceNodeComponent::iter().count();
    if num_resource_nodes_spawned >= num_resource_nodes {
        log::info!("All resource nodes spawned. Skipping.");
        return Ok(());
    }

    // Pick a random X and Z based off the map_extents
    let mut rng = rand::thread_rng();
    let map_extents = config.map_extents as f32;
    let location = StdbVector2 {
        x: rng.gen_range(-map_extents..map_extents),
        z: rng.gen_range(-map_extents..map_extents),
    };
    // Pick a random Y rotation in degrees
    let rotation = rng.gen_range(0.0..360.0);

    // Insert our SpawnableEntityComponent which assigns us our entity_id
    let entity_id = SpawnableEntityComponent::insert(SpawnableEntityComponent { entity_id: 0 })
        .expect("Failed to create resource spawnable entity component.")
        .entity_id;

    // Insert our static location with the random position and rotation we selected
    StaticLocationComponent::insert(StaticLocationComponent {
        entity_id,
        location: location.clone(),
        rotation,
    })
    .expect("Failed to insert resource static location component.");

    // Insert our resource node component, so far we only have iron
    ResourceNodeComponent::insert(ResourceNodeComponent {
        entity_id,
        resource_type: ResourceNodeType::Iron,
    })
    .expect("Failed to insert resource node component.");

    // Log that we spawned a node with the entity_id and location
    log::info!(
        "Resource node spawned: {} at ({}, {})",
        entity_id,
        location.x,
        location.z,
    );

    Ok(())
}
```

2. Since this reducer uses `rand::Rng` we need add include it. Add this `use` statement to the top of lib.rs.

```rust
use rand::Rng;
```

3. Add the following code to the end of the `init` reducer to set the reducer to repeat at every regular interval.

```rust
    // Start our resource spawner repeating reducer
    ResouceSpawnAgentSchedueler::insert(ResouceSpawnAgentSchedueler {
        _prev_time: TimeStamp::now(),
        scheduled_id: 1,
        scheduled_at: duration!(1000ms).into()
    }).expect();
```

struct ResouceSpawnAgentSchedueler {

4. Next we need to generate our client code and publish the module. Since we changed the schema we need to make sure we include the `--clear-database` flag. Run the following commands from your Server directory:

```bash
spacetime generate --out-dir ../Assets/autogen --lang=csharp

spacetime publish -c yourname/bitcraftmini
```

Your resource node spawner will start as soon as you publish since we scheduled it to run in our init reducer. You can watch the log output by using the `--follow` flag on the logs CLI command.

```bash
spacetime logs -f yourname/bitcraftmini
```

### Step 3: Spawn the Resource Nodes on the Client

1. First we need to update the `GameResource` component in Unity to work for multiplayer. Open GameResource.cs and add `using SpacetimeDB.Types;` to the top of the file. Then change the variable `Type` to be of type `ResourceNodeType` instead of `int`. Also add a new variable called `EntityId` of type `ulong`.

```csharp
    public ulong EntityId;

    public ResourceNodeType Type = ResourceNodeType.Iron;
```

2. Now that we've changed the `Type` variable, we need to update the code in the `PlayerAnimator` component that references it. Open PlayerAnimator.cs and update the following section of code. We need to add `using SpacetimeDB.Types;` to this file as well. This fixes the compile errors that result from changing the type of the `Type` variable to our new server generated enum.

```csharp
            var resourceType = res?.Type ?? ResourceNodeType.Iron;
            switch (resourceType)
            {
                case ResourceNodeType.Iron:
                    _animator.SetTrigger("Mine");
                    Interacting = true;
                    break;
                default:
                    Interacting = false;
                    break;
            }
            for (int i = 0; i < _tools.Length; i++)
            {
                _tools[i].SetActive(((int)resourceType) == i);
            }
            _target = res;
```

3. Now that our `GameResource` is ready to be spawned, lets update the `BitcraftMiniGameManager` component to actually create them. First, we need to add the new tables to our SpacetimeDB subscription. Open BitcraftMiniGameManager.cs and update the following code:

```csharp
            SpacetimeDBClient.instance.Subscribe(new List<string>()
            {
                "SELECT * FROM Config",
                "SELECT * FROM SpawnableEntityComponent",
                "SELECT * FROM PlayerComponent",
                "SELECT * FROM MobileEntityComponent",
                // Our new tables for part 2 of the tutorial
                "SELECT * FROM ResourceNodeComponent",
                "SELECT * FROM StaticLocationComponent"
            });
```

4. Next let's add an `OnInsert` handler for the `ResourceNodeComponent`. Add the following line to the `Start` function.

```csharp
        ResourceNodeComponent.OnInsert += ResourceNodeComponent_OnInsert;
```

5. Finally we add the new function to handle the insert event. This function will be called whenever a new `ResourceNodeComponent` is inserted into our local client cache. We can use this to spawn the resource node in the world. Add the following code to the `BitcraftMiniGameManager` class.

To get the position and the rotation of the node, we look up the `StaticLocationComponent` for this entity by using the EntityId.

```csharp
    private void ResourceNodeComponent_OnInsert(ResourceNodeComponent insertedValue, ReducerEvent callInfo)
    {
        switch(insertedValue.ResourceType)
        {
            case ResourceNodeType.Iron:
                var iron = Instantiate(IronPrefab);
                StaticLocationComponent loc = StaticLocationComponent.FindByEntityId(insertedValue.EntityId);
                Vector3 nodePos = new Vector3(loc.Location.X, 0.0f, loc.Location.Z);
                iron.transform.position = new Vector3(nodePos.x, MathUtil.GetTerrainHeight(nodePos), nodePos.z);
                iron.transform.rotation = Quaternion.Euler(0.0f, loc.Rotation, 0.0f);
                break;
        }
    }
```

### Step 4: Play the Game!

6. Hit Play in the Unity Editor and you should now see your resource nodes spawning in the world!

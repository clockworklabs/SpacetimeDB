---
title: 4 - Moving and Colliding
slug: /unity/part-4
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';

# Moving and Colliding

Need help with the tutorial? [Join our Discord server](https://discord.gg/spacetimedb)!

This progressive tutorial is continued from [part 3](/unity/part-3).

### Moving the player

At this point, we're very close to having a working game. All we have to do is modify our server to allow the player to move around, and to simulate the physics and collisions of the game.

<Tabs groupId="server-language" defaultValue="rust">
<TabItem value="rust" label="Rust" >
Let's start by building out a simple math library to help us do collision calculations. Create a new `math.rs` file in the `server-rust/src` directory and add the following contents. Let's also move the `DbVector2` type from `lib.rs` into this file.

```rust
use spacetimedb::SpacetimeType;

// This allows us to store 2D points in tables.
#[derive(SpacetimeType, Debug, Clone, Copy)]
pub struct DbVector2 {
    pub x: f32,
    pub y: f32,
}

impl std::ops::Add<&DbVector2> for DbVector2 {
    type Output = DbVector2;

    fn add(self, other: &DbVector2) -> DbVector2 {
        DbVector2 {
            x: self.x + other.x,
            y: self.y + other.y,
        }
    }
}

impl std::ops::Add<DbVector2> for DbVector2 {
    type Output = DbVector2;

    fn add(self, other: DbVector2) -> DbVector2 {
        DbVector2 {
            x: self.x + other.x,
            y: self.y + other.y,
        }
    }
}

impl std::ops::AddAssign<DbVector2> for DbVector2 {
    fn add_assign(&mut self, rhs: DbVector2) {
        self.x += rhs.x;
        self.y += rhs.y;
    }
}

impl std::iter::Sum<DbVector2> for DbVector2 {
    fn sum<I: Iterator<Item = DbVector2>>(iter: I) -> Self {
        let mut r = DbVector2::new(0.0, 0.0);
        for val in iter {
            r += val;
        }
        r
    }
}

impl std::ops::Sub<&DbVector2> for DbVector2 {
    type Output = DbVector2;

    fn sub(self, other: &DbVector2) -> DbVector2 {
        DbVector2 {
            x: self.x - other.x,
            y: self.y - other.y,
        }
    }
}

impl std::ops::Sub<DbVector2> for DbVector2 {
    type Output = DbVector2;

    fn sub(self, other: DbVector2) -> DbVector2 {
        DbVector2 {
            x: self.x - other.x,
            y: self.y - other.y,
        }
    }
}

impl std::ops::SubAssign<DbVector2> for DbVector2 {
    fn sub_assign(&mut self, rhs: DbVector2) {
        self.x -= rhs.x;
        self.y -= rhs.y;
    }
}

impl std::ops::Mul<f32> for DbVector2 {
    type Output = DbVector2;

    fn mul(self, other: f32) -> DbVector2 {
        DbVector2 {
            x: self.x * other,
            y: self.y * other,
        }
    }
}

impl std::ops::Div<f32> for DbVector2 {
    type Output = DbVector2;

    fn div(self, other: f32) -> DbVector2 {
        if other != 0.0 {
            DbVector2 {
                x: self.x / other,
                y: self.y / other,
            }
        } else {
            DbVector2 { x: 0.0, y: 0.0 }
        }
    }
}

impl DbVector2 {
    pub fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    pub fn sqr_magnitude(&self) -> f32 {
        self.x * self.x + self.y * self.y
    }

    pub fn magnitude(&self) -> f32 {
        (self.x * self.x + self.y * self.y).sqrt()
    }

    pub fn normalized(self) -> DbVector2 {
        self / self.magnitude()
    }
}
```

At the very top of `lib.rs` add the following lines to import the moved `DbVector2` from the `math` module.

```rust
pub mod math;

use math::DbVector2;
// ...
```

Next, add the following reducer to your `lib.rs` file.

```rust
#[spacetimedb::reducer]
pub fn update_player_input(ctx: &ReducerContext, direction: DbVector2) -> Result<(), String> {
    let player = ctx
        .db
        .player()
        .identity()
        .find(&ctx.sender)
        .ok_or("Player not found")?;
    for mut circle in ctx.db.circle().player_id().filter(&player.player_id) {
        circle.direction = direction.normalized();
        circle.speed = direction.magnitude().clamp(0.0, 1.0);
        ctx.db.circle().entity_id().update(circle);
    }
    Ok(())
}
```

This is a simple reducer that takes the movement input from the client and applies them to all circles that that player controls. Note that it is not possible for a player to move another player's circles using this reducer, because the `ctx.sender` value is not set by the client. Instead `ctx.sender` is set by SpacetimeDB after it has authenticated that sender. You can rest assured that the caller has been authenticated as that player by the time this reducer is called.

</TabItem>
<TabItem value="csharp" label="C#" >
Let's start by building out a simple math library to help us do collision calculations. Create a new `Math.cs` file in the `csharp-server` directory and add the following contents. Let's also remove the `DbVector2` type from `Lib.cs`.

```csharp
[SpacetimeDB.Type]
public partial struct DbVector2
{
    public float x;
    public float y;

    public DbVector2(float x, float y)
    {
        this.x = x;
        this.y = y;
    }

    public float SqrMagnitude => x * x + y * y;
    public float Magnitude => MathF.Sqrt(SqrMagnitude);
    public DbVector2 Normalized => this / Magnitude;

    public static DbVector2 operator +(DbVector2 a, DbVector2 b) => new DbVector2(a.x + b.x, a.y + b.y);
    public static DbVector2 operator -(DbVector2 a, DbVector2 b) => new DbVector2(a.x - b.x, a.y - b.y);
    public static DbVector2 operator *(DbVector2 a, float b) => new DbVector2(a.x * b, a.y * b);
    public static DbVector2 operator /(DbVector2 a, float b) => new DbVector2(a.x / b, a.y / b);
}
```

Next, add the following reducer to the `Module` class of your `Lib.cs` file.

```csharp
[Reducer]
public static void UpdatePlayerInput(ReducerContext ctx, DbVector2 direction)
{
    var player = ctx.Db.player.identity.Find(ctx.Sender) ?? throw new Exception("Player not found");
    foreach (var c in ctx.Db.circle.player_id.Filter(player.player_id))
    {
        var circle = c;
        circle.direction = direction.Normalized;
        circle.speed = Math.Clamp(direction.Magnitude, 0f, 1f);
        ctx.Db.circle.entity_id.Update(circle);
    }
}
```

This is a simple reducer that takes the movement input from the client and applies them to all circles that that player controls. Note that it is not possible for a player to move another player's circles using this reducer, because the `ctx.Sender` value is not set by the client. Instead `ctx.Sender` is set by SpacetimeDB after it has authenticated that sender. You can rest assured that the caller has been authenticated as that player by the time this reducer is called.

</TabItem>
</Tabs>

Finally, let's schedule a reducer to run every 50 milliseconds to move the player's circles around based on the most recently set player input.

<Tabs groupId="server-language" defaultValue="rust">
<TabItem value="rust" label="Rust" >

```rust
#[spacetimedb::table(name = move_all_players_timer, scheduled(move_all_players))]
pub struct MoveAllPlayersTimer {
    #[primary_key]
    #[auto_inc]
    scheduled_id: u64,
    scheduled_at: spacetimedb::ScheduleAt,
}

const START_PLAYER_SPEED: i32 = 10;

fn mass_to_max_move_speed(mass: i32) -> f32 {
    2.0 * START_PLAYER_SPEED as f32 / (1.0 + (mass as f32 / START_PLAYER_MASS as f32).sqrt())
}

#[spacetimedb::reducer]
pub fn move_all_players(ctx: &ReducerContext, _timer: MoveAllPlayersTimer) -> Result<(), String> {
    let world_size = ctx
        .db
        .config()
        .id()
        .find(0)
        .ok_or("Config not found")?
        .world_size;

    // Handle player input
    for circle in ctx.db.circle().iter() {
        let circle_entity = ctx.db.entity().entity_id().find(&circle.entity_id);
        if !circle_entity.is_some() {
            // This can happen if a circle is eaten by another circle
            continue;
        }
        let mut circle_entity = circle_entity.unwrap();
        let circle_radius = mass_to_radius(circle_entity.mass);
        let direction = circle.direction * circle.speed;
        let new_pos =
            circle_entity.position + direction * mass_to_max_move_speed(circle_entity.mass);
        let min = circle_radius;
        let max = world_size as f32 - circle_radius;
        circle_entity.position.x = new_pos.x.clamp(min, max);
        circle_entity.position.y = new_pos.y.clamp(min, max);
        ctx.db.entity().entity_id().update(circle_entity);
    }

    Ok(())
}
```

</TabItem>
<TabItem value="csharp" label="C#" >

```csharp
[Table(Name = "move_all_players_timer", Scheduled = nameof(MoveAllPlayers), ScheduledAt = nameof(scheduled_at))]
public partial struct MoveAllPlayersTimer
{
    [PrimaryKey, AutoInc]
    public ulong scheduled_id;
    public ScheduleAt scheduled_at;
}

const int START_PLAYER_SPEED = 10;

public static float MassToMaxMoveSpeed(int mass) => 2f * START_PLAYER_SPEED / (1f + MathF.Sqrt((float)mass / START_PLAYER_MASS));

[Reducer]
public static void MoveAllPlayers(ReducerContext ctx, MoveAllPlayersTimer timer)
{
    var world_size = (ctx.Db.config.id.Find(0) ?? throw new Exception("Config not found")).world_size;

    var circle_directions = ctx.Db.circle.Iter().Select(c => (c.entity_id, c.direction * c.speed)).ToDictionary();

    // Handle player input
    foreach (var circle in ctx.Db.circle.Iter())
    {
        var check_entity = ctx.Db.entity.entity_id.Find(circle.entity_id);
        if (check_entity == null)
        {
            // This can happen if the circle has been eaten by another circle.
            continue;
        }
        var circle_entity = check_entity.Value;
        var circle_radius = MassToRadius(circle_entity.mass);
        var direction = circle_directions[circle.entity_id];
        var new_pos = circle_entity.position + direction * MassToMaxMoveSpeed(circle_entity.mass);
        circle_entity.position.x = Math.Clamp(new_pos.x, circle_radius, world_size - circle_radius);
        circle_entity.position.y = Math.Clamp(new_pos.y, circle_radius, world_size - circle_radius);
        ctx.Db.entity.entity_id.Update(circle_entity);
    }
}
```

</TabItem>
</Tabs>

This reducer is very similar to a standard game "tick" or "frame" that you might find in an ordinary game server or similar to something like the `Update` loop in a game engine like Unity. We've scheduled it every 50 milliseconds and we can use it to step forward our simulation by moving all the circles a little bit further in the direction they're moving.

In this reducer, we're just looping through all the circles in the game and updating their position based on their direction, speed, and mass. Just basic physics.

<Tabs groupId="server-language" defaultValue="rust">
<TabItem value="rust" label="Rust" >
Add the following to your `init` reducer to schedule the `move_all_players` reducer to run every 50 milliseconds.

```rust
ctx.db
    .move_all_players_timer()
    .try_insert(MoveAllPlayersTimer {
        scheduled_id: 0,
        scheduled_at: ScheduleAt::Interval(Duration::from_millis(50).into()),
    })?;
```

</TabItem>
<TabItem value="csharp" label="C#" >
Add the following to your `Init` reducer to schedule the `MoveAllPlayers` reducer to run every 50 milliseconds.

```csharp
ctx.Db.move_all_players_timer.Insert(new MoveAllPlayersTimer
{
    scheduled_at = new ScheduleAt.Interval(TimeSpan.FromMilliseconds(50))
});
```

</TabItem>
</Tabs>

Republish your module with:

```sh
spacetime publish --server local blackholio --delete-data
```

Regenerate your server bindings with:

```sh
spacetime generate --lang csharp --out-dir ../Assets/module_bindings
```

### Moving on the Client

All that's left is to modify our `PlayerController` on the client to call the `update_player_input` reducer. Open `PlayerController.cs` and add an `Update` function:

```cs
public void Update()
{
    if (!IsLocalPlayer || NumberOfOwnedCircles == 0)
    {
        return;
    }

    if (Input.GetKeyDown(KeyCode.Q))
    {
        if (LockInputPosition.HasValue)
        {
            LockInputPosition = null;
        }
        else
        {
            LockInputPosition = (Vector2)Input.mousePosition;
        }
    }

    // Throttled input requests
    if (Time.time - LastMovementSendTimestamp >= SEND_UPDATES_FREQUENCY)
    {
        LastMovementSendTimestamp = Time.time;

        var mousePosition = LockInputPosition ?? (Vector2)Input.mousePosition;
        var screenSize = new Vector2
        {
            x = Screen.width,
            y = Screen.height,
        };
        var centerOfScreen = screenSize / 2;

        var direction = (mousePosition - centerOfScreen) / (screenSize.y / 3);
        if (testInputEnabled) { direction = testInput; }
        GameManager.Conn.Reducers.UpdatePlayerInput(direction);
    }
}
```

Let's try it out! Press play and roam freely around the arena! Now we're cooking with gas.

### Collisions and Eating Food

Well this is pretty fun, but wouldn't it be better if we could eat food and grow our circle? Surely, that's going to be a pain, right?

<Tabs groupId="server-language" defaultValue="rust">
<TabItem value="rust" label="Rust" >
Wrong. With SpacetimeDB it's extremely easy. All we have to do is add an `is_overlapping` helper function which does some basic math based on mass radii, and modify our `move_all_player` reducer to loop through every entity in the arena for every circle, checking each for overlaps. This may not be the most efficient way to do collision checking (building a quad tree or doing [spatial hashing](https://conkerjo.wordpress.com/2009/06/13/spatial-hashing-implementation-for-fast-2d-collisions/) might be better), but SpacetimeDB is very fast so for this number of entities it'll be a breeze for SpacetimeDB.

Sometimes simple is best! Add the following code to your `lib.rs` file and make sure to replace the existing `move_all_players` reducer.

```rust
const MINIMUM_SAFE_MASS_RATIO: f32 = 0.85;

fn is_overlapping(a: &Entity, b: &Entity) -> bool {
    let dx = a.position.x - b.position.x;
    let dy = a.position.y - b.position.y;
    let distance_sq = dx * dx + dy * dy;

    let radius_a = mass_to_radius(a.mass);
    let radius_b = mass_to_radius(b.mass);

    // If the distance between the two circle centers is less than the
    // maximum radius, then the center of the smaller circle is inside
    // the larger circle. This gives some leeway for the circles to overlap
    // before being eaten.
    let max_radius = f32::max(radius_a, radius_b);
    distance_sq <= max_radius * max_radius
}

#[spacetimedb::reducer]
pub fn move_all_players(ctx: &ReducerContext, _timer: MoveAllPlayersTimer) -> Result<(), String> {
    let world_size = ctx
        .db
        .config()
        .id()
        .find(0)
        .ok_or("Config not found")?
        .world_size;

    // Handle player input
    for circle in ctx.db.circle().iter() {
        let circle_entity = ctx.db.entity().entity_id().find(&circle.entity_id);
        if !circle_entity.is_some() {
            // This can happen if a circle is eaten by another circle
            continue;
        }
        let mut circle_entity = circle_entity.unwrap();
        let circle_radius = mass_to_radius(circle_entity.mass);
        let direction = circle.direction * circle.speed;
        let new_pos =
            circle_entity.position + direction * mass_to_max_move_speed(circle_entity.mass);
        let min = circle_radius;
        let max = world_size as f32 - circle_radius;
        circle_entity.position.x = new_pos.x.clamp(min, max);
        circle_entity.position.y = new_pos.y.clamp(min, max);

        // Check collisions
        for entity in ctx.db.entity().iter() {
            if entity.entity_id == circle_entity.entity_id {
                continue;
            }
            if is_overlapping(&circle_entity, &entity) {
                // Check to see if we're overlapping with food
                if ctx.db.food().entity_id().find(&entity.entity_id).is_some() {
                    ctx.db.entity().entity_id().delete(&entity.entity_id);
                    ctx.db.food().entity_id().delete(&entity.entity_id);
                    circle_entity.mass += entity.mass;
                }

                // Check to see if we're overlapping with another circle owned by another player
                let other_circle = ctx.db.circle().entity_id().find(&entity.entity_id);
                if let Some(other_circle) = other_circle {
                    if other_circle.player_id != circle.player_id {
                        let mass_ratio = entity.mass as f32 / circle_entity.mass as f32;
                        if mass_ratio < MINIMUM_SAFE_MASS_RATIO {
                            ctx.db.entity().entity_id().delete(&entity.entity_id);
                            ctx.db.circle().entity_id().delete(&entity.entity_id);
                            circle_entity.mass += entity.mass;
                        }
                    }
                }
            }
        }
        ctx.db.entity().entity_id().update(circle_entity);
    }

    Ok(())
}
```

</TabItem>
<TabItem value="csharp" label="C#" >
Wrong. With SpacetimeDB it's extremely easy. All we have to do is add an `IsOverlapping` helper function which does some basic math based on mass radii, and modify our `MoveAllPlayers` reducer to loop through every entity in the arena for every circle, checking each for overlaps. This may not be the most efficient way to do collision checking (building a quad tree or doing [spatial hashing](https://conkerjo.wordpress.com/2009/06/13/spatial-hashing-implementation-for-fast-2d-collisions/) might be better), but SpacetimeDB is very fast so for this number of entities it'll be a breeze for SpacetimeDB.

Sometimes simple is best! Add the following code to the `Module` class of your `Lib.cs` file and make sure to replace the existing `MoveAllPlayers` reducer.

```csharp
const float MINIMUM_SAFE_MASS_RATIO = 0.85f;

public static bool IsOverlapping(Entity a, Entity b)
{
    var dx = a.position.x - b.position.x;
    var dy = a.position.y - b.position.y;
    var distance_sq = dx * dx + dy * dy;

    var radius_a = MassToRadius(a.mass);
    var radius_b = MassToRadius(b.mass);

    // If the distance between the two circle centers is less than the
    // maximum radius, then the center of the smaller circle is inside
    // the larger circle. This gives some leeway for the circles to overlap
    // before being eaten.
    var max_radius = radius_a > radius_b ? radius_a: radius_b;
    return distance_sq <= max_radius * max_radius;
}

[Reducer]
public static void MoveAllPlayers(ReducerContext ctx, MoveAllPlayersTimer timer)
{
    var world_size = (ctx.Db.config.id.Find(0) ?? throw new Exception("Config not found")).world_size;

    // Handle player input
    foreach (var circle in ctx.Db.circle.Iter())
    {
        var check_entity = ctx.Db.entity.entity_id.Find(circle.entity_id);
        if (check_entity == null)
        {
            // This can happen if the circle has been eaten by another circle.
            continue;
        }
        var circle_entity = check_entity.Value;
        var circle_radius = MassToRadius(circle_entity.mass);
        var direction = circle.direction * circle.speed;
        var new_pos = circle_entity.position + direction * MassToMaxMoveSpeed(circle_entity.mass);
        circle_entity.position.x = Math.Clamp(new_pos.x, circle_radius, world_size - circle_radius);
        circle_entity.position.y = Math.Clamp(new_pos.y, circle_radius, world_size - circle_radius);

        // Check collisions
        foreach (var entity in ctx.Db.entity.Iter())
        {
            if (entity.entity_id == circle_entity.entity_id)
            {
                continue;
            }
            if (IsOverlapping(circle_entity, entity))
            {
                // Check to see if we're overlapping with food
                if (ctx.Db.food.entity_id.Find(entity.entity_id).HasValue) {
                    ctx.Db.entity.entity_id.Delete(entity.entity_id);
                    ctx.Db.food.entity_id.Delete(entity.entity_id);
                    circle_entity.mass += entity.mass;
                }

                // Check to see if we're overlapping with another circle owned by another player
                var other_circle = ctx.Db.circle.entity_id.Find(entity.entity_id);
                if (other_circle.HasValue &&
                    other_circle.Value.player_id != circle.player_id)
                {
                    var mass_ratio = (float)entity.mass / circle_entity.mass;
                    if (mass_ratio < MINIMUM_SAFE_MASS_RATIO)
                    {
                        ctx.Db.entity.entity_id.Delete(entity.entity_id);
                        ctx.Db.circle.entity_id.Delete(entity.entity_id);
                        circle_entity.mass += entity.mass;
                    }
                }
            }
        }
        ctx.Db.entity.entity_id.Update(circle_entity);
    }
}
```

</TabItem>
</Tabs>

For every circle, we look at all other entities. If they are overlapping then for food, we add the mass of the food to the circle and delete the food, otherwise if it's a circle we delete the smaller circle and add the mass to the bigger circle.

That's it. We don't even have to do anything on the client.

```sh
spacetime publish --server local blackholio
```

Just update your module by publishing and you're on your way eating food! Try to see how big you can get!

We didn't even have to update the client, because our client's `OnDelete` callbacks already handled deleting entities from the scene when they're deleted on the server. SpacetimeDB just synchronizes the state with your client automatically.

Notice that the food automatically respawns as you vaccuum them up. This is because our scheduled reducer is automatically replacing the food 2 times per second, to ensure that there is always 600 food on the map.

## Connecting to Maincloud

- Publish to Maincloud `spacetime publish --server maincloud <your database name> --delete-data`
  - `<your database name>` This name should be unique and cannot contain any special characters other than internal hyphens (`-`).
- Update the URL in the Unity project to: `https://maincloud.spacetimedb.com`
- Update the module name in the Unity project to `<your database name>`.
- Clear the PlayerPrefs in Start() within `GameManager.cs`
- Your `GameManager.cs` should look something like this:

```csharp
const string SERVER_URL = "https://maincloud.spacetimedb.com";
const string MODULE_NAME = "<your module name>";

...

private void Start()
{
    // Clear cached connection data to ensure proper connection
    PlayerPrefs.DeleteAll();

    // Continue with initialization
}
```

To delete your Maincloud database, you can run: `spacetime delete --server maincloud <your database name>`

# Conclusion

<Tabs groupId="server-language" defaultValue="rust">
  <TabItem value="rust" label="Rust">
    So far you've learned how to configure a new Unity project to work with
    SpacetimeDB, how to develop, build, and publish a SpacetimeDB server module.
    Within the module, you've learned how to create tables, update tables, and
    write reducers. You've learned about special reducers like
    `client_connected` and `init` and how to created scheduled reducers. You
    learned how we can used scheduled reducers to implement a physics simulation
    right within your module.
  </TabItem>
  <TabItem value="csharp" label="C#">
    So far you've learned how to configure a new Unity project to work with
    SpacetimeDB, how to develop, build, and publish a SpacetimeDB server module.
    Within the module, you've learned how to create tables, update tables, and
    write reducers. You've learned about special reducers like `ClientConnected`
    and `Init` and how to created scheduled reducers. You learned how we can
    used scheduled reducers to implement a physics simulation right within your
    module.
  </TabItem>
</Tabs>

You've also learned how view module logs and connect your client to your database server, call reducers from the client and synchronize the data with client. Finally you learned how to use that synchronized data to draw game objects on the screen, so that we can interact with them and play a game!

And all of that completely from scratch!

Our game is still pretty limited in some important ways. The biggest limitation is that the client assumes your username is "3Blave" and doesn't give you a menu or a window to set your username before joining the game. Notably, we do not have a unique constraint on the `name` column, so that does not prevent us from connecting multiple clients to the same server.

In fact, if you build what we have and run multiple clients you already have a (very simple) MMO! You can connect hundreds of players to this arena with SpacetimeDB.

There's still plenty more we can do to build this into a proper game though. For example, you might want to also add

- Username chooser
- Chat
- Leaderboards
- Nice animations
- Nice shaders
- Space theme!

Fortunately, we've done that for you! If you'd like to check out the completed tutorial game, with these additional features, you can download it on GitHub:

[https://github.com/clockworklabs/SpacetimeDB/tree/master/demo/Blackholio](https://github.com/clockworklabs/SpacetimeDB/tree/master/demo/Blackholio)

If you have any suggestions or comments on the tutorial, either [open an issue](https://github.com/clockworklabs/SpacetimeDB/issues/new), or join our Discord ([https://discord.gg/SpacetimeDB](https://discord.gg/SpacetimeDB)) and chat with us!

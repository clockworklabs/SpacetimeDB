# Unity Tutorial - Blackholio - Part 3 - Moving and Colliding

Need help with the tutorial? [Join our Discord server](https://discord.gg/spacetimedb)!

This progressive tutorial is continued from [part 3](/docs/unity/part-3).

### Moving the player

At this point, we're very close to having a working game. All we have to do is modify our server to allow the player to move around, and to simulate the physics and collisions of the game.

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

Next, add the following table to your `lib.rs` file.

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

Finally, let's schedule a reducer to run every 50 milliseconds to move the player's circles around based on the most recently set player input.

```rust
#[spacetimedb::table(name = move_all_players_timer, scheduled(move_all_players))]
pub struct MoveAllPlayersTimer {
    #[primary_key]
    #[auto_inc]
    scheduled_id: u64,
    scheduled_at: spacetimedb::ScheduleAt,
}

const START_PLAYER_SPEED: u32 = 10;

fn mass_to_max_move_speed(mass: u32) -> f32 {
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

    let circle_directions: HashMap<u32, DbVector2> = ctx
        .db
        .circle()
        .iter()
        .map(|c| (c.entity_id, c.direction * c.speed))
        .collect();

    // Handle player input
    for circle in ctx.db.circle().iter() {
        let mut circle_entity = ctx.db.entity().entity_id().find(&circle.entity_id).unwrap();
        let circle_radius = mass_to_radius(circle_entity.mass);
        let direction = *circle_directions.get(&circle.entity_id).unwrap();
        let new_pos =
            circle_entity.position + direction * mass_to_max_move_speed(circle_entity.mass);
        circle_entity.position.x = new_pos
            .x
            .clamp(circle_radius, world_size as f32 - circle_radius);
        circle_entity.position.y = new_pos
            .y
            .clamp(circle_radius, world_size as f32 - circle_radius);
        ctx.db.entity().entity_id().update(circle_entity);
    }

    Ok(())
}
```

This reducer is very similar to a standard game "tick" or "frame" that you might find in an ordinary game server or similar to something like the `Update` loop in a game engine like Unity. We've scheduled it every 50 milliseconds and we can use it to step forward our simulation by moving all the circles a little bit further in the direction they're moving.

In this reducer, we're just looping through all the circles in the game and updating their position based on their direction, speed, and mass. Just basic physics.

Add the following to your `init` reducer to schedule the `move_all_players` reducer to run every 50 milliseconds.

```rust
    ctx.db
        .move_all_players_timer()
        .try_insert(MoveAllPlayersTimer {
            scheduled_id: 0,
            scheduled_at: ScheduleAt::Interval(Duration::from_millis(50).as_micros() as u64),
        })?;
```

Republish your module with:

```sh
spacetime publish --server local blackholio --delete-data
```

Regenerate your server bindings with:

```sh
spacetime generate --lang csharp --out-dir ../client/Assets/autogen
```

> **BUG WORKAROUND NOTE**: You may have to delete LoggedOutPlayer.cs again.

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

Let's try it out! Press play and roam freely around the arena!

### Step 4: Play the Game!

6. Hit Play in the Unity Editor and you should now see your resource nodes spawning in the world!

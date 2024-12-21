using SpacetimeDB;

public static partial class Module
{
	const uint START_PLAYER_MASS = 15;
	const uint START_PLAYER_SPEED = 10;
	const uint FOOD_MASS_MIN = 2;
	const uint FOOD_MASS_MAX = 4;
	const uint TARGET_FOOD_COUNT = 600;
	const float MINIMUM_SAFE_MASS_RATIO = 0.85f;
	const float MIN_OVERLAP_PCT_TO_CONSUME = 0.1f;

	const uint MIN_MASS_TO_SPLIT = START_PLAYER_MASS * 2;
	const uint MAX_CIRCLES_PER_PLAYER = 16;
	const float SPLIT_RECOMBINE_DELAY_SEC = 5f;
	const float SPLIT_GRAV_PULL_BEFORE_RECOMBINE_SEC = 2f;
	const float ALLOWED_SPLIT_CIRCLE_OVERLAP_PCT = 0.9f;
	const float SELF_COLLISION_SPEED = 0.05f; //1 == instantly separate circles. less means separation takes time



	#region Tables
	[Table(Name = "config", Public = true)]
	public partial struct Config
	{
		[PrimaryKey]
		public uint id;
		public ulong world_size;
	}

	[Table(Name = "entity", Public = true)]
	public partial struct Entity
	{
		[PrimaryKey, AutoInc]
		public uint entity_id;
		public DbVector2 position;
		public uint mass;
	}

	[Table(Name = "circle", Public = true)]
	[Index(Name = "player_id", BTree = ["player_id"])]
	public partial struct Circle
	{
		[PrimaryKey]
		public uint entity_id;
		public uint player_id;
		public DbVector2 direction;
		public float speed;
		public DateTimeOffset last_split_time;
	}

	[Table(Name = "player", Public = true)]
	public partial struct Player
	{
		[PrimaryKey]
		public Identity identity;
		[Unique, AutoInc]
		public uint player_id;
		public string name;
	}

	[Table(Name = "logged_out_player", Public = true)]
	public partial struct LoggedOutPlayer
	{
		[PrimaryKey]
		public Identity identity;
		public Player player;
	}

	[Table(Name = "food", Public = true)]
	public partial struct Food
	{
		[PrimaryKey]
		public uint entity_id;
	}

	[Table(Name = "move_all_players_timer", Scheduled = nameof(MoveAllPlayers))]
	public partial struct MoveAllPlayersTimer
	{
		[PrimaryKey, AutoInc]
		public ulong scheduled_id;
		public ScheduleAt scheduled_at;
	}

	[Table(Name = "spawn_food_timer", Scheduled = nameof(SpawnFood))]
	public partial struct SpawnFoodTimer
	{
		[PrimaryKey, AutoInc]
		public ulong scheduled_id;
		public ScheduleAt scheduled_at;
	}

	[Table(Name = "circle_decay_timer", Scheduled = nameof(CircleDecay))]
	public partial struct CircleDecayTimer
	{
		[PrimaryKey, AutoInc]
		public ulong scheduled_id;
		public ScheduleAt scheduled_at;
	}

	[Table(Name = "circle_recombine_timer", Scheduled = nameof(CircleRecombine))]
	public partial struct CircleRecombineTimer
	{
		[PrimaryKey, AutoInc]
		public ulong scheduled_id;
		public ScheduleAt scheduled_at;
		public uint player_id;
	}
	#endregion



	#region Reducers
	[Reducer(ReducerKind.Init)]
	public static void Init(ReducerContext ctx)
	{
		Log.Info($"Initializing...");
		ctx.Db.config.Insert(new Config { world_size = 1000 });
		ctx.Db.circle_decay_timer.Insert(new CircleDecayTimer
		{
			scheduled_at = new ScheduleAt.Interval(TimeSpan.FromSeconds(5))
		});
		ctx.Db.spawn_food_timer.Insert(new SpawnFoodTimer
		{
			scheduled_at = new ScheduleAt.Interval(TimeSpan.FromMilliseconds(500))
		});
		ctx.Db.move_all_players_timer.Insert(new MoveAllPlayersTimer
		{
			scheduled_at = new ScheduleAt.Interval(TimeSpan.FromMilliseconds(50))
		});
	}

	[Reducer(ReducerKind.ClientConnected)]
	public static void Connect(ReducerContext ctx)
	{
		var player = ctx.Db.logged_out_player.identity.Find(ctx.CallerIdentity);
		if (player != null)
		{
			ctx.Db.player.Insert(player.Value.player);
			ctx.Db.logged_out_player.Delete(player.Value);
		}
		else
		{
			ctx.Db.player.Insert(new Player
			{
				identity = ctx.CallerIdentity,
				name = "",
			});
		}
	}

	[Reducer(ReducerKind.ClientDisconnected)]
	public static void Disconnect(ReducerContext ctx)
	{
		var player = ctx
			.Db
			.player
			.identity
			.Find(ctx.CallerIdentity)
			?? throw new Exception("Player not found");
		foreach (var circle in ctx.Db.circle.player_id.Filter(player.player_id))
		{
			var entity = ctx
				.Db
				.entity
				.entity_id
				.Find(circle.entity_id)
				?? throw new Exception("Could not find circle");
			ctx.Db.entity.entity_id.Delete(entity.entity_id);
			ctx.Db.circle.entity_id.Delete(entity.entity_id);
		}
		ctx.Db.logged_out_player.Insert(new LoggedOutPlayer {
			identity = player.identity,
			player = player
		});
		ctx.Db.player.Delete(player);
	}

	[Reducer]
	public static void EnterGame(ReducerContext ctx, string name)
	{
		Log.Info($"Creating player with name {name}");
		var player = ctx.Db.player.identity.Find(ctx.CallerIdentity) ?? throw new Exception("Player not found");
		var player_id = player.player_id;
		player.name = name;
		ctx.Db.player.identity.Update(player);
		SpawnPlayerInitialCircle(ctx, player_id);
	}

	[Reducer]
	public static void Respawn(ReducerContext ctx)
	{
		var player = ctx
			.Db
			.player
			.identity
			.Find(ctx.CallerIdentity)
			?? throw new Exception("No such player found");
		if (ctx
			.Db
			.circle
			.player_id
			.Filter(player.player_id)
			.Any())


		{
			throw new Exception($"Player {player.player_id} already has a circle");
		}

		SpawnPlayerInitialCircle(ctx, player.player_id);
	}

	public static Entity SpawnPlayerInitialCircle(ReducerContext ctx, uint player_id)
	{
		var rng = ctx.Rng;
		var world_size = (ctx
			.Db
			.config
			.id
			.Find(0)
			?? throw new Exception("Config not found"))
			.world_size;
		var player_start_radius = MassToRadius(START_PLAYER_MASS);
		var x = rng.(player_start_radius, world_size - player_start_radius);
		var y = rng.NextSingle(player_start_radius, world_size - player_start_radius);
		return SpawnCircleAt(
			ctx,
			player_id,
			START_PLAYER_MASS,
			new DbVector2(x, y),
			ctx.Timestamp


		);
	}

	public static Entity SpawnCircleAt(ReducerContext ctx, uint player_id, uint mass, DbVector2 position, DateTimeOffset timestamp)
	{
		var entity = ctx.Db.entity.Insert(new Entity {
			position = position,
			mass = mass,
		});

		ctx.Db.circle.Insert(new Circle {
			entity_id = entity.entity_id,
			player_id = player_id,
			direction = new DbVector2(0, 1),
			speed = 0f,
			last_split_time = timestamp,

		});
		return entity;
	}

	[Reducer]
	public static void UpdatePlayerInput(ReducerContext ctx, DbVector2 direction)
	{
		var player = ctx
			.Db
			.player
			.identity
			.Find(ctx.CallerIdentity)
			?? throw new Exception("Player not found");
		foreach (var c in ctx.Db.circle.player_id.Filter(player.player_id))
		{
			var circle = c;
			circle.direction = direction.Normalized;
			circle.speed = Math.Clamp(direction.Magnitude, 0f, 1f);
			ctx.Db.circle.entity_id.Update(circle);
		}
	}

	public static bool IsOverlapping(Entity a, Entity b)
	{
		var dx = a.position.x - b.position.x;
		var dy = a.position.y - b.position.y;
		var distance_sq = dx * dx + dy * dy;

		var radius_a = MassToRadius(a.mass);
		var radius_b = MassToRadius(b.mass);
		var radius_sum = (radius_a + radius_b) * (1.0 - MIN_OVERLAP_PCT_TO_CONSUME);

		return distance_sq <= radius_sum * radius_sum;
	}

	public static float MassToRadius(uint mass) => MathF.Sqrt(mass);

	public static float MassToMaxMoveSpeed(uint mass) => 2f * START_PLAYER_SPEED / (1f + MathF.Sqrt((float)mass / START_PLAYER_MASS));

	[Reducer]
	public static void MoveAllPlayers(ReducerContext ctx, MoveAllPlayersTimer timer)
	{
		//var span = spacetimedb::log_stopwatch::LogStopwatch::new("tick");
		var world_size = (ctx
			.Db
			.config
			.id
			.Find(0)
			?? throw new Exception("Config not found"))
			.world_size;

		var circle_directions = ctx
			.Db
			.circle
			.Iter()
			.Select(c => (c.entity_id, c.direction * c.speed))
			.ToDictionary();

		//Split circle movement
		foreach (var player in ctx.Db.player.Iter())
		{
			var circles = ctx
				.Db
				.circle
				.player_id
				.Filter(player.player_id)
				.ToList();
			var player_entities = circles
				.Select(c => ctx.Db.entity.entity_id.Find(c.entity_id)!)
				.ToList();
			if (player_entities.Count <= 1)
			{
				continue;
			}
			var count = player_entities.Count;

			//Gravitate circles towards other circles before they recombine
			for (int i = 0; i < player_entities.Count; i++)
			{
				var circle_i = circles[i];
				var time_since_split = (float)((ctx
					.Timestamp
					- circle_i.last_split_time)
					.TotalSeconds);
				var time_before_recombining = MathF.Max(SPLIT_RECOMBINE_DELAY_SEC - time_since_split, 0f);
				if (time_before_recombining > SPLIT_GRAV_PULL_BEFORE_RECOMBINE_SEC) {
					continue;
				}

				var (slice1, slice_i) = player_entities.split_at_mut(i);
				var (slice_i, slice2) = slice_i.split_at_mut(1);
				var entity_i = &slice_i[0];
				for entity_j in slice1.Iter().chain(slice2.Iter()) {
					var diff = entity_i.position - entity_j.position;
					var distance_sqr = diff.sqr_magnitude();
					if (distance_sqr <= 0.0001f) {
						diff = new DbVector2(1f, 0f);
						distance_sqr = 1.0;
					}
					var radius_sum = MassToRadius(entity_i.mass) + MassToRadius(entity_j.mass);
					if distance_sqr > radius_sum * radius_sum {
						var gravity_multiplier =
							1.0 - time_before_recombining / SPLIT_GRAV_PULL_BEFORE_RECOMBINE_SEC;
						var vec = diff.normalized()
							* (radius_sum - distance_sqr.sqrt())
							* gravity_multiplier
							* 0.05
							/ count as f32;
						*circle_directions.get_mut(entity_i.entity_id).Value += vec / 2.0;
						*circle_directions.get_mut(entity_j.entity_id).Value -= vec / 2.0;
					}
				}
			}

			//Force circles apart
			for (int i = 0; i < player_entities.Count; i++)
			{
				var (slice1, slice2) = player_entities.split_at_mut(i + 1);
				var entity_i = &slice1[i];
				for j in 0..slice2.Count {
					var entity_j = &slice2[j];
					var diff = entity_i.position - entity_j.position;
					var distance_sqr = diff.sqr_magnitude();
					if distance_sqr <= 0.0001 {
						diff = DbVector2::new(1.0, 0.0);
						distance_sqr = 1.0;
					}
					var radius_sum = MassToRadius(entity_i.mass) + MassToRadius(entity_j.mass);
					var radius_sum_multiplied = radius_sum * ALLOWED_SPLIT_CIRCLE_OVERLAP_PCT;
					if distance_sqr < radius_sum_multiplied * radius_sum_multiplied {
						var vec = diff.normalized()
							* (radius_sum - distance_sqr.sqrt())
							* SELF_COLLISION_SPEED;
						*circle_directions.get_mut(entity_i.entity_id).Value += vec / 2.0;
						*circle_directions.get_mut(entity_j.entity_id).Value -= vec / 2.0;
					}
				}
			}
		}

		//Handle player input
		foreach (var circle in ctx.Db.circle.Iter()) {
			var circle_entity = ctx.Db.entity.entity_id.Find(circle.entity_id).Value;
			var circle_radius = MassToRadius(circle_entity.mass);
			var direction = *circle_directions.get(circle.entity_id).Value;
			var new_pos =
				circle_entity.position + direction * mass_to_max_move_speed(circle_entity.mass);
			circle_entity.position.x = new_pos
				.x
				.clamp(circle_radius, world_size as f32 - circle_radius);
			circle_entity.position.y = new_pos
				.y
				.clamp(circle_radius, world_size as f32 - circle_radius);
			ctx.Db.entity.entity_id.Update(circle_entity);
		}

		// Check collisions
		var entities = ctx.Db.entity.Iter().Select(e => (e.entity_id, e)).ToDictionary();
		foreach (var circle in ctx.Db.circle.Iter()) {
			// var span = spacetimedb::time_span::Span::start("collisions");
			var circle_entity = entities.get(circle.entity_id).Value;
			foreach (var (_, other_entity) in entities.Iter()) {
				if other_entity.entity_id == circle_entity.entity_id {
					continue;
				}

				if (IsOverlapping(circle_entity, &other_entity)) {
					// Check to see if we're overlapping with food
					if (ctx
						.Db
						.food
						.entity_id
						.Find(other_entity.entity_id)
						.HasValue
						)

					{
						ctx.Db.entity.entity_id.Delete(other_entity.entity_id);
						ctx.Db.food.entity_id.Delete(other_entity.entity_id);
						circle_entity.mass += other_entity.mass;
					}

					// Check to see if we're overlapping with another circle owned by another player
					var other_circle = ctx.Db.circle.entity_id.Find(other_entity.entity_id);
					if var Some(other_circle) = other_circle {
						if (other_circle.player_id != circle.player_id) {
							var mass_ratio = other_entity.mass as f32 / circle_entity.mass as f32;
							if mass_ratio < MINIMUM_SAFE_MASS_RATIO {
								ctx.Db.entity.entity_id.Delete(other_entity.entity_id);
								ctx.Db.circle.entity_id.Delete(other_entity.entity_id);
								circle_entity.mass += other_entity.mass;
							}
						}
					}
				}
			}
			// span.end();

			ctx.Db.entity.entity_id.Update(circle_entity);
		}
	}

	[Reducer]
	public static void PlayerSplit(ReducerContext ctx)
	{
		var player = ctx
			.Db
			.player
			.identity
			.Find(ctx.CallerIdentity)
			?? throw new Exception("Sender has no player");
		var circles = ctx
			.Db
			.circle
			.player_id
			.Filter(player.player_id)
			.ToList();
		var circle_count = circles.Count;
		if (circle_count >= MAX_CIRCLES_PER_PLAYER) {
			return;
		}

		foreach (var circle in circles) {
			var circle_entity = ctx
				.Db
				.entity
				.entity_id
				.Find(circle.entity_id)
				?? throw new Exception("Circle has no entity");
			if (circle_entity.mass >= MIN_MASS_TO_SPLIT * 2) {
				var half_mass = circle_entity.mass / 2;
				SpawnCircleAt(
					ctx,
					circle.player_id,
					half_mass,
					circle_entity.position + circle.direction,
					ctx.Timestamp
				);
				circle_entity.mass -= half_mass;
				circle.last_split_time = ctx.Timestamp;
				ctx.Db.circle.entity_id.Update(circle);
				ctx.Db.entity.entity_id.Update(circle_entity);
				circle_count += 1;
				if (circle_count >= MAX_CIRCLES_PER_PLAYER) {
					break;
				}
			}
		}

		ctx.Db
			.circle_recombine_timer
			.Insert(new CircleRecombineTimer {
				scheduled_at = new ScheduleAt.Time(
				DateTimeOffset.Now.Add(TimeSpan.FromSeconds(SPLIT_RECOMBINE_DELAY_SEC))),
				player_id = player.player_id,
			});

		Log.Warn("Player split!");
	}

	[Reducer]
	public static void SpawnFood(ReducerContext ctx, SpawnFoodTimer timer)
	{
		if (ctx.Db.player.Count == 0) //Are there no players yet?
		{
			return;
		}

		var world_size = (ctx
			.Db
			.config
			.id
			.Find(0)
			?? throw new Exception("Config not found"))
			.world_size;

		var rng = ctx.Rng;
		var food_count = ctx.Db.food.Count;
		while (food_count < TARGET_FOOD_COUNT) {
			var food_mass = rng.gen_range(FOOD_MASS_MIN, FOOD_MASS_MAX);
			var food_radius = MassToRadius(food_mass);
			var x = rng.gen_range(food_radius, world_size - food_radius);
			var y = rng.gen_range(food_radius, world_size - food_radius);
			var entity = ctx.Db.entity.Insert(new Entity() {
				position = new DbVector2(x, y),
				mass = food_mass,
			});
			ctx.Db.food.Insert(new Food {
				entity_id = entity.entity_id,
			});
			food_count += 1;
			Log.Info($"Spawned food! {entity.entity_id}");
		}
	}

	[Reducer]
	public static void CircleDecay(ReducerContext ctx, CircleDecayTimer timer)
	{
		foreach (var circle in ctx.Db.circle.Iter())
		{
			var circle_entity = ctx
				.Db
				.entity
				.entity_id
				.Find(circle.entity_id)
				?? throw new Exception("Entity not found");
			if (circle_entity.mass <= START_PLAYER_MASS)
			{
				continue;
			}
			circle_entity.mass = (uint)(circle_entity.mass * 0.99f);
			ctx.Db.entity.entity_id.Update(circle_entity);
		}
	}

	public static DbVector2 CalculateCenterOfMass(List<Entity> entities)
	{
		var total_mass = entities.Sum(e => e.mass);
		var center_of_mass = entities.Select(e => e.position * e.mass).Aggregate((a, b) => a + b);
		return center_of_mass / total_mass;
	}

	[Reducer]
	public static void CircleRecombine(ReducerContext ctx, CircleRecombineTimer timer)
	{
		var circles = ctx
			.Db
			.circle
			.player_id
			.Filter(timer.player_id)
			.ToList();
		var recombining_entities = circles
			.Where(c => (ctx.Timestamp - c.last_split_time)
				.TotalSeconds >= SPLIT_RECOMBINE_DELAY_SEC)
		.Select(c => ctx.Db.entity.entity_id.Find(c.entity_id) ?? throw new Exception())
		.ToList();
		if (recombining_entities.Count <= 1) {
			return; //No circles to recombine
		}

		var total_mass = recombining_entities.Sum(e => e.mass);
		var center_of_mass = CalculateCenterOfMass(recombining_entities);
		recombining_entities[0].mass = total_mass;
		recombining_entities[0].position = center_of_mass;

		ctx.Db
			.entity
			.entity_id
			.Update(recombining_entities[0]);
		for (int i = 1; i < recombining_entities.Count; i++) {
			var entity_id = recombining_entities[i].entity_id;
			ctx.Db.entity.entity_id.Delete(entity_id);
			ctx.Db.circle.entity_id.Delete(entity_id);
		}
	}
	#endregion
}
using SpacetimeDB;

public static partial class Module
{
	const int START_PLAYER_MASS = 15;
	const int START_PLAYER_SPEED = 10;
	const int FOOD_MASS_MIN = 2;
	const int FOOD_MASS_MAX = 4;
	const int TARGET_FOOD_COUNT = 600;
	const float MINIMUM_SAFE_MASS_RATIO = 0.85f;
	const float MIN_OVERLAP_PCT_TO_CONSUME = 0.1f;

	const int MIN_MASS_TO_SPLIT = START_PLAYER_MASS * 2;
	const int MAX_CIRCLES_PER_PLAYER = 16;
	const float SPLIT_RECOMBINE_DELAY_SEC = 5f;
	const float SPLIT_GRAV_PULL_BEFORE_RECOMBINE_SEC = 2f;
	const float ALLOWED_SPLIT_CIRCLE_OVERLAP_PCT = 0.9f;
	const float SELF_COLLISION_SPEED = 0.05f; //1 == instantly separate circles. less means separation takes time



	#region Tables
	[Table(Name = "config", Public = true)]
	public partial struct Config
	{
		[PrimaryKey]
		public int id;
		public long world_size;
	}

	[Table(Name = "entity", Public = true)]
	[Table(Name = "logged_out_entity")]
	public partial struct Entity
	{
		[PrimaryKey, AutoInc]
		public int entity_id;
		public DbVector2 position;
		public int mass;
	}

	[Table(Name = "circle", Public = true)]
	[SpacetimeDB.Index.BTree(Name = "player_id", Columns = [nameof(player_id)])]
	[Table(Name = "logged_out_circle")]
	public partial struct Circle
	{
		[PrimaryKey]
		public int entity_id;
		public int player_id;
		public DbVector2 direction;
		public float speed;
		public SpacetimeDB.Timestamp last_split_time;
	}

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

	[Table(Name = "food", Public = true)]
	public partial struct Food
	{
		[PrimaryKey]
		public int entity_id;
	}

	[Table(Name = "move_all_players_timer", Scheduled = nameof(MoveAllPlayers), ScheduledAt = nameof(scheduled_at))]
	public partial struct MoveAllPlayersTimer
	{
		[PrimaryKey, AutoInc]
		public ulong scheduled_id;
		public ScheduleAt scheduled_at;
	}

	[Table(Name = "spawn_food_timer", Scheduled = nameof(SpawnFood), ScheduledAt = nameof(scheduled_at))]
	public partial struct SpawnFoodTimer
	{
		[PrimaryKey, AutoInc]
		public ulong scheduled_id;
		public ScheduleAt scheduled_at;
	}

	[Table(Name = "circle_decay_timer", Scheduled = nameof(CircleDecay), ScheduledAt = nameof(scheduled_at))]
	public partial struct CircleDecayTimer
	{
		[PrimaryKey, AutoInc]
		public ulong scheduled_id;
		public ScheduleAt scheduled_at;
	}

	[Table(Name = "circle_recombine_timer", Scheduled = nameof(CircleRecombine), ScheduledAt = nameof(scheduled_at))]
	public partial struct CircleRecombineTimer
	{
		[PrimaryKey, AutoInc]
		public ulong scheduled_id;
		public ScheduleAt scheduled_at;
		public int player_id;
	}

	[Table(Name = "consume_entity_timer", Scheduled = nameof(ConsumeEntity), ScheduledAt = nameof(scheduled_at))]
	public partial struct ConsumeEntityTimer
	{
		[PrimaryKey, AutoInc]
		public ulong scheduled_id;
		public ScheduleAt scheduled_at;
		public int consumed_entity_id;
		public int consumer_entity_id;
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
		var player = ctx.Db.logged_out_player.identity.Find(ctx.Sender);
		if (player != null)
		{
			ctx.Db.player.Insert(player.Value);
			ctx.Db.logged_out_player.identity.Delete(player.Value.identity);

			foreach (var circle in ctx.Db.logged_out_circle.player_id.Filter(player.Value.player_id))
			{
				var entity = ctx.Db.logged_out_entity.entity_id.Find(circle.entity_id) ?? throw new Exception("Could not find Entity");
				ctx.Db.entity.Insert(entity);
				ctx.Db.logged_out_entity.entity_id.Delete(entity.entity_id);
				ctx.Db.circle.Insert(circle);
				ctx.Db.logged_out_circle.entity_id.Delete(entity.entity_id);
			}
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
		foreach (var circle in ctx.Db.circle.player_id.Filter(player.player_id))
		{
			var entity = ctx.Db.entity.entity_id.Find(circle.entity_id) ?? throw new Exception("Could not find circle");
			ctx.Db.logged_out_entity.Insert(entity);
			ctx.Db.entity.entity_id.Delete(entity.entity_id);
			ctx.Db.logged_out_circle.Insert(circle);
			ctx.Db.circle.entity_id.Delete(entity.entity_id);
		}
		ctx.Db.logged_out_player.Insert(player);
		ctx.Db.player.identity.Delete(player.identity);
	}

	[Reducer]
	public static void EnterGame(ReducerContext ctx, string name)
	{
		Log.Info($"Creating player with name {name}");
		var player = ctx.Db.player.identity.Find(ctx.Sender) ?? throw new Exception("Player not found");
		player.name = name;
		ctx.Db.player.identity.Update(player);
		SpawnPlayerInitialCircle(ctx, player.player_id);
	}

	[Reducer]
	public static void Respawn(ReducerContext ctx)
	{
		var player = ctx.Db.player.identity.Find(ctx.Sender) ?? throw new Exception("No such player found");

		SpawnPlayerInitialCircle(ctx, player.player_id);
	}

	[Reducer]
	public static void Suicide(ReducerContext ctx)
	{
		var player = ctx.Db.player.identity.Find(ctx.Sender) ?? throw new Exception("No such player found");

		foreach (var circle in ctx.Db.circle.player_id.Filter(player.player_id))
		{
			DestroyEntity(ctx, circle.entity_id);
		}
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

	public static float MassToRadius(int mass) => MathF.Sqrt(mass);

	public static float MassToMaxMoveSpeed(int mass) => 2f * START_PLAYER_SPEED / (1f + MathF.Sqrt((float)mass / START_PLAYER_MASS));

	[Reducer]
	public static void MoveAllPlayers(ReducerContext ctx, MoveAllPlayersTimer timer)
	{
		//var span = new SpacetimeDB.LogStopwatch("tick");
		var world_size = (ctx.Db.config.id.Find(0) ?? throw new Exception("Config not found")).world_size;

		var circle_directions = ctx.Db.circle.Iter().Select(c => (c.entity_id, c.direction * c.speed)).ToDictionary();

		//Split circle movement
		foreach (var player in ctx.Db.player.Iter())
		{
			List<Circle> circles = ctx.Db.circle.player_id.Filter(player.player_id).ToList();
			List<Entity> player_entities = circles
				.Select(c => ctx.Db.entity.entity_id.Find(c.entity_id) ?? throw new Exception("No entity for circle"))
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
				var time_since_split = ctx.Timestamp.TimeDurationSince(circle_i.last_split_time).Microseconds / 1_000_000f;
				var time_before_recombining = MathF.Max(SPLIT_RECOMBINE_DELAY_SEC - time_since_split, 0f);
				if (time_before_recombining > SPLIT_GRAV_PULL_BEFORE_RECOMBINE_SEC)
				{
					continue;
				}

				var entity_i = player_entities[i];
				for (int j = i + 1; j < player_entities.Count; j++)
				{
					var entity_j = player_entities[j];
					var diff = entity_i.position - entity_j.position;
					var distance_sqr = diff.SqrMagnitude;
					if (distance_sqr <= 0.0001f)
					{
						diff = new DbVector2(1f, 0f);
						distance_sqr = 1f;
					}
					var radius_sum = MassToRadius(entity_i.mass) + MassToRadius(entity_j.mass);
					if (distance_sqr > radius_sum * radius_sum)
					{
						var gravity_multiplier =
							1f - time_before_recombining / SPLIT_GRAV_PULL_BEFORE_RECOMBINE_SEC;
						var vec = diff.Normalized
							* (radius_sum - MathF.Sqrt(distance_sqr))
							* gravity_multiplier
							* 0.05f
							/ count;
						circle_directions[entity_i.entity_id] += vec / 2f;
						circle_directions[entity_j.entity_id] -= vec / 2f;
					}
				}
			}

			//Force circles apart
			for (int i = 0; i < player_entities.Count; i++)
			{
				var entity_i = player_entities[i];
				for (int j = i + 1; j < player_entities.Count; j++)
				{
					var entity_j = player_entities[j];
					var diff = entity_i.position - entity_j.position;
					var distance_sqr = diff.SqrMagnitude;
					if (distance_sqr <= 0.0001f)
					{
						diff = new DbVector2(1f, 0f);
						distance_sqr = 1f;
					}
					var radius_sum = MassToRadius(entity_i.mass) + MassToRadius(entity_j.mass);
					var radius_sum_multiplied = radius_sum * ALLOWED_SPLIT_CIRCLE_OVERLAP_PCT;
					if (distance_sqr < radius_sum_multiplied * radius_sum_multiplied)
					{
						var vec = diff.Normalized
							* (radius_sum - MathF.Sqrt(distance_sqr))
							* SELF_COLLISION_SPEED;
						circle_directions[entity_i.entity_id] += vec / 2f;
						circle_directions[entity_j.entity_id] -= vec / 2f;
					}
				}
			}
		}

		//Handle player input
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

		// Check collisions
		Dictionary<int, Entity> entities = ctx.Db.entity.Iter().Select(e => (e.entity_id, e)).ToDictionary();
		foreach (var circle in ctx.Db.circle.Iter())
		{
			//var span = new SpacetimeDB.LogStopwatch("collisions");
			var circle_entity = entities[circle.entity_id];
			foreach (var (_, other_entity) in entities)
			{
				if (other_entity.entity_id == circle_entity.entity_id)
				{
					continue;
				}

				if (IsOverlapping(circle_entity, other_entity))
				{
					// Check to see if we're overlapping with another circle owned by another player
					var other_circle = ctx.Db.circle.entity_id.Find(other_entity.entity_id);
					if (other_circle.HasValue)
					{
						if (other_circle.Value.player_id != circle.player_id)
						{
							var mass_ratio = (float)other_entity.mass / circle_entity.mass;
							if (mass_ratio < MINIMUM_SAFE_MASS_RATIO)
							{
								schedule_consume_entity(ctx, circle_entity.entity_id, other_entity.entity_id);
							}
						}
					}
					else
					{
						schedule_consume_entity(ctx, circle_entity.entity_id, other_entity.entity_id);
					}
				}
			}
			//span.End();
		}

		//span.End();
	}

	private static void schedule_consume_entity(ReducerContext ctx, int consumer_id, int consumed_id)
	{
		ctx.Db.consume_entity_timer.Insert(new ConsumeEntityTimer
		{
			scheduled_at = new ScheduleAt.Time(DateTimeOffset.Now),
			consumer_entity_id = consumer_id,
			consumed_entity_id = consumed_id,
		});
	}

	[Reducer]
	public static void ConsumeEntity(ReducerContext ctx, ConsumeEntityTimer request)
	{
		var consumed_entity = ctx.Db.entity.entity_id.Find(request.consumed_entity_id) ?? throw new Exception("Consumed entity doesn't exist");
		var consumer_entity = ctx.Db.entity.entity_id.Find(request.consumer_entity_id) ?? throw new Exception("Consumer entity doesn't exist");

		consumer_entity.mass += consumed_entity.mass;
		DestroyEntity(ctx, consumed_entity.entity_id);
		ctx.Db.entity.entity_id.Update(consumer_entity);
	}

	public static void DestroyEntity(ReducerContext ctx, int entityId)
	{
		ctx.Db.food.entity_id.Delete(entityId);
		ctx.Db.circle.entity_id.Delete(entityId);
		ctx.Db.entity.entity_id.Delete(entityId);
	}

	[Reducer]
	public static void PlayerSplit(ReducerContext ctx)
	{
		var player = ctx.Db.player.identity.Find(ctx.Sender) ?? throw new Exception("Sender has no player");
		List<Circle> circles = ctx.Db.circle.player_id.Filter(player.player_id).ToList();
		var circle_count = circles.Count;
		if (circle_count >= MAX_CIRCLES_PER_PLAYER)
		{
			return;
		}

		for (int i = 0; i < circles.Count; i++)
		{
			var circle = circles[i];
			var circle_entity = ctx.Db.entity.entity_id.Find(circle.entity_id) ?? throw new Exception("Circle has no entity");
			if (circle_entity.mass >= MIN_MASS_TO_SPLIT * 2)
			{
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
				if (circle_count >= MAX_CIRCLES_PER_PLAYER)
				{
					break;
				}
			}
		}

		var duration = new TimeDuration { Microseconds = (long)(SPLIT_RECOMBINE_DELAY_SEC * 1_000_000) };
		var trigger_at = ctx.Timestamp + duration;
		ctx.Db.circle_recombine_timer.Insert(new CircleRecombineTimer
		{
			scheduled_at = new ScheduleAt.Time(trigger_at),
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

	[Reducer]
	public static void CircleDecay(ReducerContext ctx, CircleDecayTimer timer)
	{
		foreach (var circle in ctx.Db.circle.Iter())
		{
			var circle_entity = ctx.Db.entity.entity_id.Find(circle.entity_id) ?? throw new Exception("Entity not found");
			if (circle_entity.mass <= START_PLAYER_MASS)
			{
				continue;
			}
			circle_entity.mass = (int)(circle_entity.mass * 0.99f);
			ctx.Db.entity.entity_id.Update(circle_entity);
		}
	}

	public static DbVector2 CalculateCenterOfMass(IEnumerable<Entity> entities)
	{
		var total_mass = entities.Sum(e => e.mass);
		var center_of_mass = entities.Select(e => e.position * e.mass).Aggregate((a, b) => a + b);
		return center_of_mass / total_mass;
	}

	[Reducer]
	public static void CircleRecombine(ReducerContext ctx, CircleRecombineTimer timer)
	{
		List<Circle> circles = ctx.Db.circle.player_id.Filter(timer.player_id).ToList();
		Entity[] recombining_entities = circles
			.Where(c => ctx.Timestamp.TimeDurationSince(c.last_split_time).Microseconds / 1_000_000f >= SPLIT_RECOMBINE_DELAY_SEC)
			.Select(c => ctx.Db.entity.entity_id.Find(c.entity_id) ?? throw new Exception())
			.ToArray();
		if (recombining_entities.Length <= 1)
		{
			return; //No circles to recombine
		}

		var base_entity_id = recombining_entities[0].entity_id;

		for (int i = 1; i < recombining_entities.Length; i++)
		{
			schedule_consume_entity(ctx, base_entity_id, recombining_entities[i].entity_id);
		}
	}
	#endregion



	public static float Range(this Random rng, float min, float max) => rng.NextSingle() * (max - min) + min;

	public static int Range(this Random rng, int min, int max) => (int)rng.NextInt64(min, max);
}
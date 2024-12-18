using SpacetimeDB;

public static partial class Module
{
	const uint TARGET_FOOD_COUNT = 600;
	const float MINIMUM_SAFE_MASS_RATIO = 0.85f;
	const uint START_PLAYER_MASS = 12;
	const uint START_PLAYER_SPEED = 10;
	const uint FOOD_MASS_MIN = 2;
	const uint FOOD_MASS_MAX = 4;



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
		[PrimaryKey]
		[AutoInc]
		public uint id;
		public Vector2 position;
		public uint mass;
	}

	[Table(Name = "circle", Public = true)]
	[Index(Name = "player_id", BTree = ["player_id"])]
	public partial struct Circle
	{
		[PrimaryKey]
		public uint entity_id;
		public uint player_id;
		public Vector2 direction;
		public float magnitude;
		public DateTime last_split_time;
	}

	[Table(Name = "player", Public = true)]
	public partial struct Player
	{
		[PrimaryKey]
		public Identity identity;
		[Unique]
		[AutoInc]
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

	[Table(Name = "logged_out_circle", Public = true)]
	[Index(Name = "player_id", BTree = ["player_id"])]
	public partial struct LoggedOutCircle
	{
		[PrimaryKey]
		[AutoInc]
		public uint logged_out_id;
		public uint player_id;
		public Circle circle;
		public Entity entity;
	}

	[Table(Name = "food", Public = true)]
	public partial struct Food
	{
		[PrimaryKey]
		public uint entity_id;
	}

	[Table(Name = "move_all_players_timer", Scheduled = nameof(MoveAllPlayersTimer))]
	public partial struct MoveAllPlayersTimer
	{
		[PrimaryKey]
		[AutoInc]
		public ulong scheduled_id;
		public ScheduleAt scheduled_at;
	}

	[Table(Name = "spawn_food_timer", Scheduled = nameof(SpawnFoodTimer))]
	public partial struct SpawnFoodTimer
	{
		[PrimaryKey]
		[AutoInc]
		public ulong scheduled_id;
		public ScheduleAt scheduled_at;
	}

	[Table(Name = "circle_decay_timer", Scheduled = nameof(CircleDecayTimer))]
	public partial struct CircleDecayTimer
	{
		[PrimaryKey]
		[AutoInc]
		public ulong scheduled_id;
		public ScheduleAt scheduled_at;
	}
	#endregion



	[SpacetimeDB.Type]
	public partial struct Vector2
	{
		public float x;
		public float y;

		public Vector2 Normalize()
		{
			var mag = MathF.Sqrt(x * x + y * y);
			if (mag != 0)
			{
				return new Vector2()
				{
					x = x / mag,
					y = y / mag,
				};
			}
			else
			{
				return new Vector2() { x = 0, y = 0 };
			}
		}
	}













	[SpacetimeDB.Table]
	public partial struct Person
	{
		[AutoInc]
		[PrimaryKey]
		public int Id;
		public string Name;
		public int Age;
	}

	[SpacetimeDB.Reducer]
	public static void Add(ReducerContext ctx, string name, int age)
	{
		var person = ctx.Db.Person.Insert(new Person { Name = name, Age = age });
		Log.Info($"Inserted {person.Name} under #{person.Id}");
	}

	[SpacetimeDB.Reducer]
	public static void SayHello(ReducerContext ctx)
	{
		foreach (var person in ctx.Db.Person.Iter())
		{
			Log.Info($"Hello, {person.Name}!");
		}
		Log.Info("Hello, World!");
	}
}
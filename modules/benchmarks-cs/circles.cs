using SpacetimeDB;

namespace Benchmarks;

public static partial class circles
{
    [SpacetimeDB.Type]
    public partial struct Vector2(float x, float y)
    {
        public float x = x;
        public float y = y;
    }

    [SpacetimeDB.Table(Name = "entity")]
    public partial struct Entity(uint id, float x, float y, uint mass)
    {
        [AutoInc]
        [PrimaryKey]
        public uint id = id;
        public Vector2 position = new(x, y);
        public uint mass = mass;
    }

    [SpacetimeDB.Table(Name = "circle")]
    public partial struct Circle(uint entity_id, uint player_id, float x, float y, float magnitude)
    {
        [PrimaryKey]
        public uint entity_id = entity_id;

        [SpacetimeDB.Index.BTree]
        public uint player_id = player_id;
        public Vector2 direction = new(x, y);
        public float magnitude = magnitude;
        public Timestamp last_split_time = (Timestamp)DateTimeOffset.UtcNow;
    }

    [SpacetimeDB.Table(Name = "food")]
    public partial struct Food(uint entity_id)
    {
        [PrimaryKey]
        public uint entity_id = entity_id;
    }

    public static float MassToRadius(uint mass)
    {
        return (float)Math.Sqrt(mass);
    }

    public static bool IsOverlapping(Entity entity1, Entity entity2)
    {
        float entity1_radius = MassToRadius(entity1.mass);
        float entity2_radius = MassToRadius(entity2.mass);
        float distance = (float)
            Math.Sqrt(
                Math.Pow(entity1.position.x - entity2.position.x, 2)
                    + Math.Pow(entity1.position.y - entity2.position.y, 2)
            );
        return distance < Math.Max(entity1_radius, entity2_radius);
    }

    [SpacetimeDB.Reducer]
    public static void insert_bulk_entity(ReducerContext ctx, uint count)
    {
        for (uint id = 0; id < count; id++)
        {
            ctx.Db.entity.Insert(new(0, id, id + 5, id * 5));
        }
        Log.Info($"INSERT ENTITY: {count}");
    }

    [SpacetimeDB.Reducer]
    public static void insert_bulk_circle(ReducerContext ctx, uint count)
    {
        for (uint id = 0; id < count; id++)
        {
            ctx.Db.circle.Insert(new(id, id, id, id + 5, id * 5));
        }
        Log.Info($"INSERT CIRCLE: {count}");
    }

    [SpacetimeDB.Reducer]
    public static void insert_bulk_food(ReducerContext ctx, uint count)
    {
        for (uint id = 1; id <= count; id++)
        {
            ctx.Db.food.Insert(new(id));
        }
        Log.Info($"INSERT FOOD: {count}");
    }

    [SpacetimeDB.Reducer]
    public static void cross_join_all(ReducerContext ctx, uint expected)
    {
        uint count = 0;
        foreach (Circle circle in ctx.Db.circle.Iter())
        {
            foreach (Entity entity in ctx.Db.entity.Iter())
            {
                foreach (Food food in ctx.Db.food.Iter())
                {
                    count++;
                }
            }
        }

        Log.Info($"CROSS JOIN ALL: {expected}, processed: {count}");
    }

    [SpacetimeDB.Reducer]
    public static void cross_join_circle_food(ReducerContext ctx, uint expected)
    {
        uint count = 0;
        foreach (Circle circle in ctx.Db.circle.Iter())
        {
            if (ctx.Db.entity.id.Find(circle.entity_id) is not { } circle_entity)
            {
                continue;
            }

            foreach (Food food in ctx.Db.food.Iter())
            {
                count++;
                Entity food_entity =
                    ctx.Db.entity.id.Find(food.entity_id)
                    ?? throw new Exception($"Entity not found: {food.entity_id}");
                Bench.BlackBox(IsOverlapping(circle_entity, food_entity));
            }
        }

        Log.Info($"CROSS JOIN CIRCLE FOOD: {expected}, processed: {count}");
    }

    [SpacetimeDB.Reducer]
    public static void init_game_circles(ReducerContext ctx, uint initial_load)
    {
        Load load = new(initial_load);

        insert_bulk_food(ctx, load.initial_load);
        insert_bulk_entity(ctx, load.initial_load);
        insert_bulk_circle(ctx, load.small_table);
    }

    [SpacetimeDB.Reducer]
    public static void run_game_circles(ReducerContext ctx, uint initial_load)
    {
        Load load = new(initial_load);

        cross_join_circle_food(ctx, initial_load * load.small_table);
        cross_join_all(ctx, initial_load * initial_load * load.small_table);
    }
}

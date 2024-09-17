
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

    [SpacetimeDB.Table]
    public partial struct Entity(uint id, float x, float y, uint mass)
    {
        [SpacetimeDB.Column(ColumnAttrs.PrimaryKeyAuto)]
        public uint id = id;
        public Vector2 position = new(x, y);
        public uint mass = mass;
    }

    [SpacetimeDB.Table]
    public partial struct Circle(uint entity_id, uint player_id, float x, float y, float magnitude)
    {
        [SpacetimeDB.Column(ColumnAttrs.PrimaryKey)]
        public uint entity_id = entity_id;

        [SpacetimeDB.Column(ColumnAttrs.Indexed)]
        public uint player_id = player_id;

        public Vector2 direction = new(x, y);
        public float magnitude = magnitude;
        public ulong last_split_time = (ulong)(DateTimeOffset.UtcNow.Ticks / 10);
    }

    [SpacetimeDB.Table]
    public partial struct Food(uint entity_id)
    {
        [SpacetimeDB.Column(ColumnAttrs.PrimaryKey)]
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
    public static void insert_bulk_entity(uint count)
    {
        for (uint id = 0; id < count; id++)
        {
            new Entity(0, id, id + 5, id * 5).Insert();
        }
        Log.Info($"INSERT ENTITY: {count}");
    }

    [SpacetimeDB.Reducer]
    public static void insert_bulk_circle(uint count)
    {
        for (uint id = 0; id < count; id++)
        {
            new Circle(id, id, id, id + 5, id * 5).Insert();
        }
        Log.Info($"INSERT CIRCLE: {count}");
    }

    [SpacetimeDB.Reducer]
    public static void insert_bulk_food(uint count)
    {
        for (uint id = 1; id <= count; id++)
        {
            new Food(id).Insert();
        }
        Log.Info($"INSERT FOOD: {count}");
    }

    [SpacetimeDB.Reducer]
    public static void cross_join_all(uint expected)
    {
        uint count = 0;
        foreach (Circle circle in Circle.Iter())
        {
            foreach (Entity entity in Entity.Iter())
            {
                foreach (Food food in Food.Iter())
                {
                    count++;
                }
            }
        }

        Log.Info($"CROSS JOIN ALL: {expected}, processed: {count}");
    }

    [SpacetimeDB.Reducer]
    public static void cross_join_circle_food(uint expected)
    {
        uint count = 0;
        foreach (Circle circle in Circle.Iter())
        {
            if (Entity.FindByid(circle.entity_id) is not { } circle_entity)
            {
                continue;
            }

            foreach (Food food in Food.Iter())
            {
                count++;
                Entity food_entity =
                    Entity.FindByid(food.entity_id)
                    ?? throw new Exception($"Entity not found: {food.entity_id}");
                Bench.BlackBox(IsOverlapping(circle_entity, food_entity));
            }
        }

        Log.Info($"CROSS JOIN CIRCLE FOOD: {expected}, processed: {count}");
    }

    [SpacetimeDB.Reducer]
    public static void init_game_circles(uint initial_load)
    {
        Load load = new(initial_load);

        insert_bulk_food(load.initial_load);
        insert_bulk_entity(load.initial_load);
        insert_bulk_circle(load.small_table);
    }

    [SpacetimeDB.Reducer]
    public static void run_game_circles(uint initial_load)
    {
        Load load = new(initial_load);

        cross_join_circle_food(initial_load * load.small_table);
        cross_join_all(initial_load * initial_load * load.small_table);
    }
}

using SpacetimeDB;

public static partial class Module
{
    [Table(Name = "entities")]
    public partial struct Entity { [PrimaryKey] public int Id; }

    [Table(Name = "positions")]
    public partial struct Position
    {
        [PrimaryKey] public int EntityId;
        public int X;
        public int Y;
    }

    [Table(Name = "velocities")]
    public partial struct Velocity
    {
        [PrimaryKey] public int EntityId;
        public int VX;
        public int VY;
    }

    [Table(Name = "next_positions")]
    public partial struct NextPosition
    {
        [PrimaryKey] public int EntityId;
        public int X;
        public int Y;
    }

    [Reducer]
    public static void Seed(ReducerContext ctx)
    {
        ctx.Db.entities.Insert(new Entity { Id = 1 });
        ctx.Db.entities.Insert(new Entity { Id = 2 });

        ctx.Db.positions.Insert(new Position { EntityId = 1, X = 0,  Y = 0 });
        ctx.Db.positions.Insert(new Position { EntityId = 2, X = 10, Y = 0 });

        ctx.Db.velocities.Insert(new Velocity { EntityId = 1, VX = 1,  VY = 0 });
        ctx.Db.velocities.Insert(new Velocity { EntityId = 2, VX = -2, VY = 3 });
    }

    [Reducer]
    public static void Step(ReducerContext ctx)
    {
        foreach (var p in ctx.Db.positions.Iter())
        {
            var velOpt = ctx.Db.velocities.EntityId.Find(p.EntityId);
            if (!velOpt.HasValue) continue;

            var np = new NextPosition {
                EntityId = p.EntityId,
                X = p.X + velOpt.Value.VX,
                Y = p.Y + velOpt.Value.VY
            };

            if (ctx.Db.next_positions.EntityId.Find(p.EntityId).HasValue)
                ctx.Db.next_positions.EntityId.Update(np);
            else
                ctx.Db.next_positions.Insert(np);
        }
    }
}

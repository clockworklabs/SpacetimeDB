using SpacetimeDB;

public static partial class Module
{
    [Type]
    public partial struct Circle { public int Radius; }

    [Type]
    public partial struct Rectangle { public int Width; public int Height; }

    [Type]
    public partial record Shape : TaggedEnum<(Circle Circle, Rectangle Rectangle)> {}

    [Table(Name = "Result")]
    public partial struct Result
    {
        [PrimaryKey] public int Id;
        public Shape Value;
    }

    [Reducer]
    public static void SetCircle(ReducerContext ctx, int id, int radius)
    {
        ctx.Db.Result.Insert(new Result { Id = id, Value = new Shape.Circle(new Circle { Radius = radius }) });
    }
}

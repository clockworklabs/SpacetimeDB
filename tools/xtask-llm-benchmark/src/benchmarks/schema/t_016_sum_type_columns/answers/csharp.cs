using SpacetimeDB;

public static partial class Module
{
    [Type]
    public partial struct Circle { public int Radius; }

    [Type]
    public partial struct Rectangle { public int Width; public int Height; }

    [Type]
    public partial record Shape : TaggedEnum<(Circle Circle, Rectangle Rectangle)> {}

    [Table(Name = "Drawing")]
    public partial struct Drawing
    {
        [PrimaryKey] public int Id;
        public Shape A;
        public Shape B;
    }

    [Reducer]
    public static void Seed(ReducerContext ctx)
    {
        ctx.Db.Drawing.Insert(new Drawing {
            Id = 1,
            A = new Shape.Circle(new Circle { Radius = 10 }),
            B = new Shape.Rectangle(new Rectangle { Width = 4, Height = 6 })
        });
    }
}

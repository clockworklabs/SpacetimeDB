using SpacetimeDB;

public static partial class Module
{
    [Table(Name = "primitives")]
    public partial struct Primitive
    {
        [PrimaryKey] public int Id;
        public int Count;
        public long Total;
        public float Price;
        public double Ratio;
        public bool Active;
        public string Name;
    }

    [Reducer]
    public static void Seed(ReducerContext ctx)
    {
        ctx.Db.primitives.Insert(new Primitive {
            Id = 1,
            Count = 2,
            Total = 3000000000,
            Price = 1.5f,
            Ratio = 2.25,
            Active = true,
            Name = "Alice"
        });
    }
}

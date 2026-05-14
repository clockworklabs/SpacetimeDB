using SpacetimeDB;

public static partial class Module
{
    [Table(Accessor = "Primitive")]
    public partial struct Primitive
    {
        [PrimaryKey, AutoInc] public ulong Id;
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
        ctx.Db.Primitive.Insert(new Primitive {
            Id = 0,
            Count = 2,
            Total = 3000000000,
            Price = 1.5f,
            Ratio = 2.25,
            Active = true,
            Name = "Alice"
        });
    }
}

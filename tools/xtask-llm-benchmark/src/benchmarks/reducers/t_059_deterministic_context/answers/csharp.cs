using SpacetimeDB;

public static partial class Module
{
    [Table(Accessor = "GeneratedValue", Public = true)]
    public partial struct GeneratedValue
    {
        [PrimaryKey, AutoInc] public ulong Id;
        public Timestamp CreatedAt;
        public long RandomValue;
    }

    [Reducer]
    public static void Generate(ReducerContext ctx) => ctx.Db.GeneratedValue.Insert(new GeneratedValue
    {
        Id = 0,
        CreatedAt = ctx.Timestamp,
        RandomValue = ctx.Rng.NextInt64(1, long.MaxValue),
    });
}

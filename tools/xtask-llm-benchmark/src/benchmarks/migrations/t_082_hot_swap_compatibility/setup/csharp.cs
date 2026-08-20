using SpacetimeDB;

public static partial class Module
{
    [Table(Accessor = "Counter", Public = true)]
    public partial struct Counter
    {
        [PrimaryKey] public ulong Id;
        public long Value;
    }

    [Reducer]
    public static void Seed(ReducerContext ctx) =>
        ctx.Db.Counter.Insert(new Counter { Id = 1, Value = 1 });

    [Reducer]
    public static void Increment(ReducerContext ctx, ulong id, long amount)
    {
        var row = ctx.Db.Counter.Id.Find(id) ?? throw new Exception("counter");
        row.Value += amount;
        ctx.Db.Counter.Id.Update(row);
    }
}

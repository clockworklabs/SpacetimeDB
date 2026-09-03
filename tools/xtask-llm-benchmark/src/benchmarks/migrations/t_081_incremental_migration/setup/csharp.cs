using SpacetimeDB;

public static partial class Module
{
    [Table(Accessor = "LegacyItem", Public = true)]
    public partial struct LegacyItem
    {
        [PrimaryKey] public ulong Id;
        public string Value;
    }

    [Reducer]
    public static void Seed(ReducerContext ctx) =>
        ctx.Db.LegacyItem.Insert(new LegacyItem { Id = 1, Value = "old" });
}

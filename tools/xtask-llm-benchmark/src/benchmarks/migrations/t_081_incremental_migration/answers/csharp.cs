using SpacetimeDB;

public static partial class Module
{
    [Table(Accessor = "LegacyItem", Public = true)]
    public partial struct LegacyItem
    {
        [PrimaryKey] public ulong Id;
        public string Value;
    }

    [Table(Accessor = "ItemV2", Public = true)]
    public partial struct ItemV2
    {
        [PrimaryKey] public ulong Id;
        public string Value;
        public uint Version;
    }

    [Reducer]
    public static void Seed(ReducerContext ctx) =>
        ctx.Db.LegacyItem.Insert(new LegacyItem { Id = 1, Value = "old" });

    [Reducer]
    public static void Migrate(ReducerContext ctx)
    {
        foreach (var row in ctx.Db.LegacyItem.Iter())
        {
            if (ctx.Db.ItemV2.Id.Find(row.Id) is null)
                ctx.Db.ItemV2.Insert(new ItemV2 { Id = row.Id, Value = row.Value, Version = 2 });
        }
    }

    [Reducer]
    public static void DualWrite(ReducerContext ctx, ulong id, string value)
    {
        ctx.Db.LegacyItem.Insert(new LegacyItem { Id = id, Value = value });
        ctx.Db.ItemV2.Insert(new ItemV2 { Id = id, Value = value, Version = 2 });
    }
}

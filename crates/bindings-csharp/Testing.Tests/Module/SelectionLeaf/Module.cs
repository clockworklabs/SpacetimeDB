namespace SelectionLeaf;

[SpacetimeDB.Table(Accessor = "Items")]
public partial struct Item
{
    [SpacetimeDB.PrimaryKey]
    public uint Id;

    [SpacetimeDB.Index.BTree]
    public uint Group;

    public string Value;
}

public static partial class Marker
{
    [SpacetimeDB.Reducer]
    public static void Noop(SpacetimeDB.ReducerContext ctx) { }

    public static Item Insert(SpacetimeDB.ReducerContext ctx, Item item) =>
        ctx.Db.Items.Insert(item);

    public static Item? Find(SpacetimeDB.ViewContext ctx, uint id) => ctx.Db.Items.Id.Find(id);
}

using SpacetimeDB;

[SpacetimeDB.Type]
public partial struct ItemCount
{
    public ulong count;
}

public static partial class Module
{
    [Table(Accessor = "item", Public = true)]
    public partial struct Item
    {
        [PrimaryKey]
        public uint id;
        public uint value;
    }

    [View(Accessor = "sender_table_count", Public = true)]
    public static ItemCount? sender_table_count(ViewContext ctx)
    {
        return new ItemCount { count = ctx.Db.item.Count };
    }

    [View(Accessor = "anon_table_count", Public = true)]
    public static ItemCount? anon_table_count(AnonymousViewContext ctx)
    {
        return new ItemCount { count = ctx.Db.item.Count };
    }

    [Reducer]
    public static void insert_item(ReducerContext ctx, uint id, uint value)
    {
        ctx.Db.item.Insert(new Item { id = id, value = value });
    }

    [Reducer]
    public static void replace_item(ReducerContext ctx, uint id, uint value)
    {
        ctx.Db.item.id.Delete(id);
        ctx.Db.item.Insert(new Item { id = id, value = value });
    }

    [Reducer]
    public static void delete_item(ReducerContext ctx, uint id)
    {
        ctx.Db.item.id.Delete(id);
    }
}

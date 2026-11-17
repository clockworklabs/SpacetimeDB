using System;
using SpacetimeDB;

public static partial class Module
{
    [Table(Accessor = "Item", Public = true)]
    public partial struct Item
    {
        [PrimaryKey]
        public ulong Id;
        public string Name;
    }

    [Reducer]
    public static void AddItem(ReducerContext ctx, ulong id, string name)
    {
        if (name == "")
        {
            throw new Exception("name required");
        }
        ctx.Db.Item.Insert(new Item { Id = id, Name = name });
    }
}

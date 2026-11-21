using System;
using SpacetimeDB;
namespace Inventory;
[Table(Accessor = "Item", Public = true)]
public partial struct Item { [PrimaryKey] public ulong Id; public uint Quantity; }
public static partial class Functions
{
    [Reducer]
    public static void Set(ReducerContext ctx, ulong id, uint quantity)
    {
        var row = new Item { Id = id, Quantity = quantity };
        if (ctx.Db.Item.Id.Find(id) is null) ctx.Db.Item.Insert(row);
        else ctx.Db.Item.Id.Update(row);
    }
    public static void Take(ReducerContext ctx, ulong id, uint quantity)
    {
        var row = ctx.Db.Item.Id.Find(id) ?? throw new Exception("missing item");
        if (row.Quantity < quantity) throw new Exception("insufficient stock");
        row.Quantity -= quantity;
        ctx.Db.Item.Id.Update(row);
    }
    public static void Add(ReducerContext ctx, ulong id, uint quantity)
    {
        var row = ctx.Db.Item.Id.Find(id) ?? new Item { Id = id };
        Set(ctx, id, checked(row.Quantity + quantity));
    }
}

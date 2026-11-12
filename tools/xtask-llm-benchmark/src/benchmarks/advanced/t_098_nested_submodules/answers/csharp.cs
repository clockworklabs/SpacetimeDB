using System;
using SpacetimeDB;
[assembly: Namespace(typeof(Branch.Marker), Accessor = "East")]
[assembly: Namespace(typeof(Branch.Marker), Accessor = "West")]
[assembly: Namespace(typeof(Inventory.Functions), Accessor = "Reserve")]
public static partial class Module
{
    private static ReducerContext Select(ReducerContext ctx, string location) => location switch {
        "east" => ctx.As.East.Stock,
        "west" => ctx.As.West.Stock,
        "reserve" => ctx.As.Reserve,
        _ => throw new Exception("invalid location"),
    };
    [Reducer]
    public static void SetStock(ReducerContext ctx, string location, ulong id, uint quantity) =>
        Inventory.Functions.Set(Select(ctx, location), id, quantity);
    [Reducer]
    public static void Transfer(ReducerContext ctx, string source, string destination, ulong id, uint quantity)
    {
        var from = Select(ctx, source);
        var to = Select(ctx, destination);
        if (source == destination) throw new Exception("same location");
        if (quantity == 0) throw new Exception("invalid quantity");
        Inventory.Functions.Take(from, id, quantity);
        Inventory.Functions.Add(to, id, quantity);
    }
    [View(Accessor = "EastStock", Public = true)]
    public static IQuery<Inventory.Item> EastStock(AnonymousViewContext ctx) => ctx.From.East.Stock.Item();
    [View(Accessor = "WestStock", Public = true)]
    public static IQuery<Inventory.Item> WestStock(AnonymousViewContext ctx) => ctx.From.West.Stock.Item();
}

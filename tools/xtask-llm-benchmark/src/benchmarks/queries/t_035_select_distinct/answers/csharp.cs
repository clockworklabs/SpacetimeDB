using SpacetimeDB;
using System.Collections.Generic;

public static partial class Module
{
    [Table(Accessor = "Order")]
    public partial struct Order
    {
        [PrimaryKey]
        [AutoInc]
        public ulong Id;
        public string Category;
        public uint Amount;
    }

    [Table(Accessor = "DistinctCategory")]
    public partial struct DistinctCategory
    {
        [PrimaryKey]
        public string Category;
    }

    [Reducer]
    public static void CollectDistinctCategories(ReducerContext ctx)
    {
        var categories = new HashSet<string>();
        foreach (var o in ctx.Db.Order.Iter())
        {
            categories.Add(o.Category);
        }
        foreach (var category in categories)
        {
            ctx.Db.DistinctCategory.Insert(new DistinctCategory { Category = category });
        }
    }
}

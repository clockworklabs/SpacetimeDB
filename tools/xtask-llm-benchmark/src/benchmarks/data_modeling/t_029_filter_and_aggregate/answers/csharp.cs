using SpacetimeDB;

public static partial class Module
{
    [Table(Accessor = "Order")]
    public partial struct Order
    {
        [PrimaryKey]
        [AutoInc]
        public ulong Id;
        [SpacetimeDB.Index.BTree]
        public string Category;
        public ulong Amount;
        public bool Fulfilled;
    }

    [Table(Accessor = "CategoryStats")]
    public partial struct CategoryStats
    {
        [PrimaryKey]
        public string Category;
        public ulong TotalAmount;
        public uint OrderCount;
    }

    [Reducer]
    public static void ComputeStats(ReducerContext ctx, string category)
    {
        ulong totalAmount = 0;
        uint orderCount = 0;

        foreach (var o in ctx.Db.Order.Category.Filter(category))
        {
            totalAmount += o.Amount;
            orderCount += 1;
        }

        // Upsert: delete existing then insert
        ctx.Db.CategoryStats.Category.Delete(category);
        ctx.Db.CategoryStats.Insert(new CategoryStats
        {
            Category = category,
            TotalAmount = totalAmount,
            OrderCount = orderCount,
        });
    }
}

using SpacetimeDB;

public static partial class Module
{
    [Table(Accessor = "Product")]
    public partial struct Product
    {
        [PrimaryKey]
        [AutoInc]
        public ulong Id;
        public string Name;
        [SpacetimeDB.Index.BTree]
        public uint Price;
    }

    [Table(Accessor = "PriceRangeResult")]
    public partial struct PriceRangeResult
    {
        [PrimaryKey]
        public ulong ProductId;
        public string Name;
        public uint Price;
    }

    [Reducer]
    public static void FindInPriceRange(ReducerContext ctx, uint minPrice, uint maxPrice)
    {
        foreach (var p in ctx.Db.Product.Iter())
        {
            if (p.Price >= minPrice && p.Price <= maxPrice)
            {
                ctx.Db.PriceRangeResult.Insert(new PriceRangeResult
                {
                    ProductId = p.Id,
                    Name = p.Name,
                    Price = p.Price,
                });
            }
        }
    }
}

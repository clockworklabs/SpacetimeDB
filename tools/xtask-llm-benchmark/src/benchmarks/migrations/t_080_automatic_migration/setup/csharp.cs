using SpacetimeDB;

public static partial class Module
{
    [Table(Accessor = "Product", Public = true)]
    public partial struct Product
    {
        [PrimaryKey] public ulong Id;
        public string Name;
    }

    [Reducer]
    public static void Seed(ReducerContext ctx) =>
        ctx.Db.Product.Insert(new Product { Id = 1, Name = "legacy" });

    [Reducer]
    public static void Touch(ReducerContext ctx) { }
}

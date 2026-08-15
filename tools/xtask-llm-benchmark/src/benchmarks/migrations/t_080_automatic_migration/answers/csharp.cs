using SpacetimeDB;

public static partial class Module
{
    [Table(Accessor = "Product", Public = true)]
    public partial struct Product
    {
        [PrimaryKey] public ulong Id;
        public string Name;
    }

    [Table(Accessor = "Category", Public = true)]
    public partial struct Category
    {
        [PrimaryKey] public ulong Id;
        public string Label;
    }

    [Reducer]
    public static void Seed(ReducerContext ctx) =>
        ctx.Db.Product.Insert(new Product { Id = 1, Name = "legacy" });

    [Reducer]
    public static void Touch(ReducerContext ctx) { }

    [Reducer]
    public static void CreateCategory(ReducerContext ctx, ulong id, string label) =>
        ctx.Db.Category.Insert(new Category { Id = id, Label = label });
}

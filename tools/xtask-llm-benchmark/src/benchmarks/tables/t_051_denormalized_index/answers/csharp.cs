using SpacetimeDB;

public static partial class Module
{
    [Table(Accessor = "Category", Public = true)]
    public partial struct Category
    {
        [PrimaryKey] public ulong Id;
        public string Slug;
    }

    [Table(Accessor = "Product", Public = true)]
    [SpacetimeDB.Index.BTree(Accessor = "by_category", Columns = new[] { nameof(CategoryId) })]
    [SpacetimeDB.Index.BTree(Accessor = "by_category_slug", Columns = new[] { nameof(CategorySlug) })]
    public partial struct Product
    {
        [PrimaryKey] public ulong Id;
        public ulong CategoryId;
        public string CategorySlug;
        public string Name;
    }

    [Reducer]
    public static void CreateCategory(ReducerContext ctx, ulong id, string slug)
    {
        ctx.Db.Category.Insert(new Category { Id = id, Slug = slug });
    }

    [Reducer]
    public static void CreateProduct(ReducerContext ctx, ulong id, ulong categoryId, string name)
    {
        var category = ctx.Db.Category.Id.Find(categoryId) ?? throw new Exception("category not found");
        ctx.Db.Product.Insert(new Product
        {
            Id = id,
            CategoryId = categoryId,
            CategorySlug = category.Slug,
            Name = name,
        });
    }

    [Reducer]
    public static void RenameCategory(ReducerContext ctx, ulong id, string newSlug)
    {
        var category = ctx.Db.Category.Id.Find(id) ?? throw new Exception("category not found");
        ctx.Db.Category.Id.Update(category with { Slug = newSlug });

        foreach (var product in ctx.Db.Product.by_category.Filter(id))
        {
            ctx.Db.Product.Id.Update(product with { CategorySlug = newSlug });
        }
    }
}

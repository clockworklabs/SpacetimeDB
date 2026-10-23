using SpacetimeDB;
public static partial class Module
{
    [Table(Accessor = "Content", Public = true)] public partial struct Content { [PrimaryKey] public ulong Id; [SpacetimeDB.Index.BTree] public string Category; public bool Active; public int Score; }
    [Reducer]
    public static void Seed(ReducerContext ctx)
    {
        ctx.Db.Content.Insert(new Content { Id = 1, Category = "news", Active = true, Score = 20 });
        ctx.Db.Content.Insert(new Content { Id = 2, Category = "news", Active = false, Score = 20 });
        ctx.Db.Content.Insert(new Content { Id = 3, Category = "news", Active = true, Score = 5 });
        ctx.Db.Content.Insert(new Content { Id = 4, Category = "sports", Active = true, Score = 20 });
    }
    [SpacetimeDB.View(Accessor = "FeaturedContent", Public = true)]
    public static IEnumerable<Content> FeaturedContent(AnonymousViewContext ctx) => ctx.Db.Content.Category.Filter("news").Where(row => row.Active && row.Score >= 10);
}

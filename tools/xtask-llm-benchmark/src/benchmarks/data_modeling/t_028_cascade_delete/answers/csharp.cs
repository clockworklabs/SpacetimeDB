using SpacetimeDB;

public static partial class Module
{
    [Table(Accessor = "Author")]
    public partial struct Author
    {
        [PrimaryKey]
        [AutoInc]
        public ulong Id;
        public string Name;
    }

    [Table(Accessor = "Post")]
    public partial struct Post
    {
        [PrimaryKey]
        [AutoInc]
        public ulong Id;
        [SpacetimeDB.Index.BTree]
        public ulong AuthorId;
        public string Title;
    }

    [Reducer]
    public static void DeleteAuthor(ReducerContext ctx, ulong authorId)
    {
        // Delete all posts by this author
        foreach (var p in ctx.Db.Post.AuthorId.Filter(authorId))
        {
            ctx.Db.Post.Id.Delete(p.Id);
        }
        // Delete the author
        ctx.Db.Author.Id.Delete(authorId);
    }
}

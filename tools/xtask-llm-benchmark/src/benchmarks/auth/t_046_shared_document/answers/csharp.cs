using SpacetimeDB;
using System.Linq;

public static partial class Module
{
    [Table(Accessor = "Document", Public = true)]
    public partial struct Document
    {
        [PrimaryKey]
        [AutoInc]
        public ulong Id;
        [SpacetimeDB.Index.BTree]
        public Identity Owner;
        public string Title;
    }

    [Table(Accessor = "DocumentShare", Public = true)]
    public partial struct DocumentShare
    {
        [SpacetimeDB.Index.BTree]
        public ulong DocumentId;
        [SpacetimeDB.Index.BTree]
        public Identity SharedWith;
    }

    [Reducer]
    public static void CreateDocument(ReducerContext ctx, string title)
    {
        ctx.Db.Document.Insert(new Document { Id = 0, Owner = ctx.Sender, Title = title });
    }

    [Reducer]
    public static void ShareDocument(ReducerContext ctx, ulong documentId, Identity target)
    {
        var doc = ctx.Db.Document.Id.Find(documentId) ?? throw new Exception("not found");
        if (doc.Owner != ctx.Sender)
        {
            throw new Exception("not owner");
        }
        ctx.Db.DocumentShare.Insert(new DocumentShare { DocumentId = documentId, SharedWith = target });
    }

    [Reducer]
    public static void EditDocument(ReducerContext ctx, ulong documentId, string newTitle)
    {
        var doc = ctx.Db.Document.Id.Find(documentId) ?? throw new Exception("not found");
        bool isOwner = doc.Owner == ctx.Sender;
        bool isShared = ctx.Db.DocumentShare.DocumentId.Filter(documentId).Any(s => s.SharedWith == ctx.Sender);
        if (!isOwner && !isShared)
        {
            throw new Exception("unauthorized");
        }
        ctx.Db.Document.Id.Update(doc with { Title = newTitle });
    }
}

using System;
using System.Linq;
using SpacetimeDB;

public static partial class Module
{
    [Table(Accessor = "PrivateDocument")]
    public partial struct PrivateDocument
    {
        [PrimaryKey]
        public ulong Id;
        public Identity Owner;
        public string Title;
        public string SecretBody;
    }

    [Table(Accessor = "ReadAccess")]
    public partial struct ReadAccess
    {
        [PrimaryKey]
        public Identity Reader;
        public bool Enabled;
    }

    [Reducer]
    public static void SetDocument(ReducerContext ctx, string title, string secretBody)
    {
        if (ctx.Db.PrivateDocument.Id.Find(1) is PrivateDocument row)
        {
            if (row.Owner != ctx.Sender)
                throw new Exception("owner only");
            row.Title = title;
            row.SecretBody = secretBody;
            ctx.Db.PrivateDocument.Id.Update(row);
        }
        else
            ctx.Db.PrivateDocument.Insert(
                new PrivateDocument
                {
                    Id = 1,
                    Owner = ctx.Sender,
                    Title = title,
                    SecretBody = secretBody,
                }
            );
    }

    [Reducer]
    public static void SetAccess(ReducerContext ctx, Identity reader, bool enabled)
    {
        var row = ctx.Db.PrivateDocument.Id.Find(1) ?? throw new Exception("owner only");
        if (row.Owner != ctx.Sender)
            throw new Exception("owner only");
        var access = new ReadAccess { Reader = reader, Enabled = enabled };
        if (ctx.Db.ReadAccess.Reader.Find(reader) is null)
            ctx.Db.ReadAccess.Insert(access);
        else
            ctx.Db.ReadAccess.Reader.Update(access);
    }

    [SpacetimeDB.Type]
    public partial struct SafeDocument
    {
        public ulong Id;
        public string Title;
    }

    [SpacetimeDB.View(Accessor = "VisibleDocument", Public = true)]
    public static System.Collections.Generic.IEnumerable<SafeDocument> VisibleDocument(
        ViewContext ctx
    )
    {
        var row = ctx.Db.PrivateDocument.Id.Find(1);
        var allowed = ctx.Db.ReadAccess.Reader.Find(ctx.Sender)?.Enabled ?? false;
        if (row is PrivateDocument document && (document.Owner == ctx.Sender || allowed))
            return new[]
            {
                new SafeDocument { Id = document.Id, Title = document.Title },
            };
        return Array.Empty<SafeDocument>();
    }
}

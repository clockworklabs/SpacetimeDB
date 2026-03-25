using SpacetimeDB;

public static partial class Module
{
    [Table(Accessor = "User")]
    public partial struct User
    {
        [PrimaryKey]
        public Identity Identity;
        public string Name;
    }

    [Table(Accessor = "Message", Public = true)]
    public partial struct Message
    {
        [PrimaryKey]
        [AutoInc]
        public ulong Id;
        [SpacetimeDB.Index.BTree]
        public Identity Sender;
        public string Text;
    }

    [Reducer]
    public static void Register(ReducerContext ctx, string name)
    {
        if (ctx.Db.User.Identity.Find(ctx.Sender) is not null)
        {
            throw new Exception("already registered");
        }
        ctx.Db.User.Insert(new User { Identity = ctx.Sender, Name = name });
    }

    [Reducer]
    public static void PostMessage(ReducerContext ctx, string text)
    {
        if (ctx.Db.User.Identity.Find(ctx.Sender) is null)
        {
            throw new Exception("not registered");
        }
        ctx.Db.Message.Insert(new Message { Id = 0, Sender = ctx.Sender, Text = text });
    }
}

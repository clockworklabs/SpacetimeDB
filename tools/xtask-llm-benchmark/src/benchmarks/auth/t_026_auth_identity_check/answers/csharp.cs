using SpacetimeDB;

public static partial class Module
{
    [Table(Accessor = "Message", Public = true)]
    public partial struct Message
    {
        [PrimaryKey]
        [AutoInc]
        public ulong Id;
        [SpacetimeDB.Index.BTree]
        public Identity Owner;
        public string Text;
    }

    [Reducer]
    public static void CreateMessage(ReducerContext ctx, string text)
    {
        ctx.Db.Message.Insert(new Message
        {
            Id = 0,
            Owner = ctx.Sender,
            Text = text,
        });
    }

    [Reducer]
    public static void DeleteMessage(ReducerContext ctx, ulong id)
    {
        var msg = ctx.Db.Message.Id.Find(id);
        if (msg is not Message m)
        {
            throw new Exception("not found");
        }
        if (m.Owner != ctx.Sender)
        {
            throw new Exception("unauthorized");
        }
        ctx.Db.Message.Id.Delete(id);
    }
}

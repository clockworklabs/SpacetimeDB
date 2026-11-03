namespace ChatModule;

using SpacetimeDB;

public static partial class Module
{
    [Table(Accessor = "Message", Public = true)]
    public partial class Message
    {
        public Identity Sender;
        public Timestamp Sent;
        public string Text = "";
    }

    [Reducer]
    public static void SendMessage(ReducerContext ctx, string text)
    {
        if (string.IsNullOrEmpty(text))
            throw new ArgumentException("Messages must not be empty");

        ctx.Db.Message.Insert(
            new Message
            {
                Sender = ctx.Sender,
                Sent = ctx.Timestamp,
                Text = text,
            }
        );
    }
}

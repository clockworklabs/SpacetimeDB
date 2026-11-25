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

    [View(Accessor = "MyMessages", Public = true)]
    public static IQuery<Message> MyMessages(ViewContext ctx) =>
        ctx.From.Message().Where(row => row.Sender.Eq(ctx.Sender));

    [Table(Accessor = "FetchFailure")]
    public partial class FetchFailure
    {
        [PrimaryKey, AutoInc]
        public ulong Id;
        public Timestamp At;
    }

    [Procedure]
    public static void CheckStatus(ProcedureContext ctx)
    {
        var ok = ctx
            .Http.Get("https://example.invalid/status")
            .Match(response => response.StatusCode is >= 200 and < 300, error => false);
        if (!ok)
            ctx.WithTx(tx =>
            {
                tx.Db.FetchFailure.Insert(new FetchFailure { At = tx.Timestamp });
                return true;
            });
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

using SpacetimeDB;
#pragma warning disable STDB_UNSTABLE

public static partial class Module
{
    [Table(Accessor = "ProcessedEvent")]
    public partial struct ProcessedEvent { [PrimaryKey] public string EventId; }

    [Table(Accessor = "WebhookState", Public = true)]
    public partial struct WebhookState { [PrimaryKey] public string Key; public ulong LastSequence; public string Value; }

    [SpacetimeDB.HttpHandler]
    public static HttpResponse Webhook(HandlerContext ctx, HttpRequest request)
    {
        var parts = request.Body.ToStringUtf8Lossy().Split('|', 3);
        if (parts.Length != 3) return new(400, HttpVersion.Http11, new(), HttpBody.FromString("invalid"));
        var eventId = parts[0]; var sequence = ulong.Parse(parts[1]); var value = parts[2];
        var outcome = ctx.WithTx(tx => {
            if (tx.Db.ProcessedEvent.EventId.Find(eventId) is not null) return "duplicate";
            tx.Db.ProcessedEvent.Insert(new ProcessedEvent { EventId = eventId });
            var state = tx.Db.WebhookState.Key.Find("account");
            if (state is not null) {
                if (sequence <= state.Value.LastSequence) return "stale";
                var row = state.Value; row.LastSequence = sequence; row.Value = value; tx.Db.WebhookState.Key.Update(row);
            } else tx.Db.WebhookState.Insert(new WebhookState { Key = "account", LastSequence = sequence, Value = value });
            return "applied";
        });
        return new(200, HttpVersion.Http11, new(), HttpBody.FromString(outcome));
    }

    [SpacetimeDB.HttpRouter]
    public static Router Routes() => SpacetimeDB.Router.New().Post("/webhook", Handlers.Webhook);
}

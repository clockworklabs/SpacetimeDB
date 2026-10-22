using System;
using System.Linq;
using SpacetimeDB;
#pragma warning disable STDB_UNSTABLE
public static partial class Module
{
    [Table(Accessor = "WebhookReceipt", Public = true)]
    public partial struct WebhookReceipt
    {
        [PrimaryKey]
        public string EventId;
        public string Account;
        public ulong Sequence;
        public string Value;
    }

    [Table(Accessor = "WebhookAccount", Public = true)]
    public partial struct WebhookAccount
    {
        [PrimaryKey]
        public string Account;
        public ulong Sequence;
        public string Value;
    }

    [SpacetimeDB.HttpHandler]
    public static HttpResponse Webhook(HandlerContext ctx, HttpRequest request)
    {
        var p = request.Body.ToStringUtf8Lossy().Split('|');
        if (
            p.Length != 4
            || p.Any(s => s.Length == 0)
            || p[2].Any(c => c < '0' || c > '9')
            || !ulong.TryParse(p[2], out var sequence)
            || sequence == 0
        )
            return new(400, HttpVersion.Http11, new(), HttpBody.FromString("invalid"));
        var account = p[0];
        var eventId = p[1];
        var value = p[3];
        var result = ctx.WithTx(tx =>
        {
            if (tx.Db.WebhookReceipt.EventId.Find(eventId) is WebhookReceipt old)
                return old.Account == account && old.Sequence == sequence && old.Value == value
                    ? "duplicate"
                    : "conflict";
            tx.Db.WebhookReceipt.Insert(
                new WebhookReceipt
                {
                    EventId = eventId,
                    Account = account,
                    Sequence = sequence,
                    Value = value,
                }
            );
            var next = new WebhookAccount
            {
                Account = account,
                Sequence = sequence,
                Value = value,
            };
            if (tx.Db.WebhookAccount.Account.Find(account) is WebhookAccount state)
            {
                if (sequence <= state.Sequence)
                    return "stale";
                tx.Db.WebhookAccount.Account.Update(next);
            }
            else
                tx.Db.WebhookAccount.Insert(next);
            return "applied";
        });
        return new(200, HttpVersion.Http11, new(), HttpBody.FromString(result));
    }

    [SpacetimeDB.HttpRouter]
    public static Router Routes() => SpacetimeDB.Router.New().Post("/webhook", Handlers.Webhook);
}

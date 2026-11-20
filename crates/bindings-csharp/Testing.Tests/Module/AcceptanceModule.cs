namespace ChatModule;

using SpacetimeDB;

public static partial class Module
{
    [Reducer(ReducerKind.ClientConnected)]
    public static void ClientConnected(ReducerContext ctx)
    {
        var jwt =
            ctx.SenderAuth.Jwt
            ?? throw new InvalidOperationException("A token is required to connect");
        if (jwt.Issuer != "https://auth.example.com")
            throw new InvalidOperationException("This token was issued by another provider");
        if (!jwt.Audience.Contains("my-app"))
            throw new InvalidOperationException("This token was issued for another application");
    }

    [Table(Accessor = "Edit")]
    public partial class Edit
    {
        [PrimaryKey]
        public ulong Id;
        public Timestamp EditedAt;
        public bool Live;
    }

    [Table(
        Accessor = "ExpireTimer",
        Scheduled = nameof(ExpireOldEdits),
        ScheduledAt = nameof(ScheduledAt)
    )]
    public partial struct ExpireTimer
    {
        [PrimaryKey, AutoInc]
        public ulong ScheduledId;
        public ScheduleAt ScheduledAt;
    }

    [Reducer]
    public static void ExpireOldEdits(ReducerContext ctx, ExpireTimer timer)
    {
        foreach (var edit in ctx.Db.Edit.Iter())
        {
            var age =
                ctx.Timestamp.MicrosecondsSinceUnixEpoch - edit.EditedAt.MicrosecondsSinceUnixEpoch;
            if (edit.Live && age > 15L * 60 * 1_000_000)
            {
                edit.Live = false;
                ctx.Db.Edit.Id.Update(edit);
            }
        }
    }
}

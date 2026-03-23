using SpacetimeDB;

public static partial class Module
{
    [Table(Accessor = "OnlinePlayer", Public = true)]
    public partial struct OnlinePlayer
    {
        [PrimaryKey]
        public Identity Identity;
        public Timestamp ConnectedAt;
    }

    [Reducer(ReducerKind.ClientConnected)]
    public static void ClientConnected(ReducerContext ctx)
    {
        ctx.Db.OnlinePlayer.Insert(new OnlinePlayer
        {
            Identity = ctx.Sender,
            ConnectedAt = ctx.Timestamp,
        });
    }

    [Reducer(ReducerKind.ClientDisconnected)]
    public static void ClientDisconnected(ReducerContext ctx)
    {
        ctx.Db.OnlinePlayer.Identity.Delete(ctx.Sender);
    }
}

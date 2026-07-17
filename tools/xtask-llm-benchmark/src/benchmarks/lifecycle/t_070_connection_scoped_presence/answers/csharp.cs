using SpacetimeDB;

public static partial class Module
{
    [Table(Accessor = "PresenceSession", Public = true)]
    public partial struct PresenceSession
    {
        [PrimaryKey] public ConnectionId ConnectionId;
        [SpacetimeDB.Index.BTree] public Identity Identity;
        public Timestamp ConnectedAt;
    }

    private static void AddSession(ReducerContext ctx, ConnectionId connectionId) =>
        ctx.Db.PresenceSession.Insert(new PresenceSession { ConnectionId = connectionId, Identity = ctx.Sender, ConnectedAt = ctx.Timestamp });

    private static void RemoveSession(ReducerContext ctx, ConnectionId connectionId) =>
        ctx.Db.PresenceSession.ConnectionId.Delete(connectionId);

    [Reducer(ReducerKind.ClientConnected)]
    public static void ClientConnected(ReducerContext ctx) => AddSession(ctx, ctx.ConnectionId ?? throw new InvalidOperationException("connection id missing"));

    [Reducer(ReducerKind.ClientDisconnected)]
    public static void ClientDisconnected(ReducerContext ctx) => RemoveSession(ctx, ctx.ConnectionId ?? throw new InvalidOperationException("connection id missing"));

    [Reducer]
    public static void ExercisePresence(ReducerContext ctx)
    {
        var firstBytes = new byte[16]; firstBytes[0] = 1;
        var secondBytes = new byte[16]; secondBytes[0] = 2;
        var first = ConnectionId.From(firstBytes) ?? throw new InvalidOperationException();
        var second = ConnectionId.From(secondBytes) ?? throw new InvalidOperationException();
        AddSession(ctx, first);
        AddSession(ctx, second);
        RemoveSession(ctx, first);
    }
}

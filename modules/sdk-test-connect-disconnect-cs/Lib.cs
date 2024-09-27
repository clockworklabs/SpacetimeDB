namespace SpacetimeDB.Sdk.Test.ConnectDisconnect;

using SpacetimeDB;

[SpacetimeDB.Table(Public = true)]
public partial struct Connected
{
    public Identity identity;
}

[SpacetimeDB.Table(Public = true)]
public partial struct Disconnected
{
    public Identity identity;
}

static partial class Module
{
    [SpacetimeDB.Reducer(ReducerKind.ClientConnected)]
    public static void Connected(ReducerContext ctx)
    {
        ctx.Db.Connected.Insert(new Connected { identity = ctx.CallerIdentity });
    }

    [SpacetimeDB.Reducer(ReducerKind.ClientDisconnected)]
    public static void Disconnected(ReducerContext ctx)
    {
        ctx.Db.Disconnected.Insert(new Disconnected { identity = ctx.CallerIdentity });
    }
}

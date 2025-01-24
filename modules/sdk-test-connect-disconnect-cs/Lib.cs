namespace SpacetimeDB.Sdk.Test.ConnectDisconnect;

using SpacetimeDB;

[SpacetimeDB.Table(Name = "connected", Public = true)]
public partial struct Connected
{
    public Identity identity;
}

[SpacetimeDB.Table(Name = "disconnected", Public = true)]
public partial struct Disconnected
{
    public Identity identity;
}

static partial class Module
{
    [SpacetimeDB.Reducer(ReducerKind.ClientConnected)]
    public static void identity_connected(ReducerContext ctx)
    {
        ctx.Db.connected.Insert(new Connected { identity = ctx.CallerIdentity });
    }

    [SpacetimeDB.Reducer(ReducerKind.ClientDisconnected)]
    public static void identity_disconnected(ReducerContext ctx)
    {
        ctx.Db.disconnected.Insert(new Disconnected { identity = ctx.CallerIdentity });
    }
}

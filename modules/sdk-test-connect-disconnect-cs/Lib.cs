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
    [SpacetimeDB.Reducer(ReducerKind.Connect)]
    public static void OnConnect(ReducerContext ctx)
    {
        ctx.Db.Connected.Insert(new Connected { identity = ctx.Sender });
    }

    [SpacetimeDB.Reducer(ReducerKind.Disconnect)]
    public static void OnDisconnect(ReducerContext ctx)
    {
        ctx.Db.Disconnected.Insert(new Disconnected { identity = ctx.Sender });
    }
}

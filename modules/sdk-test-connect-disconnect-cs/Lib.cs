namespace SpacetimeDB.Sdk.Test.ConnectDisconnect;

using SpacetimeDB;

[SpacetimeDB.Table(Public = true)]
public partial struct Connected {
    public Identity identity;
}

[SpacetimeDB.Table(Public = true)]
public partial struct Disconnected {
    public Identity identity;
}

static partial class Module
{
    [SpacetimeDB.Reducer(Name = ReducerKind.Connect)]
    public static void OnConnect(ReducerContext ctx)
    {
        var row = new Connected { identity = ctx.Sender };
        ctx.Db.Connected.Insert(ref row);
    }

    [SpacetimeDB.Reducer(Name = ReducerKind.Disconnect)]
    public static void OnDisconnect(ReducerContext ctx)
    {
        var row = new Disconnected { identity = ctx.Sender };
        ctx.Db.Disconnected.Insert(ref row);
    }
}

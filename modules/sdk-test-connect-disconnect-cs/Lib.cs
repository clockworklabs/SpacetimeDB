namespace SpacetimeDB.Sdk.Test.ConnectDisconnect;

using SpacetimeDB;

[Table(Public = true)]
public partial struct Connected {
    public Identity identity;
}

[Table(Public = true)]
public partial struct Disconnected {
    public Identity identity;
}

static partial class Module
{
    [Reducer(Name = ReducerKind.Connect)]
    public static void OnConnect(ReducerContext ctx) =>
        ctx.Db.Connected().Insert(new() { identity = ctx.Sender });

    [Reducer(Name = ReducerKind.Disconnect)]
    public static void OnDisconnect(ReducerContext ctx) =>
        ctx.Db.Disconnected().Insert(new() { identity = ctx.Sender });
}

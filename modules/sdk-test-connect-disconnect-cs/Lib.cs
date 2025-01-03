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
    // TODO: these method names shouldn't be special, but for now they have to match Rust snapshots.
    // See https://github.com/clockworklabs/SpacetimeDB/issues/1891.
    // For now, disable the error that would be raised due to those names starting with `__`.
#pragma warning disable STDB0009
    [SpacetimeDB.Reducer(ReducerKind.ClientConnected)]
    public static void __identity_connected__(ReducerContext ctx)
    {
        ctx.Db.connected.Insert(new Connected { identity = ctx.CallerIdentity });
    }

    [SpacetimeDB.Reducer(ReducerKind.ClientDisconnected)]
    public static void __identity_disconnected__(ReducerContext ctx)
    {
        ctx.Db.disconnected.Insert(new Disconnected { identity = ctx.CallerIdentity });
    }
#pragma warning restore STDB0009
}

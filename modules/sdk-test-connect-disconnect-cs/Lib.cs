using SpacetimeDB;

static partial class Module
{
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

    [SpacetimeDB.Reducer(ReducerKind.Connect)]
    public static void OnConnect(ReducerContext e)
    {
        new Connected { identity = e.Sender }.Insert();
    }

    [SpacetimeDB.Reducer(ReducerKind.Disconnect)]
    public static void OnDisconnect(ReducerContext e)
    {
        new Disconnected { identity = e.Sender }.Insert();
    }
}

using SpacetimeDB.Module;
using static SpacetimeDB.Runtime;

static partial class Module
{
    [SpacetimeDB.Table]
    public partial struct Connected
    {
        public Identity identity;
    }

    [SpacetimeDB.Table]
    public partial struct Disconnected
    {
        public Identity identity;
    }

    [SpacetimeDB.Reducer(ReducerKind.Connect)]
    public static void OnConnect(DbEventArgs e)
    {
        new Connected { identity = e.Sender }.Insert();
    }

    [SpacetimeDB.Reducer(ReducerKind.Disconnect)]
    public static void OnDisconnect(DbEventArgs e)
    {
        new Disconnected { identity = e.Sender }.Insert();
    }
}

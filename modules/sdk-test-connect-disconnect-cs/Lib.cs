using SpacetimeDB.Module;
using static SpacetimeDB.Runtime;
using System.Runtime.CompilerServices;

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

    [ModuleInitializer]
    public static void Init()
    {
        OnConnect += (e) =>
        {
            new Connected { identity = e.Sender }.Insert();
        };

        OnDisconnect += (e) =>
        {
            new Disconnected { identity = e.Sender }.Insert();
        };
    }

    [SpacetimeDB.Reducer]
    /// Due to a bug in SATS' `derive(Desrialize)`
    /// https://github.com/clockworklabs/SpacetimeDB/issues/325 ,
    /// Rust module bindings fail to compile for modules which define zero reducers
    /// (not counting init, update, connect, disconnect).
    /// Adding this useless empty reducer causes the module bindings to compile.
    public static void UselessEmptyReducer() {}
}

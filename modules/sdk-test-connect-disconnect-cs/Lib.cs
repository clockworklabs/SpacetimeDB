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
}

using System.Runtime.CompilerServices;
using SpacetimeDB;

public static partial class Dependency
{
    [MethodImpl(MethodImplOptions.NoInlining)]
    public static int Answer() => 42;

    [Reducer]
    public static void DependencyEntry(ReducerContext ctx)
    {
        Log.Info("dependency published as root");
    }
}

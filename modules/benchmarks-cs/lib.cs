using System.Runtime.CompilerServices;

namespace Benchmarks;

public static class Bench
{
    public static void BlackBox<T>(IEnumerable<T> input)
    {
        foreach (T? item in input)
        {
            BlackBox(item);
        }
    }

    [MethodImpl(MethodImplOptions.NoInlining)]
    public static void BlackBox<T>(T input)
    {
        _ = input;
    }
}

[SpacetimeDB.Type]
public partial struct Load(uint initial_load)
{
    public uint initial_load = initial_load;
    public uint small_table = initial_load;
    public uint num_players = initial_load;
    public uint big_table = initial_load * 50;
    public uint biggest_table = initial_load * 100;
}
